//! Discover screen (US3, §6.5): discovered sessions grouped by source, with connect
//! action, the FR-013 de-duplication footnote, the FR-014 source-dropped treatment, and
//! empty states — copy verbatim from the prototype `Discover` (`design/prototype/screens.jsx`).

use daedalus_app::App;
use daedalus_proto::{DiscoveredSession, SessionStatus, SourceKind};

use crate::components::{ActionButton, ButtonIntent, EmptyState, StatusBadge};
use crate::theme::{StatusTone, Theme};

/// The row note under a session whose source dropped (FR-014 — kept listed, never
/// silently removed).
pub const UNREACHABLE_ROW_NOTE: &str =
    "Last seen before the tunnel dropped — kept listed until it reconnects.";

/// Why Connect is disabled on an unreachable row.
pub const UNREACHABLE_CONNECT_REASON: &str = "Unreachable — reconnect the tunnel to attach";

/// The nothing-discovered empty state explains *why* nothing was found (brief §6.5).
pub const EMPTY_TITLE: &str = "Nothing discovered";
/// Its body copy.
pub const EMPTY_BODY: &str = "No hosts are advertising sessions right now. mDNS only \
     reaches hosts on your local network, a Workshop only advertises once the Daedalus SDK \
     is registered inside it, and tunneled Workshops appear only while their tunnel is \
     connected.";

/// The design group label per source kind (prototype `DISC_GROUPS`).
#[must_use]
pub fn group_label(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::Local => "Local host",
        SourceKind::Mdns => "mDNS-advertised hosts",
        SourceKind::TunneledWorkshop => "Tunneled Workshops",
    }
}

/// The in-group empty note per source kind (prototype `disc-empty`).
#[must_use]
pub fn group_empty_note(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::Mdns => "No hosts advertising on the local network.",
        _ => "Nothing discovered here.",
    }
}

/// The label on a not-attachable row: ended sessions read "Review only" (they open in
/// review mode), anything else "Not attachable" — never silent (brief §6.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotAttachable {
    /// "Review only" / "Not attachable".
    pub label: &'static str,
    /// The stated reason (shown on hover in the prototype).
    pub reason: String,
}

/// A discovered-session row.
#[derive(Debug, Clone)]
pub struct DiscoverRow {
    /// The discovered session.
    pub session: DiscoveredSession,
    /// Status badge (the advertised status; Unreachable when the source dropped).
    pub badge: StatusBadge,
    /// The Connect action — disabled with a reason on unreachable rows; `None` on
    /// not-attachable rows (which carry [`Self::not_attachable`] instead).
    pub connect: Option<ActionButton>,
    /// The not-attachable label + reason, when the row cannot be connected.
    pub not_attachable: Option<NotAttachable>,
    /// The FR-014 note under a row whose source dropped.
    pub unreachable_note: Option<&'static str>,
}

/// A source group — always rendered, even when empty (the prototype keeps all three
/// groups visible so the operator sees where discovery reaches).
#[derive(Debug, Clone)]
pub struct SourceGroup {
    /// Source kind (Local / mDNS / TunneledWorkshop).
    pub kind: SourceKind,
    /// Design label ("Local host" / "mDNS-advertised hosts" / "Tunneled Workshops").
    pub label: &'static str,
    /// Rows under this source.
    pub rows: Vec<DiscoverRow>,
    /// The in-group note when the group is empty.
    pub empty_note: Option<&'static str>,
    /// True when this group's source dropped (FR-014): the group is flagged, its rows
    /// turn unreachable, and a Reconnect action is offered.
    pub dropped: bool,
    /// The group-level Reconnect action while dropped.
    pub reconnect: Option<ActionButton>,
}

/// The discover view.
#[derive(Debug, Clone)]
pub struct DiscoverView {
    /// Groups by source kind, in nav order (Local · mDNS · Tunnel) — all three always.
    pub groups: Vec<SourceGroup>,
    /// The FR-013 footnote (e.g. "1 session advertised from multiple sources was
    /// de-duplicated."); `None` when nothing was folded.
    pub dedup_note: Option<String>,
}

