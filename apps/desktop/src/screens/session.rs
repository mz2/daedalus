//! Session detail screen (US2/US4, §6.4): embedded terminal + task board + telemetry +
//! lifecycle controls. The control set encodes the v1 capability rules — Send-input is
//! disabled with a reason when the tool does not accept it; Confirm appears only when the
//! session is awaiting confirmation; there is **no** pause/resume.

use daedalus_app::{App, AppQuery};
use daedalus_proto::{SessionDetail, SessionId, SessionStatus, TaskStatus};

use crate::components::{ActionButton, ButtonIntent, Meter, StatusBadge};
use crate::theme::Theme;

/// A row on the per-session task board.
#[derive(Debug, Clone)]
pub struct TaskRow {
    /// Task id (e.g. T001).
    pub id: String,
    /// Description.
    pub description: String,
    /// Status badge.
    pub badge: StatusBadge,
}

/// A labelled telemetry meter.
#[derive(Debug, Clone)]
pub struct TelemetryRow {
    /// Metric label.
    pub label: String,
    /// Meter (value/max).
    pub meter: Meter,
    /// Human value text.
    pub value_text: String,
}

/// The session detail view.
#[derive(Debug, Clone)]
pub struct SessionView {
    /// The session id.
    pub id: SessionId,
    /// Status badge for the header.
    pub badge: StatusBadge,
    /// Tool name.
    pub tool: String,
    /// Objective summary.
    pub objective: String,
    /// Task board rows.
    pub board: Vec<TaskRow>,
    /// Telemetry rail.
    pub telemetry: Vec<TelemetryRow>,
    /// Lifecycle controls in header order.
    pub controls: Vec<ActionButton>,
}

impl SessionView {
    /// Build from a session detail.
    #[must_use]
    pub fn build(app: &App, id: SessionId, theme: &Theme) -> Option<Self> {
        let detail = app.session(id)?;
        Some(Self::from_detail(detail, theme))
    }

    /// Build directly from a [`SessionDetail`].
    #[must_use]
    pub fn from_detail(detail: SessionDetail, theme: &Theme) -> Self {
        let board = detail
            .tasks
            .iter()
            .map(|t| TaskRow {
                id: t.id.0.clone(),
                description: t.description.clone(),
                badge: StatusBadge::task(t.status, theme),
            })
            .collect();

        let telemetry = detail
            .metrics
            .iter()
            .map(|m| TelemetryRow {
                label: format!("{:?}", m.metric),
                meter: Meter {
                    value: m.value,
                    max: m.value.max(1.0),
                },
                value_text: format!("{:.0}", m.value),
            })
            .collect();

        let controls = Self::controls(&detail, theme);

        Self {
            id: detail.session.id,
            badge: StatusBadge::session(detail.session.status, theme),
            tool: detail.tool.name,
            objective: detail.objective.description,
            board,
            telemetry,
            controls,
        }
    }

    fn controls(detail: &SessionDetail, _theme: &Theme) -> Vec<ActionButton> {
        let mut controls = Vec::new();
        let running = !detail.session.status.is_terminal();

        if running {
            controls.push(ActionButton::enabled("Stop", ButtonIntent::Danger));
        }

        // Send-input gated on the tool capability (FR-023); disabled-with-reason otherwise.
        if detail.session.accepts_input {
            controls.push(ActionButton::enabled("Send input", ButtonIntent::Tinted));
        } else {
            controls.push(ActionButton::disabled(
                "Send input",
                ButtonIntent::Tinted,
                "this tool does not accept interactive input",
            ));
        }

        // Confirm completion only when awaiting (FR-015a).
        if detail.session.status == SessionStatus::AwaitingConfirmation {
            controls.push(ActionButton::enabled(
                "Confirm completion",
                ButtonIntent::Primary,
            ));
        }

        controls.push(ActionButton::enabled("Clean up", ButtonIntent::Ghost));
        controls
    }

    /// Whether all tasks are done (drives the completed affordance).
    #[must_use]
    pub fn all_tasks_done(&self) -> bool {
        !self.board.is_empty()
            && self
                .board
                .iter()
                .all(|r| r.badge.tone == crate::theme::StatusTone::from_task(TaskStatus::Done))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn awaiting_session_shows_confirm_and_disabled_input_when_unsupported() {
        let fx = daedalus_tests::Fixture::new();
        // A tool that does NOT accept input.
        let tool = fx
            .core
            .register_tool(daedalus_proto::ToolDef {
                name: "batch".into(),
                invocation: daedalus_proto::InvocationSpec {
                    program: "batch".into(),
                    args: vec![],
                    env: vec![],
                },
                capabilities: daedalus_proto::Capabilities {
                    accepts_interactive_input: false,
                },
            })
            .unwrap();
        let req = fx.fresh_request(tool);
        let objective = req.objective.clone();
        fx.write_tasks(&objective, "- [ ] T001 Implement\n");
        let id = fx.core.start_session(req).await.unwrap();
        fx.core
            .apply_signal(id, daedalus_core::AgentSignal::ExitedCleanly)
            .await
            .unwrap();

        let view = SessionView::build(&fx.app, id, &Theme::host_default()).unwrap();
        assert!(view
            .controls
            .iter()
            .any(|c| c.label == "Confirm completion" && c.enabled));
        let send = view
            .controls
            .iter()
            .find(|c| c.label == "Send input")
            .unwrap();
        assert!(!send.enabled);
        assert!(send.disabled_reason.is_some());
    }
}
