//! Tools registry screen (§6.6, FR-001a): the operator-registered agentic tools.

use daedalus_app::App;
use daedalus_proto::ToolId;

/// A tool row.
#[derive(Debug, Clone)]
pub struct ToolRow {
    /// Tool id.
    pub id: ToolId,
    /// Name.
    pub name: String,
    /// The launch command preview (`program args…`).
    pub command: String,
    /// Whether it accepts interactive input.
    pub accepts_input: bool,
}

/// The tools view.
#[derive(Debug, Clone)]
pub struct ToolsView {
    /// Registered tools.
    pub rows: Vec<ToolRow>,
}

impl ToolsView {
    /// Build from the registry.
    #[must_use]
    pub fn build(app: &App) -> Self {
        let rows = app
            .core()
            .list_tools()
            .unwrap_or_default()
            .into_iter()
            .map(|t| {
                let mut command = t.invocation.program.clone();
                for arg in &t.invocation.args {
                    command.push(' ');
                    command.push_str(arg);
                }
                ToolRow {
                    id: t.id,
                    name: t.name,
                    command,
                    accepts_input: t.capabilities.accepts_interactive_input,
                }
            })
            .collect();
        Self { rows }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn lists_registered_tools_with_command_preview() {
        let fx = daedalus_tests::Fixture::new();
        fx.register_sample_tool("claude");
        let view = ToolsView::build(&fx.app);
        assert_eq!(view.rows.len(), 1);
        assert_eq!(view.rows[0].command, "echo hello");
    }
}
