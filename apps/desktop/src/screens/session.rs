//! Session detail screen (US2/US4, §6.4): embedded terminal + task board + telemetry +
//! lifecycle controls. The control set encodes the v1 capability rules — Send-input is
//! disabled with a reason when the tool does not accept it; Confirm appears only when the
//! session is awaiting confirmation; there is **no** pause/resume.

use daedalus_app::{App, AppQuery};
use daedalus_proto::{
    AttentionKind, EventPayload, EventRecord, MetricKind, OperatorAction, ResourceUsageMetric,
    SessionDetail, SessionId, SessionStatus, TaskStatus, Timestamp,
};

use crate::components::{ActionButton, ButtonIntent, Chip, Meter, StatusBadge};
use crate::screens::needs::wait_label;
use crate::theme::{StatusTone, Theme};

/// The review-mode input notice for an ended session (prototype `Terminal` disabled row).
pub const REVIEW_MODE_NOTICE: &str = "Session ended — output is read-only (review mode).";

/// The terminal's bounded display scrollback, in lines (FR-016a). The persisted capture
/// always holds the full output; only the display trims.
pub const TERMINAL_SCROLLBACK_LINES: u64 = 10_000;

/// How many recent samples feed a telemetry-rail sparkline (prototype `Sparkline`).
pub const SPARKLINE_SAMPLES: usize = 28;

/// Format an integer with thousands separators (prototype `toLocaleString("en-US")`).
#[must_use]
pub fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Where the session detail should land focus when opened from a deep link (T078):
/// the prototype's answer mode (`design/prototype/session.jsx`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionFocus {
    /// Scroll the pending question into view and focus the terminal input.
    AnswerPrompt,
    /// Focus the confirm-completion controls (confirm-kind items).
    ConfirmControls,
}

impl SessionFocus {
    /// The deep-link focus an attention kind lands with, when the item is answerable:
    /// a question focuses the prompt, a confirm focuses the confirm controls; the
    /// other kinds have no answer mode to land in.
    #[must_use]
    pub fn for_kind(kind: AttentionKind) -> Option<Self> {
        match kind {
            AttentionKind::WaitingForInput => Some(Self::AnswerPrompt),
            AttentionKind::AwaitingConfirmation => Some(Self::ConfirmControls),
            AttentionKind::Stalled | AttentionKind::Disconnected | AttentionKind::Failed => None,
        }
    }
}

/// A navigation intent to the session detail — what activating a Needs-you row or a
/// notification item yields (FR-021, T078).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionLink {
    /// The session to open.
    pub session: SessionId,
    /// The focus to land with, when the item is answerable.
    pub focus: Option<SessionFocus>,
}

/// A row on the per-session task board.
#[derive(Debug, Clone)]
pub struct TaskRow {
    /// Task id (e.g. T001).
    pub id: String,
    /// Description.
    pub description: String,
    /// Raw status (drives board-column grouping).
    pub status: TaskStatus,
    /// Status badge.
    pub badge: StatusBadge,
}

/// One column of the session task board (prototype `TaskBoard` `COLUMNS` in
/// `session.jsx`): a status bucket with its cards, rendered even when empty.
#[derive(Debug, Clone)]
pub struct BoardColumn {
    /// Column title ("To do", "In progress", "Blocked", "Done").
    pub title: &'static str,
    /// The status this column buckets.
    pub status: TaskStatus,
    /// Cards in board order.
    pub cards: Vec<TaskRow>,
}

/// A header meta chip (prototype `sess-chips`): spec branch, backend, isolation posture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaChip {
    /// Icon hint for the renderer's icon set.
    pub icon: &'static str,
    /// Chip text.
    pub label: String,
    /// Soft-positive tint (the green `worktree-isolated` chip).
    pub positive: bool,
}

/// A labelled telemetry meter.
#[derive(Debug, Clone, PartialEq)]
pub struct TelemetryRow {
    /// Metric label.
    pub label: String,
    /// Meter (value/max), or `None` when no meaningful ceiling is known — then the row is
    /// a plain value readout instead of an always-full bar (G3).
    pub meter: Option<Meter>,
    /// Human value text.
    pub value_text: String,
}

/// The meter for a telemetry metric, or `None` when no meaningful ceiling is known so the
/// row renders as a plain value readout rather than an always-full bar (G3). CPU is a
/// percentage (0–100); memory/disk/time carry no ceiling in the view-model (environment
/// [`daedalus_proto::ResourceLimits`] are not surfaced on the session/environment here), so
/// they have no fraction bar.
#[must_use]
fn telemetry_meter(metric: MetricKind, value: f64) -> Option<Meter> {
    match metric {
        MetricKind::Cpu => Some(Meter { value, max: 100.0 }),
        MetricKind::Memory | MetricKind::Disk | MetricKind::Time => None,
    }
}

/// Human-readable bytes (metrics carry memory/disk in bytes).
#[must_use]
pub fn human_bytes(v: f64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const KIB: f64 = 1024.0;
    if v >= GIB {
        format!("{:.1} GB", v / GIB)
    } else if v >= MIB {
        format!("{:.0} MB", v / MIB)
    } else if v >= KIB {
        format!("{:.0} KB", v / KIB)
    } else {
        format!("{v:.0} B")
    }
}

/// The timeline label for a lifecycle transition (FR-019a; prototype `EVENTS` shapes).
fn lifecycle_label(status: SessionStatus, note: Option<&str>) -> String {
    let base = match status {
        SessionStatus::Starting => "Created — provisioning environment",
        SessionStatus::Running => "Agent started",
        other => StatusTone::session_label(other),
    };
    match note {
        // Starting/stop notes repeat the base ("stopped by operator") — skip echoes.
        Some(note) if !base.eq_ignore_ascii_case(note) => format!("{base} — {note}"),
        _ => base.to_string(),
    }
}

/// The timeline label for a key operator action (FR-019a).
fn action_label(action: OperatorAction) -> &'static str {
    match action {
        OperatorAction::Start => "Started by operator",
        OperatorAction::Stop => "Stop requested by operator",
        // Historic only — no longer recorded (per-keystroke noise since live typing).
        OperatorAction::InputSent => "Input sent by operator",
        OperatorAction::ConfirmCompletion => "Completion confirmed by operator",
        OperatorAction::CleanUp => "Environment cleaned up",
    }
}

