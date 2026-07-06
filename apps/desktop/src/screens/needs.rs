//! Needs-you surfaces (US6, FR-021a/b): the design's `needs.jsx` — one `NeedsRow`
//! rendered in three placements: `NeedsView` (dedicated screen, the default placement),
//! `NeedsTray` (header popover), and `NeedsStrip` (pinned band on the Tasks home).
//! Activating a row deep-links into the session with answer-mode focus (T078).

use std::time::Duration;

use daedalus_app::{App, AppQuery};
use daedalus_proto::{AttentionItem, AttentionKind, CostIndication, SessionId};

use crate::components::{ActionButton, ButtonIntent};
use crate::screens::session::{SessionFocus, SessionLink};
use crate::theme::StatusTone;

/// The big "time waiting" readout — minutes/hours, calm but legible (`needs.jsx`).
#[must_use]
pub fn wait_label(waiting: Duration) -> String {
    let sec = waiting.as_secs();
    if sec < 60 {
        return format!("{sec}s");
    }
    let minutes = sec / 60;
    if minutes < 60 {
        return format!("{minutes}m");
    }
    format!("{}h {}m", minutes / 60, minutes % 60)
}

/// The palette tone for an attention kind — waiting-input and confirm share the purple
/// "blocked on the operator" family (design/README.md status palette).
#[must_use]
pub fn kind_tone(kind: AttentionKind) -> StatusTone {
    match kind {
        AttentionKind::WaitingForInput | AttentionKind::AwaitingConfirmation => {
            StatusTone::Awaiting
        }
        AttentionKind::Stalled => StatusTone::Stalled,
        AttentionKind::Failed => StatusTone::Failed,
        AttentionKind::Disconnected => StatusTone::Unknown,
    }
}

/// The status glyph for an attention kind — waiting-input and confirm share a tone but
/// carry **distinct glyphs** (prototype `STATUS_GLYPH`: speech-bubble vs clipboard-check).
#[must_use]
pub fn kind_glyph(kind: AttentionKind) -> &'static str {
    match kind {
        AttentionKind::WaitingForInput => "?",
        AttentionKind::AwaitingConfirmation => "☑",
        other => kind_tone(other).glyph(),
    }
}

/// The short inline needs-tag cue per kind (prototype `NEEDS_META` in `primitives.jsx`):
/// the compact "blocked on you" marker shown in the Tasks and Sessions lists.
#[must_use]
pub fn kind_tag(kind: AttentionKind) -> &'static str {
    match kind {
        AttentionKind::WaitingForInput => "Asked a question",
        AttentionKind::AwaitingConfirmation => "Confirm completion",
        AttentionKind::Stalled => "Stalled",
        AttentionKind::Failed => "Failed",
        AttentionKind::Disconnected => "Connection lost",
    }
}

/// The cue line per kind (prototype `data.js needsYou()`).
#[must_use]
pub fn kind_cue(kind: AttentionKind) -> &'static str {
    match kind {
        AttentionKind::WaitingForInput => "Asked you a question",
        AttentionKind::AwaitingConfirmation => "Run ended — confirm completion",
        AttentionKind::Stalled => "No output — may be stuck",
        AttentionKind::Failed => "Run failed — needs a call",
        AttentionKind::Disconnected => "Connection lost",
    }
}

/// The idle-cost chip on a row (FR-021b): "idle · $x.xx" for a live blocked sandbox,
/// "env held" for an ended session still holding its environment, absent otherwise.
#[derive(Debug, Clone, PartialEq)]
pub enum CostChip {
    /// Sandbox time billed while the session sits blocked (accrued USD estimate).
    Idle(f64),
    /// Session ended — its environment is still allocated.
    EnvHeld,
}

impl CostChip {
    /// Shape a core cost indication into a chip (`None` renders no chip).
    #[must_use]
    pub fn from_indication(cost: &CostIndication) -> Option<Self> {
        match cost {
            CostIndication::Idle(usd) => Some(Self::Idle(*usd)),
            CostIndication::EnvHeld => Some(Self::EnvHeld),
            CostIndication::None => None,
        }
    }

    /// The chip text.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Idle(usd) => format!("idle · ${usd:.2}"),
            Self::EnvHeld => "env held".to_string(),
        }
    }
}

