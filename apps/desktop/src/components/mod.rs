//! Reusable component view-models ported from `design/components.css` + `*.jsx`.
//!
//! Each is a render-agnostic descriptor the GPUI layer turns into widgets (behind the
//! `gpui` feature). Keeping them as data makes the design mapping unit-testable headlessly.

use daedalus_proto::{Availability, BackendKind, SessionStatus, SourceKind, TaskStatus};

use crate::theme::{Color, StatusTone, Theme};

/// A status badge: color + icon + label (never color alone — accessibility).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusBadge {
    /// Palette tone.
    pub tone: StatusTone,
    /// Resolved color under the active skin (the filled-pill background).
    pub color: Color,
    /// Legible label/icon color to paint over `color` (WCAG-AA on the fill).
    pub foreground: Color,
    /// Glyph paired with the color.
    pub glyph: &'static str,
    /// Text label.
    pub label: &'static str,
}

impl StatusBadge {
    /// The screen-reader label for AccessKit (e.g. "Status: Running").
    #[must_use]
    pub fn accessible_label(&self) -> String {
        self.tone.accessible_label()
    }
}

impl StatusBadge {
    /// Build a badge for a session status under a theme. The label follows the design
    /// status table (e.g. "Waiting for input" vs. "Awaiting confirmation" on the shared
    /// purple tone; `Unknown` reads "Connection lost").
    #[must_use]
    pub fn session(status: SessionStatus, theme: &Theme) -> Self {
        let tone = StatusTone::from_session(status);
        Self {
            label: StatusTone::session_label(status),
            ..Self::from_tone(tone, theme)
        }
    }

    /// Build a badge for a task status under a theme.
    #[must_use]
    pub fn task(status: TaskStatus, theme: &Theme) -> Self {
        Self::from_tone(StatusTone::from_task(status), theme)
    }

    /// Build a badge for an arbitrary tone (e.g. discovery `Unreachable`).
    #[must_use]
    pub fn from_tone(tone: StatusTone, theme: &Theme) -> Self {
        let color = theme.status_color(tone);
        Self {
            tone,
            color,
            foreground: color.best_foreground(),
            glyph: tone.glyph(),
            label: tone.label(),
        }
    }
}

/// The lowercase availability text shown alongside a dot (never color alone).
#[must_use]
pub fn availability_text(availability: Availability) -> &'static str {
    match availability {
        Availability::Available => "available",
        Availability::Degraded => "degraded",
        Availability::Unavailable => "unavailable",
    }
}

/// The availability-dot color for a chip/indicator: `None` when available (the dot is
/// rendered ONLY when availability != available — prototype `primitives.jsx`
/// `availDotColor`); degraded uses the stalled hue, unavailable the failed hue
/// (`AvailDot`).
#[must_use]
pub fn availability_dot(availability: Availability, theme: &Theme) -> Option<Color> {
    match availability {
        Availability::Available => None,
        Availability::Degraded => Some(theme.status_color(StatusTone::Stalled)),
        Availability::Unavailable => Some(theme.status_color(StatusTone::Failed)),
    }
}

/// A source/backend chip (prototype `SourceChip`/`BackendChip`): an icon hint + label with
/// an availability dot rendered ONLY when the source/backend is not available (FR-028).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chip {
    /// Icon hint the renderer maps to its icon set (prototype `SOURCE_META`/`BACKEND_META`).
    pub icon: &'static str,
    /// Chip label.
    pub label: &'static str,
    /// Availability dot color; `None` (no dot) when available.
    pub dot: Option<Color>,
    /// The availability behind the dot (drives the accessible label).
    pub availability: Availability,
}

impl Chip {
    /// A chip for a discovery source kind (prototype `SourceChip`).
    #[must_use]
    pub fn source(kind: SourceKind, availability: Availability, theme: &Theme) -> Self {
        let (icon, label) = match kind {
            SourceKind::Local => ("dot", "Local"),
            SourceKind::Mdns => ("globe", "mDNS"),
            SourceKind::TunneledWorkshop => ("tunnel", "Tunnel"),
        };
        Self {
            icon,
            label,
            dot: availability_dot(availability, theme),
            availability,
        }
    }

