//! Fleet screen (US5, FR-025): the session-level unified view — attention-first ordering,
//! the harmonized row sub-line (spec branch · tool · backend), inline needs-tags, the
//! full status filter set, and the first-run empty state (prototype `fleet.jsx`).

use daedalus_app::{App, AppQuery, AppQueryAsync};
use daedalus_proto::{AttentionKind, Availability, BackendKind, Origin, SessionId, SessionStatus};

use crate::components::{ActionButton, ButtonIntent, Chip, EmptyState, StatusBadge};
use crate::screens::needs::kind_tag;
use crate::theme::Theme;

/// The status filter chip set, in prototype order (`STATUS_FILTERS` in `fleet.jsx`) —
/// including the purple attention pair and the unknown state.
pub const STATUS_FILTERS: [SessionStatus; 8] = [
    SessionStatus::Running,
    SessionStatus::WaitingForInput,
    SessionStatus::AwaitingConfirmation,
    SessionStatus::Stalled,
    SessionStatus::Failed,
    SessionStatus::Completed,
    SessionStatus::Stopped,
    SessionStatus::Unknown,
];

/// The attention kind a session status maps to, when it is blocked on the operator —
/// drives the inline [`NeedsTag`](crate::screens::needs::kind_tag) (prototype `needsMeta`).
#[must_use]
pub fn attention_kind(status: SessionStatus) -> Option<AttentionKind> {
    match status {
        SessionStatus::WaitingForInput => Some(AttentionKind::WaitingForInput),
        SessionStatus::AwaitingConfirmation => Some(AttentionKind::AwaitingConfirmation),
        SessionStatus::Stalled => Some(AttentionKind::Stalled),
        SessionStatus::Failed => Some(AttentionKind::Failed),
        SessionStatus::Unknown => Some(AttentionKind::Disconnected),
        _ => None,
    }
}

/// The attention sort rank (prototype `order`): blocked-on-you first, ended last.
#[must_use]
pub fn attention_rank(status: SessionStatus) -> u8 {
    match status {
        SessionStatus::WaitingForInput => 0,
        SessionStatus::AwaitingConfirmation => 1,
        SessionStatus::Stalled => 2,
        SessionStatus::Failed => 3,
        SessionStatus::Unknown => 4,
        SessionStatus::Starting => 5,
        SessionStatus::Running => 6,
        SessionStatus::Completed => 7,
        SessionStatus::Stopped => 8,
    }
}

/// One row in the fleet table.
#[derive(Debug, Clone)]
pub struct FleetRow {
    /// Session id.
    pub id: SessionId,
    /// Tool name.
    pub tool: String,
    /// Objective summary.
    pub objective: String,
    /// The spec/branch sub-line entry (`row-branch` — the tracked SDD artifact ref).
    pub spec: String,
    /// Backend kind.
    pub backend: BackendKind,
    /// The backend chip — availability dot ONLY when the hosting backend is not
    /// available (FR-028, T083).
    pub backend_chip: Chip,
    /// Environment origin.
    pub origin: Origin,
    /// Current status (drives sorting, filtering, and the needs-tag).
    pub status: SessionStatus,
    /// Status badge.
    pub badge: StatusBadge,
    /// Inline needs-tag cue when the session is blocked on the operator
    /// (prototype `NeedsTag`: "Asked a question", "Confirm completion", …).
    pub needs_tag: Option<&'static str>,
    /// Tracked tasks done.
    pub tasks_done: usize,
    /// Tracked tasks total.
    pub tasks_total: usize,
}

impl FleetRow {
    /// The progress readout (prototype `ProgressPill`), e.g. "4/9".
    #[must_use]
    pub fn progress_label(&self) -> String {
        format!("{}/{}", self.tasks_done, self.tasks_total)
    }
}

/// The fleet view.
#[derive(Debug, Clone)]
pub struct FleetView {
    /// All session rows, attention-first (prototype default sort: awaiting → confirm →
    /// stalled → failed → disconnected → starting → running → ended).
    pub rows: Vec<FleetRow>,
}

impl FleetView {
    /// Build from current state (including live backend availability for the chips).
    pub async fn build(app: &App, theme: &Theme) -> Self {
        let backends = app.backends().await;
        let availability_of = |kind: BackendKind| {
            backends
                .iter()
                .filter(|b| b.kind == kind)
                .map(|b| b.availability)
                .max_by_key(|a| match a {
                    Availability::Available => 0,
                    Availability::Degraded => 1,
                    Availability::Unavailable => 2,
                })
                .unwrap_or(Availability::Available)
        };
        let mut rows: Vec<FleetRow> = app
            .fleet()
            .into_iter()
            .map(|s| FleetRow {
                id: s.id,
                tool: s.tool_name,
                objective: s.objective,
                spec: s.spec,
                backend: s.backend,
                backend_chip: Chip::backend(s.backend, availability_of(s.backend), theme),
                origin: s.origin,
                status: s.status,
                badge: StatusBadge::session(s.status, theme),
                needs_tag: attention_kind(s.status).map(kind_tag),
                tasks_done: s.tasks_done,
                tasks_total: s.tasks_total,
            })
            .collect();
        rows.sort_by_key(|r| attention_rank(r.status));
        Self { rows }
    }