/// The design label for a task status (prototype `TASK_STATUS_LABEL`).
fn task_status_label(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Todo => "To do",
        TaskStatus::InProgress => "In progress",
        TaskStatus::Blocked => "Blocked",
        TaskStatus::Done => "Done",
    }
}

/// A sparkline-backed resource readout on the rail (`RailSpark`): current value in mono
/// plus the recent sample history, oldest → newest.
#[derive(Debug, Clone, PartialEq)]
pub struct ResourceSpark {
    /// Metric label ("CPU" / "Memory").
    pub label: &'static str,
    /// Current value text (e.g. "12%", "256 MB"), "—" when no sample exists.
    pub value_text: String,
    /// Recent sample values feeding the sparkline, oldest → newest.
    pub history: Vec<f64>,
}

/// What painted a timeline node (prototype `RAIL_EV_NODE`): a status color, the accent
/// (operator action), or the dim note treatment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelineKind {
    /// A lifecycle transition — node takes the status color.
    Status(SessionStatus),
    /// A key operator action — node takes the accent (FR-019a).
    Action(OperatorAction),
    /// A notable agent event (e.g. a task moved) — dim treatment.
    Note,
}

/// One entry on the rail's Timeline (FR-019a): kind, label, timestamp.
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineEntry {
    /// What kind of event this is (drives the node color).
    pub kind: TimelineKind,
    /// Human label.
    pub label: String,
    /// When it happened.
    pub timestamp: Timestamp,
}

/// The rail's Resources section: CPU/memory sparklines, the disk meter, and runtime.
#[derive(Debug, Clone, PartialEq)]
pub struct RailResources {
    /// CPU with recent history.
    pub cpu: ResourceSpark,
    /// Memory with recent history.
    pub memory: ResourceSpark,
    /// Disk as a current meter.
    pub disk: TelemetryRow,
    /// "Ran for" once the run is over (incl. awaiting confirmation), else "Runtime".
    pub runtime_label: &'static str,
    /// Elapsed running time (e.g. "1h 31m").
    pub runtime: String,
}

/// The Outcome block for an ended/unknown session (FR-019a): final state, exit
/// code/reason, and where the persisted capture lives.
#[derive(Debug, Clone, PartialEq)]
pub struct OutcomeBlock {
    /// Final (or current abnormal) state.
    pub state: SessionStatus,
    /// Its design label (e.g. "Stopped", "Connection lost").
    pub state_label: &'static str,
    /// The exit row: "SIGTERM (operator)" for stopped, "code N" when the agent exited.
    pub exit: Option<String>,
    /// Reason / exit-summary line.
    pub reason: Option<String>,
    /// The persisted-capture note.
    pub note: &'static str,
}

/// The session telemetry rail (FR-019/019a): Resources + Timeline + Outcome, with a
/// visibility toggle that defaults ON (prototype `railOpen`).
#[derive(Debug, Clone, PartialEq)]
pub struct TelemetryRail {
    /// Whether the rail is shown (default ON).
    pub open: bool,
    /// Resources — `None` while the session is `Unknown` (metrics never shown as healthy).
    pub resources: Option<RailResources>,
    /// The unknown-state notice replacing Resources (s-908).
    pub unavailable_notice: Option<&'static str>,
    /// Lifecycle / operator-action / note timeline, oldest → newest.
    pub timeline: Vec<TimelineEntry>,
    /// Outcome block for ended (incl. awaiting-confirmation) and unknown sessions.
    pub outcome: Option<OutcomeBlock>,
}

impl TelemetryRail {
    /// Flip the rail visibility.
    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    /// The toggle's label/tooltip (prototype `railOpen` icon button).
    #[must_use]
    pub fn toggle_label(&self) -> &'static str {
        if self.open {
            "Hide telemetry rail"
        } else {
            "Show telemetry rail"
        }
    }
}

/// The tint of a session [`StateBanner`] (prototype `banner-*` classes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BannerTone {
    /// Provisioning (blue).
    Info,
    /// Completed (green).
    Ok,
    /// Stalled (amber).
    Warn,
    /// Failed (red).
    Error,
    /// Stopped (grey).
    Neutral,
    /// Waiting for input (purple).
    Await,
    /// Awaiting confirmation (purple, clipboard-check).
    Confirm,
    /// Connection lost (grey warn).
    Unknown,
}

/// The display label for the objective's spec reference: the trailing `specs/…` fragment
/// of the artifact root when present (SpecKit convention), else its last path component.
fn spec_label(root: &str) -> Option<String> {
    let trimmed = root.trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    if let Some(idx) = trimmed.rfind("specs/") {
        return Some(trimmed[idx..].to_string());
    }
    trimmed.rsplit('/').next().map(str::to_string)
}

/// The per-status session banner (prototype `StateBanner` in `session.jsx`): a bold lead,
/// an optional plain continuation, an optional quoted line (the agent's question or exit
/// summary), an optional meta line, and inline actions.
#[derive(Debug, Clone, PartialEq)]
pub struct StateBanner {
    /// Banner tint.
    pub tone: BannerTone,
    /// Bold lead sentence (e.g. "Session failed.").
    pub lead: String,
    /// Plain continuation after the lead.
    pub body: Option<String>,
    /// Quoted line — the pending question (awaiting) or the agent summary (confirm).
    pub quote: Option<String>,
    /// Meta line (e.g. "waiting 8m", "exited cleanly (code 0) · 7/9 tasks · waiting 12m").
    pub meta: Option<String>,
    /// Inline banner actions.
    pub actions: Vec<ActionButton>,
}

