//! Environments / Backends screen (§6.7, FR-027/028): backend availability + limits,
//! including the stated degraded/unavailable reason and the host-down continuity
//! reassurance (prototype `screens.jsx` Backends, `host-down` state).

use daedalus_app::{App, AppQueryAsync};
use daedalus_proto::{Availability, BackendKind, BackendStatus, SandboxEnvironment};

use crate::components::{ActionButton, ButtonIntent, Chip, EmptyState, StatusBadge};
use crate::theme::{StatusTone, Theme};

/// The FR-028 continuity line shown when a host is down (prototype `host-reassure`).
pub const HOST_DOWN_REASSURANCE: &str =
    "Other hosts keep working — sessions elsewhere are unaffected.";

/// A backend availability row.
#[derive(Debug, Clone)]
pub struct BackendRow {
    /// Backend kind.
    pub kind: BackendKind,
    /// Availability.
    pub availability: Availability,
    /// Availability badge (Running tone = available, Unreachable = unavailable).
    pub badge: StatusBadge,
    /// The backend chip — availability dot ONLY when not available (T083).
    pub chip: Chip,
    /// Stated reason when degraded/unavailable (FR-028; prototype `brow-note`).
    pub reason: Option<String>,
    /// Row action: "Reconnect" for a down host (prototype `host-down` state).
    pub action: Option<&'static str>,
}

/// The backends view.
#[derive(Debug, Clone)]
pub struct BackendsView {
    /// Rows, one per registered backend.
    pub rows: Vec<BackendRow>,
    /// The continuity reassurance banner, shown when any host is down (FR-028).
    pub reassurance: Option<&'static str>,
    /// The environments section rows (§6.7).
    pub environments: Vec<SandboxEnvironment>,
}

impl BackendsView {
    /// The no-environments empty state (prototype Backends `no-envs`): explains that
    /// provisioning is automatic, and offers Create / Start.
    #[must_use]
    pub fn no_envs_empty_state(&self) -> Option<EmptyState> {
        self.environments.is_empty().then(|| EmptyState {
            icon: "backends",
            title: "No environments yet",
            body: "Daedalus provisions an isolated environment automatically when you \
                   start a session — or create one here to have it ready ahead of time.",
            actions: vec![
                ActionButton::enabled("Create environment", ButtonIntent::Primary),
                ActionButton::enabled("Start a session", ButtonIntent::Ghost),
            ],
        })
    }

    /// Build from current backend statuses.
    pub async fn build(app: &App, theme: &Theme) -> Self {
        let statuses = app.backends().await;
        let environments = app.core().environments().unwrap_or_default();
        Self::from_data(statuses, environments, theme)
    }

    /// Shape prefetched backend statuses + environments into the view — lets a refresh
    /// tick fetch each once and feed every consumer (no double polling).
    #[must_use]
    pub fn from_data(
        statuses: Vec<BackendStatus>,
        environments: Vec<SandboxEnvironment>,
        theme: &Theme,
    ) -> Self {
        let rows: Vec<BackendRow> = statuses
            .into_iter()
            .map(|s| {
                let tone = match s.availability {
                    Availability::Available => StatusTone::Running,
                    Availability::Degraded => StatusTone::Stalled,
                    Availability::Unavailable => StatusTone::Unreachable,
                };
                BackendRow {
                    kind: s.kind,
                    availability: s.availability,
                    badge: StatusBadge::from_tone(tone, theme),
                    chip: Chip::backend(s.kind, s.availability, theme),
                    reason: s.availability_reason,
                    action: (s.availability == Availability::Unavailable).then_some("Reconnect"),
                }
            })
            .collect();
        let reassurance = rows
            .iter()
            .any(|r| r.availability == Availability::Unavailable)
            .then_some(HOST_DOWN_REASSURANCE);
        Self {
            rows,
            reassurance,
            environments,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use daedalus_proto::Availability;

    #[tokio::test]
    async fn degraded_rows_state_their_reason_and_a_down_host_shows_the_reassurance() {
        // FR-028 (T083): a degraded backend states why (prototype `brow-note`); a down
        // host adds the continuity banner + Reconnect, copy verbatim from the prototype
        // Backends `host-down` state.
        let fx = daedalus_tests::Fixture::new();
        let theme = Theme::host_default();

        // Healthy: no reason, no banner, no availability dot on the chip.
        let view = BackendsView::build(&fx.app, &theme).await;
        assert_eq!(view.rows.len(), 1);
        assert!(view.rows[0].reason.is_none());
        assert!(view.rows[0].chip.dot.is_none());
        assert!(view.reassurance.is_none());
        assert!(view.rows[0].action.is_none());

        // Degraded with a stated reason (prototype degraded host note).
        fx.backend.set_availability_with_reason(
            Availability::Degraded,
            Some("High memory pressure — provisioning may be slow."),
        );
        let view = BackendsView::build(&fx.app, &theme).await;
        assert_eq!(
            view.rows[0].reason.as_deref(),
            Some("High memory pressure — provisioning may be slow.")
        );
        assert!(view.rows[0].chip.dot.is_some());
        assert!(
            view.reassurance.is_none(),
            "degraded alone does not raise the host-down banner"
        );

        // Host down: flagged, never hidden — with the FR-028 reassurance line.
        fx.backend.set_availability_with_reason(
            Availability::Unavailable,
            Some("Tunnel dropped — reconnect to resume."),
        );
        let view = BackendsView::build(&fx.app, &theme).await;
        assert_eq!(
            view.reassurance,
            Some("Other hosts keep working — sessions elsewhere are unaffected.")
        );
        assert_eq!(view.rows[0].action, Some("Reconnect"));
        assert_eq!(
            view.rows[0].reason.as_deref(),
            Some("Tunnel dropped — reconnect to resume.")
        );
    }

    #[tokio::test]
    async fn no_environments_yet_shows_the_first_run_empty_state() {
        // §6.7 (T085): before any session, the Environments section explains that
        // Daedalus provisions automatically — prototype `no-envs` copy verbatim.
        let fx = daedalus_tests::Fixture::new();
        let theme = Theme::host_default();

        let view = BackendsView::build(&fx.app, &theme).await;
        assert!(view.environments.is_empty());
        let empty = view.no_envs_empty_state().expect("first run: no envs");
        assert_eq!(empty.title, "No environments yet");
        assert_eq!(
            empty.body,
            "Daedalus provisions an isolated environment automatically when you start a \
             session — or create one here to have it ready ahead of time."
        );
        let labels: Vec<&str> = empty.actions.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels, ["Create environment", "Start a session"]);

        // A started session provisions an environment: the empty state clears.
        let tool = fx.register_sample_tool("claude");
        fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
        let view = BackendsView::build(&fx.app, &theme).await;
        assert_eq!(view.environments.len(), 1);
        assert!(view.no_envs_empty_state().is_none());
    }
}
