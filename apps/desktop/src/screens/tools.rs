//! Tools registry screen (§6.6, FR-001a): the operator-registered agentic tools, the
//! first-run empty state, and the register-modal validation — copy verbatim from the
//! prototype `Tools`/`ToolRegisterModal` (`design/prototype/screens.jsx`).

use daedalus_app::App;
use daedalus_proto::ToolId;

use crate::components::{ActionButton, ButtonIntent, EmptyState, FieldError};

/// The success line after a dry-check passes (prototype "Validate definition").
pub const COMMAND_VALID_NOTE: &str = "Command found in sandbox PATH — definition looks valid.";

/// Validate the tool name (prototype `nameError`): required, and unique against the
/// registry (case-insensitive) — FR-001a.
#[must_use]
pub fn name_error(name: &str, existing: &[String]) -> Option<FieldError> {
    let t = name.trim();
    if t.is_empty() {
        return Some(FieldError::plain("Name is required."));
    }
    if existing.iter().any(|n| n.eq_ignore_ascii_case(t)) {
        return Some(FieldError::plain(format!(
            "A tool named '{t}' is already registered."
        )));
    }
    None
}

/// Validate the launch command (prototype `invokeError`): required, balanced quotes, and
/// the program must exist on the sandbox image — the raw "command not found in sandbox
/// PATH" error renders monospace (FR-001a).
#[must_use]
pub fn launch_command_error(command: &str, sandbox_commands: &[&str]) -> Option<FieldError> {
    let t = command.trim();
    if t.is_empty() {
        return Some(FieldError::plain("Launch command is required."));
    }
    if t.matches('"').count() % 2 == 1 {
        return Some(FieldError::plain(
            "Unbalanced quotes — arguments with spaces must be quoted.",
        ));
    }
    let cmd = t.split_whitespace().next().unwrap_or_default();
    if !sandbox_commands.contains(&cmd) {
        return Some(FieldError::mono(format!(
            "{cmd}: command not found in sandbox PATH"
        )));
    }
    None
}

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

    /// The first-run empty state (prototype Tools `empty`, FR-001a): what a tool
    /// definition is, with Register as the single action.
    #[must_use]
    pub fn empty_state(&self) -> Option<EmptyState> {
        self.rows.is_empty().then(|| EmptyState {
            icon: "tools",
            title: "Register your first agentic tool",
            body: "A tool is a declarative definition of how to launch an agent inside a \
                   sandbox — its launch command, version, and whether it accepts input. \
                   Daedalus tracks progress from tasks.md and session output; no per-tool \
                   integration.",
            actions: vec![ActionButton::enabled(
                "Register tool",
                ButtonIntent::Primary,
            )],
        })
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
        assert!(view.empty_state().is_none());
    }

    #[tokio::test]
    async fn first_run_empty_state_explains_tool_definitions() {
        // FR-001a (T085): the prototype's first-run copy, verbatim.
        let fx = daedalus_tests::Fixture::new();
        let view = ToolsView::build(&fx.app);
        let empty = view.empty_state().expect("no tools yet");
        assert_eq!(empty.title, "Register your first agentic tool");
        assert_eq!(
            empty.body,
            "A tool is a declarative definition of how to launch an agent inside a \
             sandbox — its launch command, version, and whether it accepts input. \
             Daedalus tracks progress from tasks.md and session output; no per-tool \
             integration."
        );
        assert_eq!(empty.actions[0].label, "Register tool");
    }

    #[test]
    fn register_modal_name_validation_matches_the_prototype() {
        // Prototype `nameError` (T085): required + case-insensitive duplicate.
        let existing = vec!["Claude Code".to_string()];
        assert_eq!(
            name_error("  ", &existing).unwrap().text,
            "Name is required."
        );
        assert_eq!(
            name_error("claude code", &existing).unwrap().text,
            "A tool named 'claude code' is already registered."
        );
        assert_eq!(name_error("Aider", &existing), None);
    }

    #[test]
    fn register_modal_launch_command_validation_matches_the_prototype() {
        // Prototype `invokeError` (T085): required, balanced quotes, and the mono
        // "command not found in sandbox PATH" for anything off the sandbox image.
        let sandbox = ["claude", "agy", "opencode"];
        assert_eq!(
            launch_command_error("", &sandbox).unwrap().text,
            "Launch command is required."
        );
        assert_eq!(
            launch_command_error("claude -p \"/speckit.implement", &sandbox)
                .unwrap()
                .text,
            "Unbalanced quotes — arguments with spaces must be quoted."
        );

        // The classic typo the prototype's Validate-definition seeds.
        let err =
            launch_command_error("claude--dangerously \"/speckit.implement\"", &sandbox).unwrap();
        assert_eq!(
            err.text,
            "claude--dangerously: command not found in sandbox PATH"
        );
        assert!(err.mono, "raw command output renders monospace");

        assert_eq!(
            launch_command_error("agy -p \"/speckit.implement\"", &sandbox),
            None
        );
        assert_eq!(
            COMMAND_VALID_NOTE,
            "Command found in sandbox PATH — definition looks valid."
        );
    }
}
