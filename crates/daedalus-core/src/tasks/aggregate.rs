//! The aggregate tasks query (FR-025a): every tracked task across ALL sessions, each row
//! carrying its session context (objective/status/tool/backend) and an attention flag when
//! the session is blocked on the operator. This backs the application's landing view — the
//! task is the unit; the session becomes a field (design `tasks.jsx` / `data.js allTasks()`).

use daedalus_proto::{AggregateTask, AttentionKind, BackendKind, TaskStatus};

use crate::{Core, CoreError};

/// Board group order (design `tasks.jsx` TASK_COLS): In progress, Blocked, To do, Done.
fn group_rank(status: TaskStatus) -> u8 {
    match status {
        TaskStatus::InProgress => 0,
        TaskStatus::Blocked => 1,
        TaskStatus::Todo => 2,
        TaskStatus::Done => 3,
    }
}

impl Core {
    /// Every tracked task across all sessions, grouped by task status (In progress /
    /// Blocked / To do / Done) with a stable order inside each group: session recency,
    /// then task id. Filtering (status/session/tool/backend) is a view concern —
    /// [`daedalus_proto::TasksFilter::matches`] applies over these rows.
    pub fn all_tasks(&self) -> Result<Vec<AggregateTask>, CoreError> {
        let mut rows = Vec::new();
        for session in self.store.list_sessions()? {
            let tool = self
                .store
                .get_tool(session.tool_id)
                .map(|t| t.name)
                .unwrap_or_else(|_| "(unknown tool)".to_string());
            let (objective, spec) = self
                .store
                .get_objective(session.objective_id)
                .map(|o| (o.description, o.artifact_ref.root))
                .unwrap_or_default();
            let backend = self
                .store
                .get_environment(session.environment_id)
                .ok()
                .and_then(|e| self.backends.by_id(e.backend_id))
                .map(|b| b.kind())
                .unwrap_or(BackendKind::Fake);
            let attention = AttentionKind::from_status(session.status);
            for task in self.store.list_tasks(session.id)? {
                rows.push(AggregateTask {
                    session: task.session_id,
                    task,
                    objective: objective.clone(),
                    session_status: session.status,
                    spec: spec.clone(),
                    tool: tool.clone(),
                    backend,
                    attention,
                });
            }
        }
        // Stable: preserves session order (newest first) then task-id order inside groups.
        rows.sort_by_key(|r| group_rank(r.task.status));
        Ok(rows)
    }
}