impl DiscoverView {
    /// Build from current discovery results.
    pub async fn build(app: &App, theme: &Theme) -> Self {
        let (discovered, deduped) = app.core().discovered_counted().await;

        let mut groups: Vec<SourceGroup> = [
            SourceKind::Local,
            SourceKind::Mdns,
            SourceKind::TunneledWorkshop,
        ]
        .into_iter()
        .map(|kind| SourceGroup {
            kind,
            label: group_label(kind),
            rows: Vec::new(),
            empty_note: None,
            dropped: false,
            reconnect: None,
        })
        .collect();

        for session in discovered {
            let unreachable =
                session.source_availability == daedalus_proto::Availability::Unavailable;
            let badge = if unreachable {
                StatusBadge::from_tone(StatusTone::Unreachable, theme)
            } else {
                match session.status {
                    Some(status) => StatusBadge::session(status, theme),
                    None => StatusBadge::from_tone(StatusTone::Running, theme),
                }
            };
            let (connect, not_attachable) = if unreachable {
                // Kept listed with an explicit unreachable state (FR-014); Connect stays
                // visible but disabled with the reason.
                (
                    Some(ActionButton::disabled(
                        "Connect",
                        ButtonIntent::Tinted,
                        UNREACHABLE_CONNECT_REASON,
                    )),
                    None,
                )
            } else if session.attachable {
                (
                    Some(ActionButton::enabled("Connect", ButtonIntent::Tinted)),
                    None,
                )
            } else {
                let label = if session.status == Some(SessionStatus::Completed) {
                    "Review only"
                } else {
                    "Not attachable"
                };
                (
                    None,
                    Some(NotAttachable {
                        label,
                        reason: session
                            .attach_reason
                            .clone()
                            .unwrap_or_else(|| "session is not attachable".into()),
                    }),
                )
            };
            let row = DiscoverRow {
                badge,
                connect,
                not_attachable,
                unreachable_note: unreachable.then_some(UNREACHABLE_ROW_NOTE),
                session: session.clone(),
            };
            if let Some(group) = groups.iter_mut().find(|g| g.kind == session.kind) {
                group.rows.push(row);
            }
        }

        for group in &mut groups {
            if group.rows.is_empty() {
                group.empty_note = Some(group_empty_note(group.kind));
            } else if group.rows.iter().all(|r| r.unreachable_note.is_some()) {
                // The whole source dropped: flag the group and offer Reconnect (FR-014).
                group.dropped = true;
                group.reconnect = Some(ActionButton::enabled("Reconnect", ButtonIntent::Tinted));
            }
        }

        let dedup_note = match deduped {
            0 => None,
            1 => Some("1 session advertised from multiple sources was de-duplicated.".to_string()),
            n => Some(format!(
                "{n} sessions advertised from multiple sources were de-duplicated."
            )),
        };

        Self { groups, dedup_note }
    }