    /// The first-run empty state (prototype Fleet `demoState = "empty"`).
    #[must_use]
    pub fn empty_state(&self) -> Option<EmptyState> {
        self.rows.is_empty().then(|| EmptyState {
            icon: "fleet",
            title: "No sessions yet",
            body: "Start an agent against an objective, or discover sessions running on \
                   other hosts. Daedalus keeps every run isolated in its own sandbox.",
            actions: vec![
                ActionButton::enabled("Start a session", ButtonIntent::Primary),
                ActionButton::enabled("Discover", ButtonIntent::Ghost),
            ],
        })
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
        let view = FleetView::build(&fx.app, &Theme::host_default()).await;
        assert_eq!(view.rows.len(), 1);
        assert_eq!(view.rows[0].tool, "claude");
    }

    #[tokio::test]
    async fn fleet_backend_chips_flag_degraded_availability_with_a_dot() {
        // FR-028 (T083): the fleet's backend chip carries the availability dot ONLY when
        // the hosting backend is not available.
        use daedalus_proto::Availability;

        let fx = daedalus_tests::Fixture::new();
        let tool = fx.register_sample_tool("claude");
        fx.core.start_session(fx.fresh_request(tool)).await.unwrap();

        let view = FleetView::build(&fx.app, &Theme::host_default()).await;
        assert!(view.rows[0].backend_chip.dot.is_none(), "healthy ⇒ no dot");

        fx.backend
            .set_availability_with_reason(Availability::Degraded, Some("resource pressure"));
        let view = FleetView::build(&fx.app, &Theme::host_default()).await;
        assert!(view.rows[0].backend_chip.dot.is_some());
        assert!(view.rows[0]
            .backend_chip
            .accessible_label()
            .contains("degraded"));
    }

    #[tokio::test]
    async fn rows_carry_the_harmonized_sub_line_progress_and_needs_tags() {
        // Prototype FleetRowRich (T085): spec branch sub-line + progress readout, and an
        // inline NeedsTag on rows blocked on the operator.
        let fx = daedalus_tests::Fixture::new();
        let tool = fx.register_sample_tool("claude");
        let req = fx.fresh_request(tool);
        fx.write_tasks(&req.objective, "- [x] T001 Done\n- [ ] T002 Open\n");
        let id = fx.core.start_session(req).await.unwrap();
        fx.core.refresh_task_board(id).unwrap();

        let view = FleetView::build(&fx.app, &Theme::host_default()).await;
        let row = &view.rows[0];
        assert!(
            row.spec.ends_with("tasks.md"),
            "the SDD spec ref rides the row"
        );
        assert_eq!(row.progress_label(), "1/2");
        assert_eq!(row.needs_tag, None, "a running row carries no needs-tag");

        fx.core.mark_waiting_for_input(id, "Continue?").unwrap();
        let view = FleetView::build(&fx.app, &Theme::host_default()).await;
        assert_eq!(view.rows[0].needs_tag, Some("Asked a question"));
    }

    #[tokio::test]
    async fn attention_sorts_first_and_the_empty_state_offers_start_and_discover() {
        let fx = daedalus_tests::Fixture::new();

        // Empty first run: title + both actions (prototype Fleet empty state).
        let view = FleetView::build(&fx.app, &Theme::host_default()).await;
        let empty = view.empty_state().expect("no sessions yet");
        assert_eq!(empty.title, "No sessions yet");
        let labels: Vec<&str> = empty.actions.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels, ["Start a session", "Discover"]);

        // A waiting session outranks a running one regardless of creation order.
        let tool = fx.register_sample_tool("claude");
        let waiting = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
        let _running = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
        fx.core
            .mark_waiting_for_input(waiting, "Continue?")
            .unwrap();
        let view = FleetView::build(&fx.app, &Theme::host_default()).await;
        assert_eq!(view.rows[0].id, waiting);
        assert!(view.empty_state().is_none());
    }

    #[test]
    fn the_status_filter_set_includes_the_attention_pair_and_unknown() {
        // Prototype STATUS_FILTERS (T085): running · awaiting · confirm · stalled ·
        // failed · completed · stopped · unknown.
        assert_eq!(
            STATUS_FILTERS,
            [
                SessionStatus::Running,
                SessionStatus::WaitingForInput,
                SessionStatus::AwaitingConfirmation,
                SessionStatus::Stalled,
                SessionStatus::Failed,
                SessionStatus::Completed,
                SessionStatus::Stopped,
                SessionStatus::Unknown,
            ]
        );
    }
}