/// One queue entry (`NeedsRow` in `needs.jsx`) — the same row renders in card and line
/// tones across the three placements.
#[derive(Debug, Clone)]
pub struct NeedsRow {
    /// The blocked session; activation deep-links to it.
    pub session: SessionId,
    /// Why it needs the operator.
    pub kind: AttentionKind,
    /// Status tone (shared purple for waiting-input + confirm).
    pub tone: StatusTone,
    /// Status glyph (distinct per kind, paired with the tone — never color alone).
    pub glyph: &'static str,
    /// Cue line ("Asked you a question", …).
    pub cue: &'static str,
    /// Tool name.
    pub tool: String,
    /// Objective line.
    pub objective: String,
    /// The reason: pending question / exit summary / stall-fail-drop reason.
    pub reason: String,
    /// Whether the reason renders in quotes (a pending question does).
    pub quoted: bool,
    /// Time-waiting readout (e.g. "8m").
    pub wait_label: String,
    /// Idle-cost chip, when the waiting has a cost.
    pub cost: Option<CostChip>,
    /// The primary action ("Answer in terminal" / "Review & confirm").
    pub action: ActionButton,
}

impl NeedsRow {
    /// Shape one attention item (plus its session's tool/objective) into a row.
    #[must_use]
    pub fn from_item(item: &AttentionItem, tool: &str, objective: &str) -> Self {
        let action = match item.kind {
            AttentionKind::AwaitingConfirmation => "Review & confirm",
            _ => "Answer in terminal",
        };
        Self {
            session: item.session_id,
            kind: item.kind,
            tone: kind_tone(item.kind),
            glyph: kind_glyph(item.kind),
            cue: kind_cue(item.kind),
            tool: tool.to_string(),
            objective: objective.to_string(),
            reason: item.cue.clone(),
            quoted: item.kind == AttentionKind::WaitingForInput,
            wait_label: wait_label(item.waiting),
            cost: CostChip::from_indication(&item.cost),
            action: ActionButton::enabled(action, ButtonIntent::Primary),
        }
    }

    /// Activation intent: jump straight into the session with answer-mode focus (T078;
    /// prototype `onOpen(item.id, { answer: true })`).
    #[must_use]
    pub fn activate(&self) -> SessionLink {
        SessionLink {
            session: self.session,
            focus: Some(match self.kind {
                AttentionKind::AwaitingConfirmation => SessionFocus::ConfirmControls,
                _ => SessionFocus::AnswerPrompt,
            }),
        }
    }

    /// Screen-reader label for AccessKit (name/role/state — T065 pattern).
    #[must_use]
    pub fn accessible_label(&self) -> String {
        let cost = self
            .cost
            .as_ref()
            .map(|c| format!(" {}.", c.label()))
            .unwrap_or_default();
        format!(
            "{}: {} ({}). {}. Waiting {}.{} {}.",
            self.cue,
            self.objective,
            self.tool,
            self.reason,
            self.wait_label,
            cost,
            self.action.label
        )
    }
}

/// The "all caught up" empty state (`NeedsEmpty` in `needs.jsx`).
#[derive(Debug, Clone, Copy)]
pub struct NeedsEmpty {
    /// Title line.
    pub title: &'static str,
    /// Subtitle line.
    pub subtitle: &'static str,
}

impl Default for NeedsEmpty {
    fn default() -> Self {
        Self {
            title: "You're all caught up",
            subtitle: "No agents are waiting on you. Daedalus will surface anything that \
                       asks for input or stalls.",
        }
    }
}

/// One per-kind count in the dedicated screen's header stats (e.g. "2 awaiting answer").
#[derive(Debug, Clone)]
pub struct StatPart {
    /// The kind counted.
    pub kind: AttentionKind,
    /// Its tone (colored dot + text, never color alone).
    pub tone: StatusTone,
    /// How many.
    pub count: usize,
    /// The rendered part (count + noun).
    pub label: String,
}

/// Variation A — the dedicated full screen (default placement; "Needs you" nav item).
#[derive(Debug, Clone)]
pub struct NeedsView {
    /// Screen title.
    pub title: &'static str,
    /// Screen subtitle.
    pub subtitle: &'static str,
    /// Per-kind counts (only non-zero kinds, queue order).
    pub stats: Vec<StatPart>,
    /// Queue rows, most answerable first.
    pub rows: Vec<NeedsRow>,
    /// The empty state, when nothing needs you.
    pub empty: Option<NeedsEmpty>,
}