/// The session detail view.
#[derive(Debug, Clone)]
pub struct SessionView {
    /// The session id.
    pub id: SessionId,
    /// Current status (drives the answer-mode rules).
    pub status: SessionStatus,
    /// Status badge for the header.
    pub badge: StatusBadge,
    /// Tool name.
    pub tool: String,
    /// Objective summary.
    pub objective: String,
    /// The agent's pending question, when it is waiting for input (FR-015b).
    pub pending_prompt: Option<String>,
    /// Answer mode (prototype `focusPrompt`): opened via a deep link while the agent is
    /// waiting — the terminal takes over and the input is focused on the prompt.
    pub answer_mode: bool,
    /// Where to land focus (scrolling/focus is renderer work; modeled as data here).
    pub focus_target: Option<SessionFocus>,
    /// The terminal input placeholder ("Reply to {tool}…" in answer mode).
    pub input_placeholder: String,
    /// The sticky trimmed-output notice (`term-trim`, FR-016a): set when the persisted
    /// capture exceeds the display scrollback; `None` while everything fits.
    pub trim_notice: Option<String>,
    /// The per-status state banner under the header (`None` while plainly running).
    pub banner: Option<StateBanner>,
    /// The disabled input-row notice: review mode for ended sessions, the no-input
    /// explanation for non-interactive tools; `None` when the input is live.
    pub input_notice: Option<String>,
    /// Header meta chips: spec ref, backend, isolation posture (prototype `sess-chips`).
    pub chips: Vec<MetaChip>,
    /// Task board rows.
    pub board: Vec<TaskRow>,
    /// Board progress: (done, total) — the Tasks panel "n/m" header.
    pub progress: (usize, usize),
    /// Compact telemetry rows (the collapsed-rail footer meters).
    pub telemetry: Vec<TelemetryRow>,
    /// The telemetry rail: Resources / Timeline / Outcome (FR-019/019a).
    pub rail: TelemetryRail,
    /// Lifecycle controls in header order.
    pub controls: Vec<ActionButton>,
}

impl SessionView {
    /// Build from current state: the session detail plus its event timeline and recent
    /// metric history for the rail sparklines.
    #[must_use]
    pub fn build(app: &App, id: SessionId, theme: &Theme) -> Option<Self> {
        let detail = app.session(id)?;
        let events = app.session_events(id);
        let cpu = app.resource_history(id, MetricKind::Cpu, SPARKLINE_SAMPLES);
        let memory = app.resource_history(id, MetricKind::Memory, SPARKLINE_SAMPLES);
        // Resolve the hosting backend's display label for the starting banner
        // ("A fresh sandbox is being created on {backend}…").
        let backend_label = app
            .core()
            .backend_registry()
            .by_id(detail.environment.backend_id)
            .map(|b| Chip::backend(b.kind(), daedalus_proto::Availability::Available, theme).label);
        Some(Self::from_parts(
            detail,
            &events,
            &cpu,
            &memory,
            backend_label,
            theme,
        ))
    }

    /// Build directly from a [`SessionDetail`] (no timeline/history — rail sections that
    /// need them come back empty).
    #[must_use]
    pub fn from_detail(detail: SessionDetail, theme: &Theme) -> Self {
        Self::from_parts(detail, &[], &[], &[], None, theme)
    }

    /// Build from a detail plus the persisted events and recent CPU/memory histories.
    #[must_use]
    pub fn from_parts(
        detail: SessionDetail,
        events: &[EventRecord],
        cpu_history: &[ResourceUsageMetric],
        memory_history: &[ResourceUsageMetric],
        backend_label: Option<&'static str>,
        theme: &Theme,
    ) -> Self {
        let board: Vec<TaskRow> = detail
            .tasks
            .iter()
            .map(|t| TaskRow {
                id: t.id.0.clone(),
                description: t.description.clone(),
                status: t.status,
                badge: StatusBadge::task(t.status, theme),
            })
            .collect();
        let progress = (
            board
                .iter()
                .filter(|t| t.status == TaskStatus::Done)
                .count(),
            board.len(),
        );
        let chips = Self::meta_chips(&detail, backend_label);

        let telemetry = detail
            .metrics
            .iter()
            .map(|m| TelemetryRow {
                label: format!("{:?}", m.metric),
                meter: telemetry_meter(m.metric, m.value),
                value_text: format!("{:.0}", m.value),
            })
            .collect();

        let controls = Self::controls(&detail, theme);

        let total = detail.capture.total_lines;
        let trim_notice = (total > TERMINAL_SCROLLBACK_LINES).then(|| {
            format!(
                "Output trimmed — showing last {} of {} lines · full log persisted",
                thousands(TERMINAL_SCROLLBACK_LINES),
                thousands(total)
            )
        });

        let rail = Self::rail(&detail, events, cpu_history, memory_history);
        let banner = Self::banner(&detail, events, backend_label);
        let ended = detail.session.status.has_ended();
        let input_notice = if ended {
            Some(REVIEW_MODE_NOTICE.to_string())
        } else if !detail.session.accepts_input {
            Some(format!(
                "{} does not accept interactive input.",
                detail.tool.name
            ))
        } else {
            None
        };

        Self {
            id: detail.session.id,
            status: detail.session.status,
            badge: StatusBadge::session(detail.session.status, theme),
            tool: detail.tool.name,
            objective: detail.objective.description,
            pending_prompt: detail.session.pending_prompt.clone(),
            answer_mode: false,
            focus_target: None,
            input_placeholder: "Send input to the focused pane…  (↵ to send)".to_string(),
            trim_notice,
            banner,
            input_notice,
            chips,
            board,
            progress,
            telemetry,
            rail,
            controls,
        }
    }

    /// Header meta chips (prototype `sess-chips`): the spec reference the objective lives
    /// on, the hosting backend, and the isolation posture — `worktree-isolated` (green,
    /// FR-002a) for pre-existing worktrees, `fresh environment` otherwise.
    #[must_use]
    pub fn meta_chips(
        detail: &SessionDetail,
        backend_label: Option<&'static str>,
    ) -> Vec<MetaChip> {
        let mut chips = Vec::new();
        if let Some(spec) = spec_label(&detail.objective.artifact_ref.root) {
            chips.push(MetaChip {
                icon: "doc",
                label: spec,
                positive: false,
            });
        }
        if let Some(backend) = backend_label {
            chips.push(MetaChip {
                icon: "backends",
                label: backend.to_string(),
                positive: false,
            });
        }
        chips.push(match detail.environment.worktree_ref {
            Some(_) => MetaChip {
                icon: "shield",
                label: "worktree-isolated".into(),
                positive: true,
            },
            None => MetaChip {
                icon: "shield",
                label: "fresh environment".into(),
                positive: false,
            },
        });
        chips
    }

