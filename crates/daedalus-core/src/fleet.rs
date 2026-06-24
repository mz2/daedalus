//! The unified fleet query: one view of every session across all sources/backends (FR-025).

use daedalus_proto::{BackendKind, SessionSummary};

use crate::{Core, CoreError};

impl Core {
    /// A compact summary of every session — identity, tool, objective, backend, env origin,
    /// and status — for the fleet view.
    pub fn fleet(&self) -> Result<Vec<SessionSummary>, CoreError> {
        let sessions = self.store.list_sessions()?;
        let mut out = Vec::with_capacity(sessions.len());
        for s in sessions {
            let tool_name = self
                .store
                .get_tool(s.tool_id)
                .map(|t| t.name)
                .unwrap_or_else(|_| "(unknown tool)".to_string());
            let objective = self
                .store
                .get_objective(s.objective_id)
                .map(|o| o.description)
                .unwrap_or_default();
            let env = self.store.get_environment(s.environment_id).ok();
            let origin = env
                .as_ref()
                .map(|e| e.origin)
                .unwrap_or(daedalus_proto::Origin::Fresh);
            let backend = env
                .and_then(|e| self.backends.by_id(e.backend_id))
                .map(|b| b.kind())
                .unwrap_or(BackendKind::Fake);
            out.push(SessionSummary {
                id: s.id,
                tool_name,
                objective,
                backend,
                origin,
                status: s.status,
                accepts_input: s.accepts_input,
            });
        }
        Ok(out)
    }
}
