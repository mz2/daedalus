//! Fleet screen (US5, FR-025): the session-level unified view.

use daedalus_app::{App, AppQuery};
use daedalus_proto::{BackendKind, Origin, SessionId};

use crate::components::StatusBadge;
use crate::theme::Theme;

/// One row in the fleet table.
#[derive(Debug, Clone)]
pub struct FleetRow {
    /// Session id.
    pub id: SessionId,
    /// Tool name.
    pub tool: String,
    /// Objective summary.
    pub objective: String,
    /// Backend kind.
    pub backend: BackendKind,
    /// Environment origin.
    pub origin: Origin,
    /// Status badge.
    pub badge: StatusBadge,
}

/// The fleet view.
#[derive(Debug, Clone)]
pub struct FleetView {
    /// All session rows, newest first.
    pub rows: Vec<FleetRow>,
}

impl FleetView {
    /// Build from current state.
    #[must_use]
    pub fn build(app: &App, theme: &Theme) -> Self {
        let rows = app
            .fleet()
            .into_iter()
            .map(|s| FleetRow {
                id: s.id,
                tool: s.tool_name,
                objective: s.objective,
                backend: s.backend,
                origin: s.origin,
                badge: StatusBadge::session(s.status, theme),
            })
            .collect();
        Self { rows }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    #[tokio::test]
    async fn fleet_view_rows_carry_status_badges() {
        let fx = daedalus_tests::Fixture::new();
        let tool = fx.register_sample_tool("claude");
        fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
        let view = FleetView::build(&fx.app, &Theme::host_default());
        assert_eq!(view.rows.len(), 1);
        assert_eq!(view.rows[0].tool, "claude");
    }
}