    /// Group the board into the prototype's columns (`session.jsx` `COLUMNS`), every
    /// column present even when empty so counts read at a glance.
    #[must_use]
    pub fn board_columns(&self) -> Vec<BoardColumn> {
        [
            ("To do", TaskStatus::Todo),
            ("In progress", TaskStatus::InProgress),
            ("Blocked", TaskStatus::Blocked),
            ("Done", TaskStatus::Done),
        ]
        .into_iter()
        .map(|(title, status)| BoardColumn {
            title,
            status,
            cards: self
                .board
                .iter()
                .filter(|t| t.status == status)
                .cloned()
                .collect(),
        })
        .collect()
    }

    /// Shape the per-status state banner (prototype `StateBanner`): every non-plain state
    /// leads with what happened, quotes the agent where it spoke, and offers actions.
    fn banner(
        detail: &SessionDetail,
        events: &[EventRecord],
        backend_label: Option<&'static str>,
    ) -> Option<StateBanner> {
        let session = &detail.session;
        let tool = &detail.tool.name;
        let outcome = session.terminal_outcome.clone().unwrap_or_default();
        let now = daedalus_core::clock::now();
        let since = |t: Timestamp| now.saturating_duration_since(&t);
        let waiting = session.waiting_since.map(since);
        let btn = |label: &str, intent| ActionButton::enabled(label, intent);

        Some(match session.status {
            SessionStatus::Running => return None,
            SessionStatus::Starting => StateBanner {
                tone: BannerTone::Info,
                lead: "Provisioning environment…".into(),
                body: Some(format!(
                    "A fresh sandbox is being created on {}. The agent will start \
                     automatically.",
                    backend_label.unwrap_or("the backend")
                )),
                quote: None,
                meta: None,
                actions: Vec::new(),
            },
            SessionStatus::WaitingForInput => StateBanner {
                tone: BannerTone::Await,
                lead: format!("{tool} is waiting for your answer."),
                body: None,
                quote: session.pending_prompt.clone(),
                meta: waiting.map(|w| format!("waiting {}", wait_label(w))),
                actions: Vec::new(),
            },
            SessionStatus::AwaitingConfirmation => {
                let done = detail
                    .tasks
                    .iter()
                    .filter(|t| t.status == TaskStatus::Done)
                    .count();
                let total = detail.tasks.len();
                let code = outcome.exit_code.unwrap_or(0);
                let mut meta = format!("exited cleanly (code {code}) · {done}/{total} tasks");
                if let Some(w) = waiting {
                    meta.push_str(&format!(" · waiting {}", wait_label(w)));
                }
                StateBanner {
                    tone: BannerTone::Confirm,
                    lead: format!("{tool} exited cleanly with tracked tasks unfinished."),
                    body: None,
                    quote: outcome.exit_summary.clone(),
                    meta: Some(meta),
                    actions: vec![
                        btn("Confirm completion", ButtonIntent::Primary),
                        btn("Clean up", ButtonIntent::Ghost),
                    ],
                }
            }
            SessionStatus::Stalled => {
                // "No progress for {d}" — since the last recorded event (the stall is
                // exactly the absence of anything newer).
                let last = events.last().map(|e| since(e.timestamp));
                StateBanner {
                    tone: BannerTone::Warn,
                    lead: format!(
                        "No progress for {}.",
                        last.map_or_else(|| "a while".to_string(), wait_label)
                    ),
                    body: Some(
                        "The agent may be stuck waiting on a fixture or input. Consider \
                         sending input or stopping the session."
                            .into(),
                    ),
                    quote: None,
                    meta: None,
                    actions: vec![
                        btn("Send input", ButtonIntent::Ghost),
                        btn("Stop", ButtonIntent::Danger),
                    ],
                }
            }
            SessionStatus::Failed => StateBanner {
                tone: BannerTone::Error,
                lead: "Session failed.".into(),
                body: outcome.reason.clone().or(outcome.exit_summary.clone()),
                quote: None,
                meta: None,
                actions: vec![
                    btn("Start similar", ButtonIntent::Ghost),
                    btn("Clean up", ButtonIntent::Ghost),
                ],
            },
            SessionStatus::Unknown => {
                let tail = "Showing last-known state — this is not a healthy session.";
                StateBanner {
                    tone: BannerTone::Unknown,
                    lead: "Connection lost.".into(),
                    body: Some(match &outcome.reason {
                        Some(note) => format!("{note} {tail}"),
                        None => tail.to_string(),
                    }),
                    quote: None,
                    meta: None,
                    actions: vec![btn("Retry connection", ButtonIntent::Ghost)],
                }
            }
            SessionStatus::Completed => {
                let tail = "You're viewing persisted output in review mode.";
                StateBanner {
                    tone: BannerTone::Ok,
                    lead: "Completed.".into(),
                    body: Some(
                        match outcome.exit_summary.clone().or(outcome.reason.clone()) {
                            Some(exit) => format!("{exit} {tail}"),
                            None => tail.to_string(),
                        },
                    ),
                    quote: None,
                    meta: None,
                    actions: vec![btn("Clean up env", ButtonIntent::Ghost)],
                }
            }
            SessionStatus::Stopped => StateBanner {
                tone: BannerTone::Neutral,
                lead: "Stopped by operator.".into(),
                body: Some(
                    "Output persisted — review mode. The environment is still allocated.".into(),
                ),
                quote: None,
                meta: None,
                actions: vec![btn("Clean up env", ButtonIntent::Ghost)],
            },
        })
    }

