//! The task-status board: reads the session's SDD artifacts (e.g. SpecKit `tasks.md`
//! checkbox state) and reflects changes onto persisted tracked tasks (FR-017, Principle IV).
//! Status is read from the artifact, never inferred from output.

use std::collections::HashMap;

use daedalus_sdk::parse_task_states;

use daedalus_proto::{AppEvent, SessionId, TaskId, TaskStatus, TrackedTask};

use crate::session::state::{transition, Trigger};
use crate::{clock, map_not_found, Core, CoreError, StoreError};

impl Core {
    /// Re-read the session's `tasks.md`, persist any changed/added task, emit
    /// [`AppEvent::TaskStatusChanged`] for each change, and — if the session is running and
    /// **all** tracked tasks are now done — complete it (FR-015a).
    pub fn refresh_task_board(&self, id: SessionId) -> Result<Vec<TrackedTask>, CoreError> {
        let session = self.store.get_session(id).map_err(map_not_found)?;
        let objective = self.store.get_objective(session.objective_id)?;

        let content = std::fs::read_to_string(&objective.artifact_ref.tasks_file)
            .map_err(|e| CoreError::Store(StoreError::Io(e)))?;
        let parsed = parse_task_states(&content, id, clock::now());

        let previous: HashMap<TaskId, TaskStatus> = self
            .store
            .list_tasks(id)?
            .into_iter()
            .map(|t| (t.id, t.status))
            .collect();

        for task in &parsed {
            let changed = previous.get(&task.id) != Some(&task.status);
            if changed {
                self.store.upsert_task(task)?;
                self.emit(AppEvent::TaskStatusChanged {
                    id,
                    task: task.id.clone(),
                    status: task.status,
                });
            }
        }

        // Completion only on all-tasks-done (never agent-exit alone) — FR-015a. The state
        // machine is the sole arbiter of which statuses may complete: `AllTasksDone` is
        // legal from every non-terminal attention state (Running, Stalled, WaitingForInput,
        // AwaitingConfirmation) and rejected from Starting / terminal / Unknown, so a
        // session that finishes its tasks while waiting still completes and a `Starting`
        // (or already-terminal) session is left untouched.
        let all_done = !parsed.is_empty() && parsed.iter().all(|t| t.status == TaskStatus::Done);
        if all_done {
            if let Ok(next) = transition(session.status, Trigger::AllTasksDone) {
                self.store
                    .set_session_state(id, next, Some(clock::now()), None, None, None)?;
                self.record_lifecycle(id, next, None);
            }
        }

        self.store.list_tasks(id).map_err(Into::into)
    }
}
