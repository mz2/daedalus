//! The GPUI app shell view-model (T017a, §6.1): nav (Tasks/Fleet/Discover/Tools/Backends/
//! Settings), global status counts, a Hosts list, and the title-bar descriptor. Pure data
//! the GPUI layer renders; built from `App` queries.

use daedalus_app::{App, AppQuery, AppQueryAsync};
use daedalus_proto::{Availability, SessionStatus, SourceKind};

use crate::components::availability_text;
use crate::theme::{Skin, Theme};

/// Primary navigation destinations (Tasks board is the landing view — design IA decision).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavItem {
    /// The Needs-you queue (US6): listed first, with a purple badge count (prototype
    /// `app.jsx` default `needsPlacement: "view"`).
    Needs,
    /// Aggregate tasks board (home).
    Tasks,
    /// Session-level fleet view.
    Fleet,
    /// Discover sessions across sources.
    Discover,
    /// Tools registry.
    Tools,
    /// Environments / backends.
    Backends,
    /// Settings modal.
    Settings,
}

impl NavItem {
    /// The landing view on launch: the aggregate Tasks board (FR-025a). The Fleet stays
    /// one click away in the nav.
    #[must_use]
    pub fn home() -> NavItem {
        NavItem::Tasks
    }

    /// All nav items in display order.
    #[must_use]
    pub fn all() -> [NavItem; 7] {
        [
            NavItem::Needs,
            NavItem::Tasks,
            NavItem::Fleet,
            NavItem::Discover,
            NavItem::Tools,
            NavItem::Backends,
            NavItem::Settings,
        ]
    }

    /// Label shown in the nav.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            NavItem::Needs => "Needs you",
            NavItem::Tasks => "Tasks",
            NavItem::Fleet => "Fleet",
            NavItem::Discover => "Discover",
            NavItem::Tools => "Tools",
            NavItem::Backends => "Backends",
            NavItem::Settings => "Settings",
        }
    }

    /// Screen-reader label for AccessKit (role conveyed alongside the name).
    #[must_use]
    pub fn accessible_label(self) -> String {
        format!("{} — navigation", self.label())
    }
}

/// A keyboard shortcut surfaced in the command palette and wired by the renderer (carried
/// over from the prototype; design/README.md). Keeping it as data lets the GPUI layer bind
/// keys and expose them to AccessKit without hard-coding strings in the view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shortcut {
    /// Human chord, e.g. "⌘K".
    pub chord: &'static str,
    /// What it does.
    pub action: &'static str,
}

/// The global keyboard shortcuts (every primary action is reachable without a mouse).
#[must_use]
pub fn keyboard_shortcuts() -> &'static [Shortcut] {
    &[
        Shortcut {
            chord: "⌘K",
            action: "Command palette",
        },
        Shortcut {
            chord: "⌘N",
            action: "Start session",
        },
        Shortcut {
            chord: "⌘⇧L",
            action: "Toggle theme",
        },
        Shortcut {
            chord: "⌘[",
            action: "Back from session detail",
        },
    ]
}

/// Global status counts shown in the title bar.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatusCounts {
    /// Running.
    pub running: usize,
    /// Blocked on the operator: a question (waiting for input) or an unconfirmed clean
    /// exit (awaiting confirmation) — the title bar's purple segment (prototype `app.jsx`).
    pub awaiting: usize,
    /// Needing attention (waiting for input, awaiting confirmation, stalled, or
    /// connection lost).
    pub attention: usize,
    /// Completed.
    pub completed: usize,
    /// Failed.
    pub failed: usize,
    /// Stopped.
    pub stopped: usize,
}

impl StatusCounts {
    /// Tally a fleet's statuses.
    #[must_use]
    pub fn from_app(app: &App) -> Self {
        let mut c = StatusCounts::default();
        for s in app.fleet() {
            match s.status {
                SessionStatus::Running | SessionStatus::Starting => c.running += 1,
                SessionStatus::Stalled
                | SessionStatus::AwaitingConfirmation
                | SessionStatus::WaitingForInput
                | SessionStatus::Unknown => c.attention += 1,
                SessionStatus::Completed => c.completed += 1,
                SessionStatus::Failed => c.failed += 1,
                SessionStatus::Stopped => c.stopped += 1,
            }
            if matches!(
                s.status,
                SessionStatus::WaitingForInput | SessionStatus::AwaitingConfirmation
            ) {
                c.awaiting += 1;
            }
        }
        c
    }
}

/// One host row in the shell hosts indicator (titlebar pill + popover) and the sidebar
/// Hosts list (FR-028; prototype `app.jsx` `HostsIndicator`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostRow {
    /// How the host is reached (drives the row's kind icon: host/globe/tunnel).
    pub kind: SourceKind,
    /// Host label ("This host" for the local machine).
    pub name: String,
    /// Current availability (one `AvailDot` per host on the pill).
    pub availability: Availability,
    /// Stated reason when degraded/unavailable (FR-028), shown on the popover row.
    pub reason: Option<String>,
}