    /// Shape the telemetry rail from the detail + persisted events + metric history.
    fn rail(
        detail: &SessionDetail,
        events: &[EventRecord],
        cpu_history: &[ResourceUsageMetric],
        memory_history: &[ResourceUsageMetric],
    ) -> TelemetryRail {
        let status = detail.session.status;
        let unknown = status == SessionStatus::Unknown;
        let ended = status.has_ended(); // prototype `ended`

        let latest = |kind: MetricKind| {
            detail
                .metrics
                .iter()
                .find(|m| m.metric == kind)
                .map(|m| m.value)
        };
        let resources = (!unknown).then(|| {
            let disk = latest(MetricKind::Disk);
            let started = detail
                .session
                .started_at
                .unwrap_or(detail.session.created_at);
            let until = if ended {
                detail
                    .session
                    .ended_at
                    .unwrap_or_else(daedalus_core::clock::now)
            } else {
                daedalus_core::clock::now()
            };
            let elapsed = until.saturating_duration_since(&started);
            RailResources {
                cpu: ResourceSpark {
                    label: "CPU",
                    value_text: latest(MetricKind::Cpu)
                        .map_or_else(|| "—".to_string(), |v| format!("{v:.0}%")),
                    history: cpu_history.iter().map(|m| m.value).collect(),
                },
                memory: ResourceSpark {
                    label: "Memory",
                    value_text: latest(MetricKind::Memory)
                        .map_or_else(|| "—".to_string(), human_bytes),
                    history: memory_history.iter().map(|m| m.value).collect(),
                },
                disk: TelemetryRow {
                    label: "Disk".to_string(),
                    // Disk is bytes with no known ceiling here — a value readout, not an
                    // always-full bar (G3).
                    meter: telemetry_meter(MetricKind::Disk, disk.unwrap_or(0.0)),
                    value_text: disk.map_or_else(|| "—".to_string(), human_bytes),
                },
                runtime_label: if ended { "Ran for" } else { "Runtime" },
                runtime: wait_label(elapsed),
            }
        });

        let timeline = events
            .iter()
            .filter_map(|e| {
                let (kind, label) = match &e.payload {
                    EventPayload::Lifecycle { status, note } => (
                        TimelineKind::Status(*status),
                        lifecycle_label(*status, note.as_deref()),
                    ),
                    EventPayload::OperatorAction { action } => (
                        TimelineKind::Action(*action),
                        action_label(*action).to_string(),
                    ),
                    EventPayload::TaskStatusChange { task, status } => (
                        TimelineKind::Note,
                        format!("{} — {}", task.0, task_status_label(*status)),
                    ),
                    // Raw output chunks are the terminal's job, not the timeline's.
                    EventPayload::Output { .. } => return None,
                };
                Some(TimelineEntry {
                    kind,
                    label,
                    timestamp: e.timestamp,
                })
            })
            .collect();

        let outcome = (ended || unknown).then(|| {
            let o = detail.session.terminal_outcome.clone().unwrap_or_default();
            OutcomeBlock {
                state: status,
                state_label: StatusTone::session_label(status),
                exit: if status == SessionStatus::Stopped {
                    Some("SIGTERM (operator)".to_string())
                } else {
                    o.exit_code.map(|c| format!("code {c}"))
                },
                reason: o.reason.or(o.exit_summary),
                note: if unknown {
                    "Last-known output preserved with its timestamp."
                } else {
                    "Output persisted — readable in review mode."
                },
            }
        });

        TelemetryRail {
            open: true,
            resources,
            unavailable_notice: unknown.then_some(
                "Live metrics unavailable — connection lost. Showing last-known state only.",
            ),
            timeline,
            outcome,
        }
    }

    /// Apply a deep-link focus (T078, prototype answer mode): `AnswerPrompt` takes effect
    /// only while the agent is actually waiting for input — the pending question scrolls
    /// into view, the terminal input is focused, and the placeholder retitles to
    /// "Reply to {tool}…"; `ConfirmControls` only while awaiting confirmation.
    #[must_use]
    pub fn with_focus(mut self, focus: Option<SessionFocus>) -> Self {
        match focus {
            Some(SessionFocus::AnswerPrompt) if self.status == SessionStatus::WaitingForInput => {
                self.answer_mode = true;
                self.focus_target = Some(SessionFocus::AnswerPrompt);
                self.input_placeholder = format!("Reply to {}…  (↵ to send)", self.tool);
            }
            Some(SessionFocus::ConfirmControls)
                if self.status == SessionStatus::AwaitingConfirmation =>
            {
                self.focus_target = Some(SessionFocus::ConfirmControls);
            }
            // A stale deep link (the session moved on) lands as a plain open.
            _ => {}
        }
        self
    }

