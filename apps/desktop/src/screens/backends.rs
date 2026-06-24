//! Environments / Backends screen (§6.7, FR-027/028): backend availability + limits.

use daedalus_app::App;
use daedalus_proto::{Availability, BackendKind};

use crate::components::StatusBadge;
use crate::theme::{StatusTone, Theme};

/// A backend availability row.
#[derive(Debug, Clone)]
pub struct BackendRow {
    /// Backend kind.
    pub kind: BackendKind,
    /// Availability.
    pub availability: Availability,
    /// Availability badge (Running tone = available, Unreachable = unavailable).
    pub badge: StatusBadge,
}

/// The backends view.
#[derive(Debug, Clone)]
pub struct BackendsView {
    /// Rows, one per registered backend.
    pub rows: Vec<BackendRow>,
}

impl BackendsView {
    /// Build from current backend statuses.
    pub async fn build(app: &App, theme: &Theme) -> Self {
        let rows = app
            .core()
            .backends_status()
            .await
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
                }
            })
            .collect();
        Self { rows }
    }
}
