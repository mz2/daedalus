//! Tasks board (home, design/README.md IA decision): the aggregate of what every agent is
//! working on at the task level — the landing view.

use daedalus_app::{App, AppQuery};
use daedalus_proto::SessionId;

use crate::components::StatusBadge;
use crate::theme::Theme;

/// One task across the fleet, attributed to its session.
#[derive(Debug, Clone)]
pub struct AggregateTask {
    /// Owning session.
    pub session: SessionId,
    /// Tool name (for context).
    pub tool: String,
    /// Task id (e.g. T001).
    pub task_id: String,
    /// Description.
    pub description: String,
    /// Status badge.
    pub badge: StatusBadge,
}

/// The aggregate tasks board.
#[derive(Debug, Clone)]
pub struct TasksBoardView {
    /// Every tracked task across non-terminal sessions.
    pub tasks: Vec<AggregateTask>,
}

impl TasksBoardView {
    /// Build the aggregate board from current state.
    #[must_use]
    pub fn build(app: &App, theme: &Theme) -> Self {
        let mut tasks = Vec::new();
        for summary in app.fleet() {
            if summary.status.is_terminal() {
                continue;
            }
            for task in app.task_board(summary.id) {
                tasks.push(AggregateTask {
                    session: summary.id,
                    tool: summary.tool_name.clone(),
                    task_id: task.id.0.clone(),
                    description: task.description.clone(),
                    badge: StatusBadge::task(task.status, theme),
                });
            }
        }
        Self { tasks }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn aggregates_tasks_across_sessions() {
        let fx = daedalus_tests::Fixture::new();
        let tool = fx.register_sample_tool("claude");
        let req = fx.fresh_request(tool);
        let objective = req.objective.clone();
        fx.write_tasks(&objective, "- [ ] T001 Implement\n- [x] T002 Setup\n");
        let id = fx.core.start_session(req).await.unwrap();
        fx.core.refresh_task_board(id).unwrap();

        let view = TasksBoardView::build(&fx.app, &Theme::host_default());
        assert_eq!(view.tasks.len(), 2);
        assert!(view.tasks.iter().all(|t| t.tool == "claude"));
    }
}