    /// The header lifecycle controls, in prototype order (`sess-controls`): Send input ·
    /// Clean up · then Stop while the run is on, Confirm completion for confirm sessions,
    /// or Start similar in review mode.
    fn controls(detail: &SessionDetail, _theme: &Theme) -> Vec<ActionButton> {
        let mut controls = Vec::new();
        let status = detail.session.status;
        let ended = status.has_ended(); // prototype `ended`
        let live = matches!(status, SessionStatus::Starting | SessionStatus::Running);

        // Send-input gated on the tool capability (FR-023) and on the run being over —
        // disabled-with-reason, never silently inert.
        if !detail.session.accepts_input {
            controls.push(ActionButton::disabled(
                "Send input",
                ButtonIntent::Tinted,
                "This tool does not accept input",
            ));
        } else if ended {
            controls.push(ActionButton::disabled(
                "Send input",
                ButtonIntent::Tinted,
                REVIEW_MODE_NOTICE,
            ));
        } else {
            controls.push(ActionButton::enabled("Send input", ButtonIntent::Tinted));
        }

        // Clean up releases the environment — disabled while the session is live
        // (prototype `cleanup-btn` disabled on isLive).
        if live {
            controls.push(ActionButton::disabled(
                "Clean up",
                ButtonIntent::Ghost,
                "the session is still live",
            ));
        } else {
            controls.push(ActionButton::enabled("Clean up", ButtonIntent::Ghost));
        }

        if !ended {
            controls.push(ActionButton::enabled("Stop", ButtonIntent::Danger));
        } else if status == SessionStatus::AwaitingConfirmation {
            // Confirm completion only when awaiting (FR-015a).
            controls.push(ActionButton::enabled(
                "Confirm completion",
                ButtonIntent::Primary,
            ));
        } else {
            controls.push(ActionButton::enabled(
                "Start similar",
                ButtonIntent::Primary,
            ));
        }

        controls
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_label_prefers_the_specs_fragment() {
        assert_eq!(
            spec_label("/work/repo/specs/044-device-flow").as_deref(),
            Some("specs/044-device-flow")
        );
        assert_eq!(
            spec_label("/tmp/objective-x/").as_deref(),
            Some("objective-x")
        );
        assert_eq!(spec_label(""), None);
    }

    #[tokio::test]
    async fn board_groups_into_prototype_columns_with_progress() {
        let fx = daedalus_tests::Fixture::new();
        let tool = fx.register_sample_tool("claude");
        let req = fx.fresh_request(tool);
        let objective = req.objective.clone();
        fx.write_tasks(
            &objective,
            "- [ ] T001 First\n- [~] T002 Second\n- [x] T003 Third\n- [!] T004 Fourth\n",
        );
        let id = fx.core.start_session(req).await.unwrap();
        fx.core.refresh_task_board(id).unwrap();

        let view = SessionView::build(&fx.app, id, &Theme::host_default()).unwrap();
        // Every prototype column present, counts match, done/total progress reads 1/4.
        let columns = view.board_columns();
        let shape: Vec<(&str, usize)> = columns.iter().map(|c| (c.title, c.cards.len())).collect();
        assert_eq!(
            shape,
            vec![
                ("To do", 1),
                ("In progress", 1),
                ("Blocked", 1),
                ("Done", 1)
            ]
        );
        assert_eq!(view.progress, (1, 4));

        // Header meta chips: spec ref, backend, and the isolation posture — a fresh
        // environment is stated, never implied (FR-002a chip is the positive one).
        assert!(view.chips.iter().any(|c| c.icon == "doc"));
        assert!(view
            .chips
            .iter()
            .any(|c| c.label == "fresh environment" && !c.positive));
    }

    #[tokio::test]
    async fn awaiting_session_shows_confirm_and_disabled_input_when_unsupported() {
        let fx = daedalus_tests::Fixture::new();
        // A tool that does NOT accept input.
        let tool = fx
            .core
            .register_tool(daedalus_proto::ToolDef {
                name: "batch".into(),
                invocation: daedalus_proto::InvocationSpec {
                    program: "batch".into(),
                    args: vec![],
                    env: vec![],
                },
                capabilities: daedalus_proto::Capabilities {
                    accepts_interactive_input: false,
                    prompt_convention: None,
                },
            })
            .unwrap();
        let req = fx.fresh_request(tool);
        let objective = req.objective.clone();
        fx.write_tasks(&objective, "- [ ] T001 Implement\n");
        let id = fx.core.start_session(req).await.unwrap();
        fx.core
            .apply_signal(id, daedalus_core::AgentSignal::ExitedCleanly)
            .await
            .unwrap();

        let view = SessionView::build(&fx.app, id, &Theme::host_default()).unwrap();
        assert!(view
            .controls
            .iter()
            .any(|c| c.label == "Confirm completion" && c.enabled));
        let send = view
            .controls
            .iter()
            .find(|c| c.label == "Send input")
            .unwrap();
        assert!(!send.enabled);
        assert!(send.disabled_reason.is_some());
    }

    #[tokio::test]
    async fn answer_prompt_focus_enters_answer_mode_while_waiting() {
        let fx = daedalus_tests::Fixture::new();
        let tool = fx.register_sample_tool("claude");
        let id = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();

        // Before the agent asks anything, an AnswerPrompt deep link is ignored.
        let view = SessionView::build(&fx.app, id, &Theme::host_default())
            .unwrap()
            .with_focus(Some(SessionFocus::AnswerPrompt));
        assert!(!view.answer_mode);
        assert!(view.focus_target.is_none());
        assert_eq!(
            view.input_placeholder,
            "Send input to the focused pane…  (↵ to send)"
        );

        // The agent asks a question: answer mode focuses the prompt + retitles the input.
        fx.core
            .mark_waiting_for_input(id, "Continue? (y/n)")
            .unwrap();
        let view = SessionView::build(&fx.app, id, &Theme::host_default())
            .unwrap()
            .with_focus(Some(SessionFocus::AnswerPrompt));
        assert!(view.answer_mode);
        assert_eq!(view.focus_target, Some(SessionFocus::AnswerPrompt));
        assert_eq!(view.pending_prompt.as_deref(), Some("Continue? (y/n)"));
        assert_eq!(view.input_placeholder, "Reply to claude…  (↵ to send)");

        // Opened without a deep link: no answer mode, default placeholder.
        let plain = SessionView::build(&fx.app, id, &Theme::host_default()).unwrap();
        assert!(!plain.answer_mode);
        assert!(plain.focus_target.is_none());
    }

    #[test]
    fn cpu_telemetry_meter_reflects_percentage_not_always_full() {
        // G3: CPU is a percentage, so distinct readings give distinct, correct fractions
        // against a 100% scale — not the old always-full `max = value.max(1.0)` bar.
        let low = telemetry_meter(MetricKind::Cpu, 3.0).expect("cpu has a meter");
        let high = telemetry_meter(MetricKind::Cpu, 97.0).expect("cpu has a meter");
        assert!((low.fraction() - 0.03).abs() < 1e-9);
        assert!((high.fraction() - 0.97).abs() < 1e-9);
        assert_ne!(low.fraction(), high.fraction());
        // Memory/disk/time carry no ceiling in the view-model → no fraction bar.
        assert!(telemetry_meter(MetricKind::Memory, 256.0 * 1024.0 * 1024.0).is_none());
        assert!(telemetry_meter(MetricKind::Disk, 64.0 * 1024.0 * 1024.0).is_none());
        assert!(telemetry_meter(MetricKind::Time, 42.0).is_none());
    }

    #[tokio::test]
    async fn telemetry_rail_shows_resource_history_timeline_and_defaults_open() {
        let fx = daedalus_tests::Fixture::new();
        let tool = fx.register_sample_tool("claude");
        let id = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
        fx.core.record_resource_usage(id).await.unwrap();
        fx.core.record_resource_usage(id).await.unwrap();

        let view = SessionView::build(&fx.app, id, &Theme::host_default()).unwrap();
        let rail = &view.rail;
        assert!(rail.open, "the rail defaults ON");
        assert_eq!(rail.toggle_label(), "Hide telemetry rail");
        assert!(rail.unavailable_notice.is_none());

        // Resources: CPU + memory sparkline history, disk as a current meter, runtime.
        let res = rail.resources.as_ref().expect("live session has metrics");
        assert_eq!(res.cpu.label, "CPU");
        assert_eq!(
            res.cpu.history.len(),
            2,
            "recent samples feed the sparkline"
        );
        assert_eq!(res.memory.label, "Memory");
        assert_eq!(res.memory.history.len(), 2);
        assert_eq!(res.disk.label, "Disk");
        // G3: disk is bytes with no known ceiling in the view-model, so it is a value
        // readout (no fraction bar) rather than an always-full meter.
        assert!(
            res.disk.meter.is_none(),
            "disk has no known ceiling — value readout, not an always-full bar"
        );
        assert!(
            res.disk.value_text.contains("MB"),
            "disk shows a byte value"
        );
        assert_eq!(
            res.runtime_label, "Runtime",
            "still running — not \"Ran for\""
        );
        assert!(!res.runtime.is_empty());

        // Timeline from the persisted events, oldest first: creation, the operator's
        // start action, then the running transition (FR-019a).
        let labels: Vec<&str> = rail.timeline.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(labels[0], "Created — provisioning environment");
        assert!(labels.contains(&"Started by operator"));
        assert!(labels.contains(&"Agent started"));
        assert!(
            rail.timeline
                .windows(2)
                .all(|w| w[0].timestamp <= w[1].timestamp),
            "oldest → newest"
        );

        // No outcome block while running.
        assert!(rail.outcome.is_none());

        // The toggle flips the visibility state.
        let mut rail = view.rail.clone();
        rail.toggle();
        assert!(!rail.open);
        assert_eq!(rail.toggle_label(), "Show telemetry rail");
    }

    #[tokio::test]
    async fn outcome_block_covers_stopped_confirm_and_unknown_sessions() {
        let fx = daedalus_tests::Fixture::new();
        let tool = fx.register_sample_tool("claude");

        // Stopped by the operator: SIGTERM exit, reason, persisted-capture note.
        let stopped = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
        fx.core.stop_session(stopped).await.unwrap();
        let view = SessionView::build(&fx.app, stopped, &Theme::host_default()).unwrap();
        let res = view.rail.resources.as_ref().expect("metrics stay shown");
        assert_eq!(res.runtime_label, "Ran for");
        let outcome = view.rail.outcome.as_ref().expect("ended ⇒ outcome block");
        assert_eq!(outcome.state, SessionStatus::Stopped);
        assert_eq!(outcome.state_label, "Stopped");
        assert_eq!(outcome.exit.as_deref(), Some("SIGTERM (operator)"));
        assert_eq!(outcome.reason.as_deref(), Some("stopped by operator"));
        assert_eq!(outcome.note, "Output persisted — readable in review mode.");

        // Clean exit awaiting confirmation: the run is over — exit code 0 shows.
        let req = fx.fresh_request(tool);
        fx.write_tasks(&req.objective, "- [ ] T001 Implement\n");
        let confirm = fx.core.start_session(req).await.unwrap();
        fx.core
            .apply_signal(confirm, daedalus_core::AgentSignal::ExitedCleanly)
            .await
            .unwrap();
        let view = SessionView::build(&fx.app, confirm, &Theme::host_default()).unwrap();
        let outcome = view.rail.outcome.as_ref().expect("confirm ⇒ outcome block");
        assert_eq!(outcome.exit.as_deref(), Some("code 0"));
        assert_eq!(
            view.rail.resources.as_ref().unwrap().runtime_label,
            "Ran for"
        );

        // Connection lost: metrics unavailable, last-known note (s-908).
        let unknown = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
        fx.core.record_resource_usage(unknown).await.unwrap();
        fx.core.mark_connection_lost(unknown).unwrap();
        let view = SessionView::build(&fx.app, unknown, &Theme::host_default()).unwrap();
        assert!(view.rail.resources.is_none(), "never shown as healthy");
        assert_eq!(
            view.rail.unavailable_notice,
            Some("Live metrics unavailable — connection lost. Showing last-known state only.")
        );
        let outcome = view.rail.outcome.as_ref().expect("unknown ⇒ outcome block");
        assert_eq!(outcome.state_label, "Connection lost");
        assert_eq!(outcome.exit, None);
        assert_eq!(
            outcome.note,
            "Last-known output preserved with its timestamp."
        );
    }

    #[tokio::test]
    async fn trim_notice_appears_only_when_capture_exceeds_the_scrollback_limit() {
        let fx = daedalus_tests::Fixture::new();
        let tool = fx.register_sample_tool("claude");
        let id = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();

        // Under the display limit: no notice (FR-016a).
        let mut detail = fx.app.session(id).unwrap();
        detail.capture.total_lines = 128;
        let view = SessionView::from_detail(detail.clone(), &Theme::host_default());
        assert_eq!(view.trim_notice, None);

        // Over the limit: the prototype's exact copy, thousands-separated (s-906).
        detail.capture.total_lines = 1_834_211;
        let view = SessionView::from_detail(detail, &Theme::host_default());
        assert_eq!(
            view.trim_notice.as_deref(),
            Some("Output trimmed — showing last 10,000 of 1,834,211 lines · full log persisted")
        );
    }

    #[tokio::test]
    async fn state_banner_covers_starting_awaiting_and_confirm() {
        // Prototype StateBanner (T085): starting explains provisioning; awaiting quotes
        // the question with a waiting readout; confirm quotes the summary with the
        // exit/task/wait meta line and the Confirm/Clean-up actions.
        let fx = daedalus_tests::Fixture::new();
        let tool = fx.register_sample_tool("claude");

        // Awaiting input: the banner quotes the pending question.
        let awaiting = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
        fx.core
            .mark_waiting_for_input(awaiting, "Continue? (y/n)")
            .unwrap();
        let view = SessionView::build(&fx.app, awaiting, &Theme::host_default()).unwrap();
        let banner = view.banner.as_ref().expect("awaiting shows a banner");
        assert_eq!(banner.tone, BannerTone::Await);
        assert_eq!(banner.lead, "claude is waiting for your answer.");
        assert_eq!(banner.quote.as_deref(), Some("Continue? (y/n)"));
        assert!(banner.meta.as_deref().unwrap().starts_with("waiting "));

        // Confirm: clean exit with tracked tasks unfinished (FR-015a).
        let req = fx.fresh_request(tool);
        fx.write_tasks(
            &req.objective,
            "- [x] T001 Done thing\n- [ ] T002 Open thing\n",
        );
        let confirm = fx.core.start_session(req).await.unwrap();
        fx.core.refresh_task_board(confirm).unwrap();
        fx.core
            .apply_signal(confirm, daedalus_core::AgentSignal::ExitedCleanly)
            .await
            .unwrap();
        let view = SessionView::build(&fx.app, confirm, &Theme::host_default()).unwrap();
        let banner = view.banner.as_ref().expect("confirm shows a banner");
        assert_eq!(banner.tone, BannerTone::Confirm);
        assert_eq!(
            banner.lead,
            "claude exited cleanly with tracked tasks unfinished."
        );
        let meta = banner.meta.as_deref().unwrap();
        assert!(
            meta.starts_with("exited cleanly (code 0) · 1/2 tasks"),
            "meta was: {meta}"
        );
        let labels: Vec<&str> = banner.actions.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels, ["Confirm completion", "Clean up"]);

        // A plainly running session shows no banner.
        let running = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
        let view = SessionView::build(&fx.app, running, &Theme::host_default()).unwrap();
        assert!(view.banner.is_none());
    }

