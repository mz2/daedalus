//! Notifications view-model (FR-021): the design's `NotifPopover`
//! (`design/prototype/palette.jsx`). Every item carries the session id so activating it
//! jumps to the session, plus the design language for the blocked-on-operator kinds —
//! "Asked a question" (waiting-input) and "Confirm completion" (confirm), both on the
//! shared purple tone (design/README.md status palette).

use daedalus_proto::{AttentionKind, SessionId, SessionStatus, TerminalOrAbnormal};

use crate::components::StatusBadge;
use crate::screens::session::{SessionFocus, SessionLink};
use crate::theme::{StatusTone, Theme};

/// One row in the notifications popover.
#[derive(Debug, Clone)]
pub struct NotificationItem {
    /// The session to jump to on activation (FR-021).
    pub session: SessionId,
    /// Status badge (color + glyph + label — never color alone).
    pub badge: StatusBadge,
    /// Title line, in the design's language per kind.
    pub title: &'static str,
    /// Body line: the question / exit summary / reason, when the event carries one.
    pub body: String,
    /// Deep-link focus for the blocked-on-operator kinds (T078 answer mode).
    pub focus: Option<SessionFocus>,
}

impl NotificationItem {
    /// Shape a core notification event into a popover row.
    #[must_use]
    pub fn from_notification(n: &TerminalOrAbnormal, theme: &Theme) -> Self {
        let title = match n.status {
            SessionStatus::WaitingForInput => "Asked a question",
            SessionStatus::AwaitingConfirmation => "Confirm completion",
            SessionStatus::Stalled => "Session stalled",
            SessionStatus::Failed => "Session failed",
            SessionStatus::Unknown => "Connection lost",
            SessionStatus::Completed => "Session completed",
            SessionStatus::Stopped => "Session stopped",
            SessionStatus::Starting | SessionStatus::Running => "Session update",
        };
        let body = n
            .note
            .clone()
            .unwrap_or_else(|| StatusTone::session_label(n.status).to_string());
        let focus = AttentionKind::from_status(n.status).and_then(SessionFocus::for_kind);
        Self {
            session: n.session,
            badge: StatusBadge::session(n.status, theme),
            title,
            body,
            focus,
        }
    }

    /// Activation intent (FR-021, T078): jump to the session, landing in answer mode when
    /// the notification is a question, or on the confirm controls for a confirm kind.
    #[must_use]
    pub fn activate(&self) -> SessionLink {
        SessionLink {
            session: self.session,
            focus: self.focus,
        }
    }

    /// Screen-reader label for AccessKit (name/role/state): kind, detail, and target.
    #[must_use]
    pub fn accessible_label(&self) -> String {
        format!("{}: {}. Opens the session.", self.title, self.body)
    }
}

/// The notifications popover: newest first, as delivered by the event stream.
#[derive(Debug, Clone, Default)]
pub struct NotificationsView {
    /// All rows.
    pub items: Vec<NotificationItem>,
}

impl NotificationsView {
    /// Build from the notification events the shell has collected.
    #[must_use]
    pub fn build(notifications: &[TerminalOrAbnormal], theme: &Theme) -> Self {
        Self {
            items: notifications
                .iter()
                .map(|n| NotificationItem::from_notification(n, theme))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::StatusTone;

    fn event(status: SessionStatus, note: Option<&str>) -> TerminalOrAbnormal {
        TerminalOrAbnormal {
            session: SessionId::new(),
            status,
            note: note.map(str::to_string),
        }
    }

    #[test]
    fn blocked_on_operator_kinds_use_the_design_language_on_the_purple_tone() {
        let theme = Theme::host_default();
        let asked = NotificationItem::from_notification(
            &event(SessionStatus::WaitingForInput, Some("Continue? (y/n)")),
            &theme,
        );
        assert_eq!(asked.title, "Asked a question");
        assert_eq!(asked.body, "Continue? (y/n)");
        assert_eq!(asked.badge.tone, StatusTone::Awaiting);

        let confirm = NotificationItem::from_notification(
            &event(
                SessionStatus::AwaitingConfirmation,
                Some("2 of 4 tracked tasks done"),
            ),
            &theme,
        );
        assert_eq!(confirm.title, "Confirm completion");
        assert_eq!(confirm.body, "2 of 4 tracked tasks done");
        assert_eq!(confirm.badge.tone, StatusTone::Awaiting);
    }

    #[test]
    fn every_item_carries_the_session_to_jump_to_and_a_body_fallback() {
        let theme = Theme::host_default();
        let events = [
            event(SessionStatus::Failed, Some("agent crashed")),
            event(SessionStatus::Stalled, None),
            event(SessionStatus::Unknown, None),
            event(SessionStatus::Completed, None),
        ];
        let view = NotificationsView::build(&events, &theme);
        assert_eq!(view.items.len(), 4);
        for (item, ev) in view.items.iter().zip(&events) {
            assert_eq!(item.session, ev.session, "activation jumps to the session");
            assert!(
                !item.body.is_empty(),
                "a note-less event still reads clearly"
            );
            assert!(item.accessible_label().contains(item.title));
        }
        assert_eq!(view.items[1].body, "Stalled");
        assert_eq!(view.items[2].title, "Connection lost");
    }

    #[test]
    fn activation_carries_answer_mode_focus_for_blocked_on_operator_kinds() {
        let theme = Theme::host_default();
        let asked = NotificationItem::from_notification(
            &event(SessionStatus::WaitingForInput, Some("Continue?")),
            &theme,
        );
        assert_eq!(asked.activate().session, asked.session);
        assert_eq!(asked.activate().focus, Some(SessionFocus::AnswerPrompt));

        let confirm = NotificationItem::from_notification(
            &event(SessionStatus::AwaitingConfirmation, None),
            &theme,
        );
        assert_eq!(
            confirm.activate().focus,
            Some(SessionFocus::ConfirmControls)
        );

        let failed =
            NotificationItem::from_notification(&event(SessionStatus::Failed, None), &theme);
        assert_eq!(failed.activate().focus, None, "no answer mode to land in");
    }
}