impl NeedsView {
    /// Build from current state.
    #[must_use]
    pub fn build(app: &App) -> Self {
        let rows = rows(app);
        // Only non-zero kinds appear, in queue order, with the prototype's nouns.
        let stats = [
            (AttentionKind::WaitingForInput, "awaiting answer"),
            (AttentionKind::AwaitingConfirmation, "to confirm"),
            (AttentionKind::Stalled, "stalled"),
            (AttentionKind::Failed, "failed"),
            (AttentionKind::Disconnected, "disconnected"),
        ]
        .into_iter()
        .filter_map(|(kind, noun)| {
            let count = rows.iter().filter(|r| r.kind == kind).count();
            (count > 0).then(|| StatPart {
                kind,
                tone: kind_tone(kind),
                count,
                label: format!("{count} {noun}"),
            })
        })
        .collect();
        Self {
            title: "Needs you",
            subtitle: "Sessions blocked on you — questions to answer, stalls and failures \
                       to clear. Open one to drop straight into its terminal.",
            stats,
            empty: rows.is_empty().then(NeedsEmpty::default),
            rows,
        }
    }

    /// The header stats joined for rendering (e.g. "2 awaiting answer · 1 to confirm").
    #[must_use]
    pub fn stats_line(&self) -> String {
        self.stats
            .iter()
            .map(|p| p.label.as_str())
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

/// Variation B — the header popover (`NeedsTray`), triageable from anywhere.
#[derive(Debug, Clone)]
pub struct NeedsTray {
    /// Popover title.
    pub title: &'static str,
    /// Summary line (e.g. "1 awaiting · 4 total").
    pub summary: String,
    /// Queue rows (rendered in the compact line tone).
    pub rows: Vec<NeedsRow>,
    /// The compact empty state, when nothing needs you.
    pub empty: Option<NeedsEmpty>,
}

impl NeedsTray {
    /// Build from current state.
    #[must_use]
    pub fn build(app: &App) -> Self {
        let rows = rows(app);
        let awaiting = rows
            .iter()
            .filter(|r| r.kind == AttentionKind::WaitingForInput)
            .count();
        Self {
            title: "Needs you",
            summary: format!("{awaiting} awaiting · {} total", rows.len()),
            empty: rows.is_empty().then(NeedsEmpty::default),
            rows,
        }
    }
}

/// Variation C — the pinned band at the top of the Tasks home (`NeedsStrip`).
#[derive(Debug, Clone)]
pub struct NeedsStrip {
    /// Headline (e.g. "3 sessions need you").
    pub headline: String,
    /// Per-kind summary (e.g. "1 asked a question · 1 to confirm · 1 stalled or failed").
    pub summary: String,
    /// Queue rows (rendered as compact chips on the rail).
    pub rows: Vec<NeedsRow>,
    /// Whether the rail is collapsed to the summary line.
    pub collapsed: bool,
}

impl NeedsStrip {
    /// Build from current state — `None` when nothing needs you (the strip disappears).
    #[must_use]
    pub fn build(app: &App) -> Option<Self> {
        let rows = rows(app);
        if rows.is_empty() {
            return None;
        }
        let n = rows.len();
        let awaiting = rows
            .iter()
            .filter(|r| r.kind == AttentionKind::WaitingForInput)
            .count();
        let confirm = rows
            .iter()
            .filter(|r| r.kind == AttentionKind::AwaitingConfirmation)
            .count();
        let rest = n - awaiting - confirm;
        let summary = [
            (awaiting > 0).then(|| format!("{awaiting} asked a question")),
            (confirm > 0).then(|| format!("{confirm} to confirm")),
            (rest > 0).then(|| format!("{rest} stalled or failed")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
        Some(Self {
            headline: format!(
                "{n} {} you",
                if n == 1 {
                    "session needs"
                } else {
                    "sessions need"
                }
            ),
            summary,
            rows,
            collapsed: false,
        })
    }

    /// The collapse-toggle label.
    #[must_use]
    pub fn toggle_label(&self) -> &'static str {
        if self.collapsed {
            "Show"
        } else {
            "Hide"
        }
    }
}

/// The shared queue: attention items joined with their sessions' tool/objective.
fn rows(app: &App) -> Vec<NeedsRow> {
    let fleet = app.fleet();
    app.needs_you()
        .iter()
        .map(|item| {
            let summary = fleet.iter().find(|s| s.id == item.session_id);
            NeedsRow::from_item(
                item,
                summary.map_or("", |s| s.tool_name.as_str()),
                summary.map_or("", |s| s.objective.as_str()),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use daedalus_proto::SessionStatus;

    fn item(
        kind: AttentionKind,
        reason: &str,
        waiting: Duration,
        cost: CostIndication,
    ) -> AttentionItem {
        AttentionItem {
            session_id: SessionId::new(),
            kind,
            cue: reason.to_string(),
            waiting,
            cost,
        }
    }

    #[test]
    fn wait_label_formats_seconds_minutes_and_hours() {
        assert_eq!(wait_label(Duration::from_secs(42)), "42s");
        assert_eq!(wait_label(Duration::from_secs(8 * 60)), "8m");
        assert_eq!(wait_label(Duration::from_secs(65 * 60)), "1h 5m");
    }

    #[test]
    fn inline_needs_tags_use_the_short_design_cues() {
        // Prototype `NEEDS_META` (primitives.jsx): the compact marker shown inline in the
        // Tasks and Sessions lists.
        assert_eq!(kind_tag(AttentionKind::WaitingForInput), "Asked a question");
        assert_eq!(
            kind_tag(AttentionKind::AwaitingConfirmation),
            "Confirm completion"
        );
        assert_eq!(kind_tag(AttentionKind::Stalled), "Stalled");
        assert_eq!(kind_tag(AttentionKind::Failed), "Failed");
        assert_eq!(kind_tag(AttentionKind::Disconnected), "Connection lost");
    }

    #[test]
    fn rows_carry_the_design_cues_glyphs_costs_and_actions() {
        let waiting = NeedsRow::from_item(
            &item(
                AttentionKind::WaitingForInput,
                "Continue? (y/n)",
                Duration::from_secs(480),
                CostIndication::Idle(0.13),
            ),
            "claude",
            "Migrate billing",
        );
        assert_eq!(waiting.cue, "Asked you a question");
        assert!(waiting.quoted, "a pending question renders in quotes");
        assert_eq!(waiting.wait_label, "8m");
        assert_eq!(waiting.cost.as_ref().unwrap().label(), "idle · $0.13");
        assert_eq!(waiting.action.label, "Answer in terminal");

        let confirm = NeedsRow::from_item(
            &item(
                AttentionKind::AwaitingConfirmation,
                "2 of 4 tracked tasks done",
                Duration::from_secs(60),
                CostIndication::EnvHeld,
            ),
            "claude",
            "Migrate billing",
        );
        assert_eq!(confirm.cue, "Run ended — confirm completion");
        assert!(!confirm.quoted);
        assert_eq!(confirm.cost.as_ref().unwrap().label(), "env held");
        assert_eq!(confirm.action.label, "Review & confirm");

        // Shared purple tone, distinct glyphs (design/README.md status palette).
        assert_eq!(waiting.tone, StatusTone::Awaiting);
        assert_eq!(confirm.tone, StatusTone::Awaiting);
        assert_ne!(waiting.glyph, confirm.glyph);

        let stalled = NeedsRow::from_item(
            &item(
                AttentionKind::Stalled,
                "no output for 12m",
                Duration::from_secs(720),
                CostIndication::None,
            ),
            "goose",
            "Fix flaky tests",
        );
        assert_eq!(stalled.cue, "No output — may be stuck");
        assert_eq!(stalled.tone, StatusTone::Stalled);
        assert!(stalled.cost.is_none(), "no idle rate — no cost chip");

        let failed = NeedsRow::from_item(
            &item(
                AttentionKind::Failed,
                "exit 1",
                Duration::from_secs(5),
                CostIndication::None,
            ),
            "goose",
            "Fix flaky tests",
        );
        assert_eq!(failed.cue, "Run failed — needs a call");

        let lost = NeedsRow::from_item(
            &item(
                AttentionKind::Disconnected,
                "connection lost",
                Duration::from_secs(5),
                CostIndication::None,
            ),
            "goose",
            "Fix flaky tests",
        );
        assert_eq!(lost.cue, "Connection lost");

        // AccessKit: rows read their cue, objective, and action (T065 pattern).
        let label = waiting.accessible_label();
        assert!(label.contains("Asked you a question"));
        assert!(label.contains("Migrate billing"));
        assert!(label.contains("Answer in terminal"));
    }

    #[test]
    fn activation_deep_links_with_answer_mode_focus() {
        let waiting = NeedsRow::from_item(
            &item(
                AttentionKind::WaitingForInput,
                "Continue?",
                Duration::ZERO,
                CostIndication::None,
            ),
            "claude",
            "obj",
        );
        let link = waiting.activate();
        assert_eq!(link.session, waiting.session);
        assert_eq!(link.focus, Some(SessionFocus::AnswerPrompt));

        let confirm = NeedsRow::from_item(
            &item(
                AttentionKind::AwaitingConfirmation,
                "done",
                Duration::ZERO,
                CostIndication::None,
            ),
            "claude",
            "obj",
        );
        assert_eq!(
            confirm.activate().focus,
            Some(SessionFocus::ConfirmControls)
        );

        // Non-question kinds still drop into the terminal (prototype: answer: true).
        let stalled = NeedsRow::from_item(
            &item(
                AttentionKind::Stalled,
                "stuck",
                Duration::ZERO,
                CostIndication::None,
            ),
            "claude",
            "obj",
        );
        assert_eq!(stalled.activate().focus, Some(SessionFocus::AnswerPrompt));
    }

    #[tokio::test]
    async fn placements_build_header_stats_summaries_and_the_empty_state() {
        let fx = daedalus_tests::Fixture::new();

        // Empty queue: the caught-up state, and no strip at all.
        let view = NeedsView::build(&fx.app);
        assert!(view.rows.is_empty());
        assert_eq!(view.empty.unwrap().title, "You're all caught up");
        assert!(NeedsStrip::build(&fx.app).is_none());
        assert_eq!(
            NeedsTray::build(&fx.app).empty.unwrap().title,
            "You're all caught up"
        );

        // One session asked a question; one exited cleanly with unfinished tasks.
        let tool = fx.register_sample_tool("claude");
        let ask = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
        fx.core
            .mark_waiting_for_input(ask, "Continue? (y/n)")
            .unwrap();
        let req = fx.fresh_request(tool);
        let objective = req.objective.clone();
        fx.write_tasks(&objective, "- [ ] T001 Implement\n");
        let confirm = fx.core.start_session(req).await.unwrap();
        fx.core
            .apply_signal(confirm, daedalus_core::AgentSignal::ExitedCleanly)
            .await
            .unwrap();

        let view = NeedsView::build(&fx.app);
        assert_eq!(view.title, "Needs you");
        assert_eq!(view.rows.len(), 2);
        assert_eq!(view.stats_line(), "1 awaiting answer · 1 to confirm");
        assert!(view.empty.is_none());
        // Most answerable first (FR-021a), with the session's tool + objective joined in.
        assert_eq!(view.rows[0].kind, AttentionKind::WaitingForInput);
        assert_eq!(view.rows[0].tool, "claude");
        assert!(!view.rows[0].objective.is_empty());
        assert_eq!(view.rows[0].reason, "Continue? (y/n)");
        let statuses: Vec<_> = fx.app.fleet().iter().map(|s| s.status).collect();
        assert!(statuses.contains(&SessionStatus::WaitingForInput));

        let tray = NeedsTray::build(&fx.app);
        assert_eq!(tray.title, "Needs you");
        assert_eq!(tray.summary, "1 awaiting · 2 total");
        assert_eq!(tray.rows.len(), 2);

        let strip = NeedsStrip::build(&fx.app).unwrap();
        assert_eq!(strip.headline, "2 sessions need you");
        assert_eq!(strip.summary, "1 asked a question · 1 to confirm");
        assert!(!strip.collapsed);
        assert_eq!(strip.toggle_label(), "Hide");
        let collapsed = NeedsStrip {
            collapsed: true,
            ..strip
        };
        assert_eq!(collapsed.toggle_label(), "Show");
    }
}