impl HostRow {
    /// The lowercase availability text shown beside the dot (prototype `hosts-avail`).
    #[must_use]
    pub fn availability_text(&self) -> &'static str {
        availability_text(self.availability)
    }
}

/// The shell hosts indicator: one dot per host on a compact titlebar pill, worst
/// availability first; the popover lists each host with its dot, kind icon, name,
/// availability text, and stated reason (FR-028, T083).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostsIndicator {
    /// Host rows, worst availability first.
    pub rows: Vec<HostRow>,
}

impl HostsIndicator {
    /// The popover footnote (prototype `hosts-pop-note`).
    pub const POPOVER_NOTE: &'static str =
        "Availability from local checks, mDNS presence, and tunnel state.";

    /// Build from current backend + discovery state: the local machine's row is derived
    /// from the worst local backend availability (with its stated reason); every
    /// discovered host contributes a row from its source availability.
    pub async fn build(app: &App) -> Self {
        let mut rows: Vec<HostRow> = Vec::new();

        // The local host: worst availability among the registered backends (FR-028 keeps
        // the shell truthful when a local backend degrades), with its stated reason.
        let backends = app.backends().await;
        let worst = backends
            .iter()
            .max_by_key(|b| severity(b.availability))
            .map(|b| (b.availability, b.availability_reason.clone()))
            .unwrap_or((Availability::Available, None));
        rows.push(HostRow {
            kind: SourceKind::Local,
            name: "This host".to_string(),
            availability: worst.0,
            reason: worst.1,
        });

        // Discovered hosts (unique labels), carrying their source availability + the
        // stated attach reason when unreachable (never silently dropped).
        for d in app.discovered().await {
            if rows.iter().any(|h| h.name == d.host_label) {
                continue;
            }
            rows.push(HostRow {
                kind: d.kind,
                name: d.host_label.clone(),
                availability: d.source_availability,
                reason: match d.source_availability {
                    Availability::Available => None,
                    _ => d.attach_reason.clone(),
                },
            });
        }

        // Worst first (prototype AVAIL_RANK sort).
        rows.sort_by_key(|h| std::cmp::Reverse(severity(h.availability)));
        Self { rows }
    }

    /// Screen-reader / tooltip label for the pill (prototype `title`).
    #[must_use]
    pub fn accessible_label(&self) -> &'static str {
        "Hosts — availability"
    }
}

/// Sort key: higher = worse (prototype `AVAIL_RANK` inverted).
fn severity(a: Availability) -> u8 {
    match a {
        Availability::Available => 0,
        Availability::Degraded => 1,
        Availability::Unavailable => 2,
    }
}

/// The title-bar descriptor (skin-correct controls handled by the renderer).
#[derive(Debug, Clone)]
pub struct TitleBar {
    /// App/brand title.
    pub title: String,
    /// Active skin (drives accent + window-control placement).
    pub skin: Skin,
    /// Global status counts.
    pub counts: StatusCounts,
}

/// The whole app shell.
#[derive(Debug, Clone)]
pub struct AppShell {
    /// Title bar.
    pub title_bar: TitleBar,
    /// Active nav destination.
    pub active: NavItem,
    /// The "Needs you" nav badge count (attention-purple `await` variant; 0 = no badge).
    pub needs_badge: usize,
    /// The hosts indicator (titlebar pill + popover rows; also the sidebar Hosts list).
    pub hosts: HostsIndicator,
    /// Active theme.
    pub theme: Theme,
}