    /// True when nothing was discovered (drives the empty state).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.groups.iter().all(|g| g.rows.is_empty())
    }

    /// The nothing-discovered empty state (brief §6.5): explains why nothing was found
    /// and offers Rescan / Set up a tunnel.
    #[must_use]
    pub fn empty_state(&self) -> Option<EmptyState> {
        self.is_empty().then(|| EmptyState {
            icon: "radar",
            title: EMPTY_TITLE,
            body: EMPTY_BODY,
            actions: vec![
                ActionButton::enabled("Rescan", ButtonIntent::Primary),
                ActionButton::enabled("Set up a tunnel", ButtonIntent::Ghost),
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use daedalus_app::App;
    use daedalus_proto::{Availability, SessionStatus, SourceKind};
    use daedalus_tests::{core_with_discovery, SharedTestSource, TestDiscoverySource};
    use std::sync::Arc;

    #[tokio::test]
    async fn groups_are_always_present_with_labels_and_empty_notes() {
        // Prototype DISC_GROUPS (T085): all three groups render even when empty, the
        // mDNS group with its own explanation.
        let (core, _dir) = core_with_discovery(Vec::new());
        let app = App::new(core);
        let view = DiscoverView::build(&app, &Theme::host_default()).await;

        assert_eq!(view.groups.len(), 3);
        assert_eq!(view.groups[0].label, "Local host");
        assert_eq!(view.groups[1].label, "mDNS-advertised hosts");
        assert_eq!(view.groups[2].label, "Tunneled Workshops");
        assert_eq!(
            view.groups[1].empty_note,
            Some("No hosts advertising on the local network.")
        );
        assert_eq!(view.groups[0].empty_note, Some("Nothing discovered here."));

        // Nothing discovered at all ⇒ the explanatory empty state (why + next actions).
        let empty = view.empty_state().expect("empty view has an empty state");
        assert_eq!(empty.title, "Nothing discovered");
        assert!(empty
            .body
            .starts_with("No hosts are advertising sessions right now."));
        let labels: Vec<&str> = empty.actions.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels, ["Rescan", "Set up a tunnel"]);
    }

    #[tokio::test]
    async fn rows_carry_status_badges_and_the_review_only_reason() {
        let mdns = TestDiscoverySource::new(SourceKind::Mdns);
        let mut running = mdns.make_session("run-1", true);
        running.status = Some(SessionStatus::Running);
        let mut ended = mdns.make_session("done-1", false);
        ended.status = Some(SessionStatus::Completed);
        ended.attach_reason =
            Some("Session ended — not attachable via zellij. Open in review mode instead.".into());
        mdns.set_sessions(vec![running, ended]);

        let (core, _dir) = core_with_discovery(vec![Box::new(mdns)]);
        let app = App::new(core);
        let view = DiscoverView::build(&app, &Theme::host_default()).await;

        let group = &view.groups[1];
        assert_eq!(group.rows.len(), 2);
        assert!(group.empty_note.is_none());
        assert!(!group.dropped);

        let running = group
            .rows
            .iter()
            .find(|r| r.session.id.identity.0 == "run-1")
            .unwrap();
        assert_eq!(running.badge.label, "Running");
        assert!(running.connect.as_ref().unwrap().enabled);
        assert!(running.not_attachable.is_none());

        // Ended ⇒ "Review only" with the stated reason — never a silent missing button.
        let ended = group
            .rows
            .iter()
            .find(|r| r.session.id.identity.0 == "done-1")
            .unwrap();
        assert_eq!(ended.badge.label, "Completed");
        assert!(ended.connect.is_none());
        let na = ended.not_attachable.as_ref().unwrap();
        assert_eq!(na.label, "Review only");
        assert_eq!(
            na.reason,
            "Session ended — not attachable via zellij. Open in review mode instead."
        );
    }

    #[tokio::test]
    async fn dropped_tunnel_keeps_rows_listed_as_unreachable_with_reconnect() {
        // FR-014 (T085): the tunnel group is flagged, rows turn unreachable with the
        // kept-listed note, and Connect is disabled with a reason.
        let tunnel = Arc::new(TestDiscoverySource::new(SourceKind::TunneledWorkshop));
        tunnel.set_sessions(vec![tunnel.make_session("t-1", true)]);
        let (core, _dir) = core_with_discovery(vec![Box::new(SharedTestSource(tunnel.clone()))]);
        let app = App::new(core);

        // Healthy first poll (the coordinator retains last-seen sessions).
        let view = DiscoverView::build(&app, &Theme::host_default()).await;
        assert!(!view.groups[2].dropped);

        tunnel.set_availability(Availability::Unavailable);
        let view = DiscoverView::build(&app, &Theme::host_default()).await;
        let group = &view.groups[2];
        assert!(group.dropped, "the dropped source is flagged, not hidden");
        assert_eq!(group.reconnect.as_ref().unwrap().label, "Reconnect");
        let row = &group.rows[0];
        assert_eq!(row.badge.tone, StatusTone::Unreachable);
        assert_eq!(
            row.unreachable_note,
            Some("Last seen before the tunnel dropped — kept listed until it reconnects.")
        );
        let connect = row.connect.as_ref().unwrap();
        assert!(!connect.enabled);
        assert_eq!(
            connect.disabled_reason.as_deref(),
            Some("Unreachable — reconnect the tunnel to attach")
        );
    }

    #[tokio::test]
    async fn multi_source_sessions_are_folded_with_the_dedup_footnote() {
        // FR-013 (T085): the same identity advertised locally and over mDNS collapses to
        // one row, and the footnote says so in the prototype's words.
        let local = TestDiscoverySource::new(SourceKind::Local);
        let mdns = TestDiscoverySource::new(SourceKind::Mdns);
        local.set_sessions(vec![local.make_session("same-1", true)]);
        mdns.set_sessions(vec![mdns.make_session("same-1", true)]);

        let (core, _dir) = core_with_discovery(vec![Box::new(local), Box::new(mdns)]);
        let app = App::new(core);
        let view = DiscoverView::build(&app, &Theme::host_default()).await;

        let total: usize = view.groups.iter().map(|g| g.rows.len()).sum();
        assert_eq!(total, 1, "duplicates fold to one entry");
        assert_eq!(
            view.dedup_note.as_deref(),
            Some("1 session advertised from multiple sources was de-duplicated.")
        );
    }
}
