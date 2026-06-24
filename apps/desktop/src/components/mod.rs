//! Reusable component view-models ported from `design/components.css` + `*.jsx`.
//!
//! Each is a render-agnostic descriptor the GPUI layer turns into widgets (behind the
//! `gpui` feature). Keeping them as data makes the design mapping unit-testable headlessly.

use daedalus_proto::{SessionStatus, TaskStatus};

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
    /// Build a badge for a session status under a theme.
    #[must_use]
    pub fn session(status: SessionStatus, theme: &Theme) -> Self {
        let tone = StatusTone::from_session(status);
        Self::from_tone(tone, theme)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    #[test]
    fn awaiting_badge_uses_awaiting_hue_and_label() {
        let theme = Theme::host_default();
        let badge = StatusBadge::session(SessionStatus::AwaitingConfirmation, &theme);
        assert_eq!(badge.tone, StatusTone::Awaiting);
        assert_eq!(badge.label, "Awaiting");
        assert!(!badge.glyph.is_empty());
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