impl AppShell {
    /// Build the shell from current state and the active nav destination.
    pub async fn build(app: &App, theme: Theme, active: NavItem) -> Self {
        let counts = StatusCounts::from_app(app);
        let hosts = HostsIndicator::build(app).await;

        Self {
            title_bar: TitleBar {
                title: "Daedalus".to_string(),
                skin: theme.skin,
                counts,
            },
            active,
            needs_badge: app.needs_you().len(),
            hosts,
            theme,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn shell_counts_running_sessions() {
        let fx = daedalus_tests::Fixture::new();
        let tool = fx.register_sample_tool("claude");
        fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
        let shell = AppShell::build(&fx.app, Theme::host_default(), NavItem::Tasks).await;
        assert_eq!(shell.title_bar.counts.running, 1);
        assert!(!shell.hosts.rows.is_empty());
    }

    #[tokio::test]
    async fn shell_surfaces_the_needs_you_badge_and_the_purple_awaiting_count() {
        let fx = daedalus_tests::Fixture::new();
        let tool = fx.register_sample_tool("claude");
        let asking = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
        fx.core
            .mark_waiting_for_input(asking, "Continue? (y/n)")
            .unwrap();
        let req = fx.fresh_request(tool);
        let objective = req.objective.clone();
        fx.write_tasks(&objective, "- [ ] T001 Implement\n");
        let confirm = fx.core.start_session(req).await.unwrap();
        fx.core
            .apply_signal(confirm, daedalus_core::AgentSignal::ExitedCleanly)
            .await
            .unwrap();

        // The purple title-bar segment counts waiting + confirm (prototype GlobalStatus).
        let counts = StatusCounts::from_app(&fx.app);
        assert_eq!(counts.awaiting, 2);

        // The "Needs you" nav item is first and carries the queue count as its badge.
        let shell = AppShell::build(&fx.app, Theme::host_default(), NavItem::Needs).await;
        assert_eq!(shell.needs_badge, 2);
        assert_eq!(NavItem::all()[0], NavItem::Needs);
        assert_eq!(NavItem::Needs.label(), "Needs you");
    }

    #[test]
    fn tasks_is_the_home_view_with_fleet_one_click_away() {
        // FR-025a: the aggregate tasks board is the landing view; the session-level
        // fleet stays one navigation action away in the same nav list.
        assert_eq!(NavItem::home(), NavItem::Tasks);
        let all = NavItem::all();
        assert!(all.contains(&NavItem::Fleet));
        // Nav order per the prototype: Needs you, Tasks, Sessions, Discover, Tools,
        // Environments (Settings closes the list).
        assert_eq!(
            all,
            [
                NavItem::Needs,
                NavItem::Tasks,
                NavItem::Fleet,
                NavItem::Discover,
                NavItem::Tools,
                NavItem::Backends,
                NavItem::Settings,
            ]
        );
    }

    #[tokio::test]
    async fn hosts_indicator_lists_hosts_worst_first_with_stated_reasons() {
        // FR-028 (T083): the shell hosts indicator shows one dot per host, worst
        // availability first, and its popover rows carry kind + name + availability text
        // + the stated reason (prototype `app.jsx` HostsIndicator).
        use daedalus_proto::{Availability, SourceKind};
        use std::sync::Arc;

        let src = Arc::new(daedalus_tests::TestDiscoverySource::new(
            SourceKind::TunneledWorkshop,
        ));
        src.set_sessions(vec![src.make_session("alpha", true)]);
        let (core, backend, _dir) =
            daedalus_tests::core_with_discovery_and_backend(vec![Box::new(
                daedalus_tests::SharedTestSource(src.clone()),
            )]);
        let app = daedalus_app::App::new(core);

        // Healthy: one dot per host, no reasons.
        let hosts = HostsIndicator::build(&app).await;
        assert_eq!(hosts.rows.len(), 2, "the local host plus the tunnel host");
        assert!(hosts.rows.iter().all(|h| h.reason.is_none()));
        assert_eq!(hosts.accessible_label(), "Hosts — availability");
        assert_eq!(
            HostsIndicator::POPOVER_NOTE,
            "Availability from local checks, mDNS presence, and tunnel state."
        );

        // The tunnel drops and the local backend degrades with a stated reason.
        src.set_availability(Availability::Unavailable);
        backend.set_availability_with_reason(
            Availability::Degraded,
            Some("high memory pressure — provisioning may be slow"),
        );

        let hosts = HostsIndicator::build(&app).await;
        let rows = &hosts.rows;
        assert_eq!(rows.len(), 2);
        // Worst first: unavailable before degraded.
        assert_eq!(rows[0].kind, SourceKind::TunneledWorkshop);
        assert_eq!(rows[0].name, "test-host");
        assert_eq!(rows[0].availability, Availability::Unavailable);
        assert_eq!(rows[0].availability_text(), "unavailable");
        assert!(
            rows[0]
                .reason
                .as_deref()
                .is_some_and(|r| r.contains("source unreachable")),
            "the down host states why, got {:?}",
            rows[0].reason
        );
        assert_eq!(rows[1].kind, SourceKind::Local);
        assert_eq!(rows[1].name, "This host");
        assert_eq!(rows[1].availability, Availability::Degraded);
        assert_eq!(rows[1].availability_text(), "degraded");
        assert_eq!(
            rows[1].reason.as_deref(),
            Some("high memory pressure — provisioning may be slow")
        );
    }

    #[test]
    fn nav_items_and_shortcuts_expose_accessible_data() {
        for item in NavItem::all() {
            assert!(item.accessible_label().contains(item.label()));
        }
        // Every primary action is reachable from the keyboard (no mouse-only paths).
        let shortcuts = keyboard_shortcuts();
        assert!(shortcuts.iter().any(|s| s.action == "Command palette"));
        assert!(shortcuts.iter().any(|s| s.action == "Start session"));
    }
}
