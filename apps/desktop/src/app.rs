//! The GPUI app shell view-model (T017a, §6.1): nav (Tasks/Fleet/Discover/Tools/Backends/
//! Settings), global status counts, a Hosts list, and the title-bar descriptor. Pure data
//! the GPUI layer renders; built from `App` queries.

use daedalus_app::{App, AppQuery};
use daedalus_proto::SessionStatus;

use crate::theme::{Skin, Theme};

/// Primary navigation destinations (Tasks board is the landing view — design IA decision).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavItem {
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
    /// All nav items in display order.
    #[must_use]
    pub fn all() -> [NavItem; 6] {
        [
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
    /// Needing attention (stalled or awaiting confirmation).
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
                SessionStatus::Stalled | SessionStatus::AwaitingConfirmation => c.attention += 1,
                SessionStatus::Completed => c.completed += 1,
                SessionStatus::Failed => c.failed += 1,
                SessionStatus::Stopped => c.stopped += 1,
            }
        }
        c
    }
}

/// A host entry in the sidebar Hosts list.
#[derive(Debug, Clone)]
pub struct HostEntry {
    /// Host label.
    pub label: String,
    /// Whether the host is reachable.
    pub reachable: bool,
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
    /// Hosts list.
    pub hosts: Vec<HostEntry>,
    /// Active theme.
    pub theme: Theme,
}

impl AppShell {
    /// Build the shell from current state and the active nav destination.
    pub async fn build(app: &App, theme: Theme, active: NavItem) -> Self {
        let counts = StatusCounts::from_app(app);

        // Hosts derived from discovered sessions (unique host labels).
        let mut hosts: Vec<HostEntry> = Vec::new();
        for d in app.core().discovered().await {
            if !hosts.iter().any(|h| h.label == d.host_label) {
                hosts.push(HostEntry {
                    label: d.host_label.clone(),
                    reachable: d.source_availability != daedalus_proto::Availability::Unavailable,
                });
            }
        }
        if hosts.is_empty() {
            hosts.push(HostEntry {
                label: "localhost".to_string(),
                reachable: true,
            });
        }

        Self {
            title_bar: TitleBar {
                title: "Daedalus".to_string(),
                skin: theme.skin,
                counts,
            },
            active,
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
        assert!(!shell.hosts.is_empty());
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