    /// A chip for a backend kind (prototype `BackendChip`).
    #[must_use]
    pub fn backend(kind: BackendKind, availability: Availability, theme: &Theme) -> Self {
        let (icon, label) = match kind {
            BackendKind::Workshop => ("linux", "Workshop"),
            // Prototype BACKEND_TYPES `openshell` — icon "gpu" (data.js).
            BackendKind::OpenShell => ("gpu", "OpenShell"),
            BackendKind::Fake => ("backends", "Fake"),
        };
        Self {
            icon,
            label,
            dot: availability_dot(availability, theme),
            availability,
        }
    }

    /// Screen-reader label: the chip text plus, when impaired, the availability — the
    /// state is never conveyed by the dot color alone (WCAG 1.4.1).
    #[must_use]
    pub fn accessible_label(&self) -> String {
        match self.availability {
            Availability::Available => self.label.to_string(),
            other => format!("{} — {}", self.label, availability_text(other)),
        }
    }
}

/// A resource meter (CPU/memory/disk/time) — value clamped to `[0, max]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Meter {
    /// Current value.
    pub value: f64,
    /// Maximum for the bar.
    pub max: f64,
}

impl Meter {
    /// Fraction filled in `[0.0, 1.0]`.
    #[must_use]
    pub fn fraction(self) -> f64 {
        if self.max <= 0.0 {
            0.0
        } else {
            (self.value / self.max).clamp(0.0, 1.0)
        }
    }
}

/// A button intent (maps to the prototype's primary/tinted/danger/ghost styles).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonIntent {
    /// Filled accent.
    Primary,
    /// Tinted accent.
    Tinted,
    /// Destructive.
    Danger,
    /// Quiet.
    Ghost,
}

/// A header action button descriptor (e.g. Stop / Send-input / Confirm / Clean-up).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionButton {
    /// Visible label.
    pub label: String,
    /// Visual intent.
    pub intent: ButtonIntent,
    /// Whether the control is enabled.
    pub enabled: bool,
    /// When disabled, the reason shown to the operator (e.g. "tool does not accept input").
    pub disabled_reason: Option<String>,
}

impl ActionButton {
    /// An enabled button.
    #[must_use]
    pub fn enabled(label: impl Into<String>, intent: ButtonIntent) -> Self {
        Self {
            label: label.into(),
            intent,
            enabled: true,
            disabled_reason: None,
        }
    }

    /// A disabled button carrying a reason.
    #[must_use]
    pub fn disabled(
        label: impl Into<String>,
        intent: ButtonIntent,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            intent,
            enabled: false,
            disabled_reason: Some(reason.into()),
        }
    }

    /// Screen-reader label for AccessKit: the action plus, when disabled, the reason so the
    /// control is never silently inert (WCAG 4.1.2 name/role/state).
    #[must_use]
    pub fn accessible_label(&self) -> String {
        match (&self.disabled_reason, self.enabled) {
            (Some(reason), false) => format!("{} (disabled: {reason})", self.label),
            _ => self.label.clone(),
        }
    }
}

/// The tint of a [`FailureNotice`] (prototype `banner-err` / `banner-warn`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeTone {
    /// A hard failure (red).
    Error,
    /// A limit / degraded condition (amber).
    Warn,
}

/// The brief-§8 named error pattern (prototype `FailureNotice` in `primitives.jsx`):
/// bold title, plain-language (optionally monospace) reason, next actions, and an
/// optional reassurance note — a failure is never a dead end.
#[derive(Debug, Clone, PartialEq)]
pub struct FailureNotice {
    /// Banner tint.
    pub tone: NoticeTone,
    /// Bold title line.
    pub title: String,
    /// Plain-language reason under the title.
    pub reason: String,
    /// Whether the reason renders as terminal-ish monospace (raw tool output).
    pub mono_reason: bool,
    /// Next actions — never empty (a failure always offers a way forward).
    pub actions: Vec<ActionButton>,
    /// The `.fnotice-note` reassurance line (e.g. "No session was created …").
    pub note: Option<String>,
}

