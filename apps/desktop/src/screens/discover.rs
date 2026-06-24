//! Discover screen (US3, §6.5): discovered sessions grouped by source, with connect action
//! and empty/scanning/unreachable states.

use daedalus_app::App;
use daedalus_proto::{DiscoveredSession, SourceKind};

use crate::components::StatusBadge;
use crate::theme::{StatusTone, Theme};

/// A discovered-session row.
#[derive(Debug, Clone)]
pub struct DiscoverRow {
    /// The discovered session.
    pub session: DiscoveredSession,
    /// Reachability badge (Unreachable when the source dropped).
    pub badge: StatusBadge,
    /// Whether the connect action is enabled.
    pub connectable: bool,
    /// Reason shown when not connectable.
    pub reason: Option<String>,
}

/// A source group.
#[derive(Debug, Clone)]
pub struct SourceGroup {
    /// Source kind (Local / mDNS / TunneledWorkshop).
    pub kind: SourceKind,
    /// Rows under this source.
    pub rows: Vec<DiscoverRow>,
}

/// The discover view.
#[derive(Debug, Clone)]
pub struct DiscoverView {
    /// Groups by source kind.
    pub groups: Vec<SourceGroup>,
}

impl DiscoverView {
    /// Build from current discovery results.
    pub async fn build(app: &App, theme: &Theme) -> Self {
        let discovered = app.core().discovered().await;
        let mut groups: Vec<SourceGroup> = Vec::new();
        for session in discovered {
            let unreachable =
                session.source_availability == daedalus_proto::Availability::Unavailable;
            let badge = if unreachable {
                StatusBadge::from_tone(StatusTone::Unreachable, theme)
            } else {
                StatusBadge::from_tone(StatusTone::Running, theme)
            };
            let row = DiscoverRow {
                connectable: session.attachable,
                reason: session.attach_reason.clone(),
                badge,
                session: session.clone(),
            };
            match groups.iter_mut().find(|g| g.kind == session.kind) {
                Some(group) => group.rows.push(row),
                None => groups.push(SourceGroup {
                    kind: session.kind,
                    rows: vec![row],
                }),
            }
        }
        Self { groups }
    }

    /// True when nothing was discovered (drives the empty state).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.groups.iter().all(|g| g.rows.is_empty())
    }
}
