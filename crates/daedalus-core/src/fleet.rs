//! The unified fleet query: one view of every session across all sources/backends (FR-025).

use daedalus_proto::SessionSummary;

use crate::{Core, CoreError};

impl Core {
    /// A compact summary of every session — identity, tool, objective, backend, env origin,
    /// and status — for the fleet view.
    pub fn fleet(&self) -> Result<Vec<SessionSummary>, CoreError> {
        let sessions = self.store.list_sessions()?;
        let mut out = Vec::with_capacity(sessions.len());
        for s in sessions {
            let ctx = self.session_context(&s);
            let tasks = self.store.list_tasks(s.id).unwrap_or_default();
            let tasks_done = tasks
                .iter()
                .filter(|t| t.status == daedalus_proto::TaskStatus::Done)
                .count();
            let tasks_total = tasks.len();
            out.push(SessionSummary {
                id: s.id,
                tool_name: ctx.tool_name,
                objective: ctx.objective,
                backend: ctx.backend,
                origin: ctx.origin,
                status: s.status,
                accepts_input: s.accepts_input,
                // The fleet row's spec sub-line shows the tracked tasks file.
                spec: ctx.tasks_file,
                tasks_done,
                tasks_total,
            });
        }
        Ok(out)
    }
}
