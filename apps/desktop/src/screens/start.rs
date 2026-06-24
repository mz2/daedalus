//! Start-session screen (US1, §6.3): tool pick → SDD objective ref → env choice (with the
//! worktree note for pre-existing) → backend choice → launch, with validation.

use daedalus_app::App;
use daedalus_proto::{BackendKind, BackendStatus, Origin, StartSessionRequest, ToolId};

/// A tool option in the picker.
#[derive(Debug, Clone)]
pub struct ToolOption {
    /// Tool id.
    pub id: ToolId,
    /// Display name.
    pub name: String,
    /// Whether it accepts input (shown as a capability chip).
    pub accepts_input: bool,
}

/// The start-session form view-model.
#[derive(Debug, Clone)]
pub struct StartScreen {
    /// Registered tools to choose from.
    pub tools: Vec<ToolOption>,
    /// Backends and their availability (unavailable ones are shown disabled).
    pub backends: Vec<BackendStatus>,
}

impl StartScreen {
    /// Build the form from current state.
    pub async fn build(app: &App) -> Self {
        let tools = app
            .core()
            .list_tools()
            .unwrap_or_default()
            .into_iter()
            .map(|t| ToolOption {
                id: t.id,
                name: t.name,
                accepts_input: t.capabilities.accepts_interactive_input,
            })
            .collect();
        let backends = app.core().backends_status().await;
        Self { tools, backends }
    }

    /// Validate a draft request before launch, returning the worktree note / errors the UI
    /// must surface (pre-existing requires a worktree — FR-002a).
    pub fn validate(&self, req: &StartSessionRequest) -> Result<(), String> {
        if !self.tools.iter().any(|t| t.id == req.tool_id) {
            return Err("select a registered tool".into());
        }
        if req.objective.artifact_ref.tasks_file.trim().is_empty() {
            return Err("choose an SDD objective (tasks.md) to track".into());
        }
        if req.origin == Origin::PreExisting && req.worktree.is_none() {
            return Err("a pre-existing environment needs an isolated git worktree".into());
        }
        if !self.backends.iter().any(|b| {
            b.kind == req.backend && b.availability != daedalus_proto::Availability::Unavailable
        }) {
            return Err("the selected backend is unavailable".into());
        }
        Ok(())
    }

    /// The available (non-unavailable) backend kinds.
    #[must_use]
    pub fn available_backends(&self) -> Vec<BackendKind> {
        self.backends
            .iter()
            .filter(|b| b.availability != daedalus_proto::Availability::Unavailable)
            .map(|b| b.kind)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn validate_rejects_preexisting_without_worktree() {
        let fx = daedalus_tests::Fixture::new();
        let tool = fx.register_sample_tool("claude");
        let screen = StartScreen::build(&fx.app).await;
        let mut req = fx.fresh_request(tool);
        req.origin = Origin::PreExisting;
        req.worktree = None;
        assert!(screen.validate(&req).is_err());
    }
}