    #[tokio::test]
    async fn state_banner_covers_stopped_unknown_and_failed_review_states() {
        let fx = daedalus_tests::Fixture::new();
        let tool = fx.register_sample_tool("claude");

        // Stopped: review-mode copy verbatim; input row shows the review notice.
        let stopped = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
        fx.core.stop_session(stopped).await.unwrap();
        let view = SessionView::build(&fx.app, stopped, &Theme::host_default()).unwrap();
        let banner = view.banner.as_ref().unwrap();
        assert_eq!(banner.tone, BannerTone::Neutral);
        assert_eq!(banner.lead, "Stopped by operator.");
        assert_eq!(
            banner.body.as_deref(),
            Some("Output persisted — review mode. The environment is still allocated.")
        );
        assert_eq!(banner.actions[0].label, "Clean up env");
        assert_eq!(
            view.input_notice.as_deref(),
            Some("Session ended — output is read-only (review mode).")
        );

        // Connection lost: explicitly not healthy (FR-020).
        let unknown = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
        fx.core.mark_connection_lost(unknown).unwrap();
        let view = SessionView::build(&fx.app, unknown, &Theme::host_default()).unwrap();
        let banner = view.banner.as_ref().unwrap();
        assert_eq!(banner.tone, BannerTone::Unknown);
        assert_eq!(banner.lead, "Connection lost.");
        assert!(banner
            .body
            .as_deref()
            .unwrap()
            .ends_with("Showing last-known state — this is not a healthy session."));
        assert_eq!(banner.actions[0].label, "Retry connection");

        // Failed: the exit reason rides in the banner body.
        let failed = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
        fx.core
            .apply_signal(failed, daedalus_core::AgentSignal::Crashed)
            .await
            .unwrap();
        let view = SessionView::build(&fx.app, failed, &Theme::host_default()).unwrap();
        let banner = view.banner.as_ref().unwrap();
        assert_eq!(banner.tone, BannerTone::Error);
        assert_eq!(banner.lead, "Session failed.");
        assert!(banner.body.is_some(), "the failure reason is never omitted");
        let labels: Vec<&str> = banner.actions.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels, ["Start similar", "Clean up"]);
    }

    #[tokio::test]
    async fn controls_follow_the_prototype_header_rules() {
        // Prototype sess-controls (T085): Clean up is disabled while live; Stop only
        // while the run is on; review mode swaps Stop for Start similar; a live
        // interactive session has no input notice.
        let fx = daedalus_tests::Fixture::new();
        let tool = fx.register_sample_tool("claude");

        let live = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
        let view = SessionView::build(&fx.app, live, &Theme::host_default()).unwrap();
        assert!(view.input_notice.is_none());
        let clean = view
            .controls
            .iter()
            .find(|c| c.label == "Clean up")
            .unwrap();
        assert!(!clean.enabled, "clean-up is disabled while live");
        assert!(view.controls.iter().any(|c| c.label == "Stop" && c.enabled));
        assert!(!view.controls.iter().any(|c| c.label == "Start similar"));

        let stopped = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
        fx.core.stop_session(stopped).await.unwrap();
        let view = SessionView::build(&fx.app, stopped, &Theme::host_default()).unwrap();
        let clean = view
            .controls
            .iter()
            .find(|c| c.label == "Clean up")
            .unwrap();
        assert!(clean.enabled, "clean-up is offered once the run is over");
        assert!(!view.controls.iter().any(|c| c.label == "Stop"));
        assert!(view
            .controls
            .iter()
            .any(|c| c.label == "Start similar" && c.enabled));
        let send = view
            .controls
            .iter()
            .find(|c| c.label == "Send input")
            .unwrap();
        assert!(!send.enabled, "no input into a persisted transcript");
    }

    #[tokio::test]
    async fn confirm_focus_targets_the_confirm_controls() {
        let fx = daedalus_tests::Fixture::new();
        let tool = fx.register_sample_tool("claude");
        let req = fx.fresh_request(tool);
        let objective = req.objective.clone();
        fx.write_tasks(&objective, "- [ ] T001 Implement\n");
        let id = fx.core.start_session(req).await.unwrap();
        fx.core
            .apply_signal(id, daedalus_core::AgentSignal::ExitedCleanly)
            .await
            .unwrap();

        let view = SessionView::build(&fx.app, id, &Theme::host_default())
            .unwrap()
            .with_focus(Some(SessionFocus::ConfirmControls));
        assert_eq!(view.focus_target, Some(SessionFocus::ConfirmControls));
        assert!(!view.answer_mode, "confirm focus is not the answer mode");
    }
}