/// An inline field-level validation message (prototype `FieldError`): the message plus
/// whether it renders monospace (raw command output, e.g. "command not found …").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    /// The message.
    pub text: String,
    /// Monospace treatment for raw-output errors.
    pub mono: bool,
}

impl FieldError {
    /// A plain-language field error.
    #[must_use]
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            mono: false,
        }
    }

    /// A monospace (raw tool output) field error.
    #[must_use]
    pub fn mono(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            mono: true,
        }
    }
}

/// The empty / first-run state pattern (prototype `EmptyState` in `primitives.jsx`):
/// icon, title, an explanatory body (why it is empty), and next actions.
#[derive(Debug, Clone, PartialEq)]
pub struct EmptyState {
    /// Icon hint for the renderer.
    pub icon: &'static str,
    /// Title line.
    pub title: &'static str,
    /// Explanatory body — always says *why* nothing is here.
    pub body: &'static str,
    /// Next actions.
    pub actions: Vec<ActionButton>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    #[test]
    fn awaiting_badge_uses_awaiting_hue_and_label() {
        let theme = Theme::host_default();
        let badge = StatusBadge::session(SessionStatus::AwaitingConfirmation, &theme);
        assert_eq!(badge.tone, StatusTone::Awaiting);
        assert_eq!(badge.label, "Awaiting confirmation");
        assert!(!badge.glyph.is_empty());

        // Waiting-for-input shares the purple family but is distinguished by label;
        // Unknown reads "Connection lost" (design/README.md status table).
        let waiting = StatusBadge::session(SessionStatus::WaitingForInput, &theme);
        assert_eq!(waiting.tone, StatusTone::Awaiting);
        assert_eq!(waiting.label, "Waiting for input");
        let unknown = StatusBadge::session(SessionStatus::Unknown, &theme);
        assert_eq!(unknown.tone, StatusTone::Unknown);
        assert_eq!(unknown.label, "Connection lost");
    }

    #[test]
    fn chips_show_an_availability_dot_only_when_not_available() {
        // Prototype `primitives.jsx` availDotColor: the chip dot is rendered ONLY when
        // availability != "available" (FR-028, T083).
        use daedalus_proto::{Availability, BackendKind, SourceKind};

        let theme = Theme::host_default();
        let ok = Chip::backend(BackendKind::Workshop, Availability::Available, &theme);
        assert_eq!(ok.label, "Workshop");
        assert_eq!(ok.dot, None, "an available chip renders no dot");

        let degraded = Chip::backend(BackendKind::Fake, Availability::Degraded, &theme);
        assert_eq!(degraded.label, "Fake");
        assert_eq!(
            degraded.dot,
            Some(theme.status_color(StatusTone::Stalled)),
            "degraded uses the stalled hue (prototype AvailDot)"
        );

        let down = Chip::source(
            SourceKind::TunneledWorkshop,
            Availability::Unavailable,
            &theme,
        );
        assert_eq!(down.label, "Tunnel");
        assert_eq!(down.dot, Some(theme.status_color(StatusTone::Failed)));
        // The state is never conveyed by the dot color alone (WCAG 1.4.1).
        assert!(down.accessible_label().contains("unavailable"));
        assert_eq!(
            Chip::source(SourceKind::Mdns, Availability::Available, &theme).label,
            "mDNS"
        );
    }

    #[test]
    fn meter_fraction_is_clamped() {
        assert_eq!(
            Meter {
                value: 5.0,
                max: 10.0
            }
            .fraction(),
            0.5
        );
        assert_eq!(
            Meter {
                value: 20.0,
                max: 10.0
            }
            .fraction(),
            1.0
        );
        assert_eq!(
            Meter {
                value: 1.0,
                max: 0.0
            }
            .fraction(),
            0.0
        );
    }
}
