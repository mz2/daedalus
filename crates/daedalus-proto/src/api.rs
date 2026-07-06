//! Surface-facing DTOs and the push event enum (contract `contracts/app-api.md`).
//!
//! These live in `daedalus-proto` so the core, the app service, and every surface share
//! them verbatim. The `Command` enum and the `AppQuery` trait live in `daedalus-app`; the
//! data they carry is defined here.

use serde::{Deserialize, Serialize};

use crate::entities::{
    AgenticTool, Capabilities, InvocationSpec, Objective, ResourceLimits, ResourceUsageMetric,
    SandboxEnvironment, Session, TrackedTask, WorktreeRef,
};
use crate::ids::{BackendId, SessionId, TaskId, ToolId};
use crate::status::{Availability, BackendKind, Origin, SessionStatus, TaskStatus};

/// A redacted byte chunk safe to stream/persist (FR-031, R9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactedBytes(pub Vec<u8>);

/// Operator input to register a tool (FR-001a).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolDef {
    /// Unique tool name.
    pub name: String,
    /// How to launch it inside a sandbox.
    pub invocation: InvocationSpec,
    /// Declared capabilities.
    pub capabilities: Capabilities,
}

/// Operator input to start a session (FR-001/002/006).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StartSessionRequest {
    /// Which registered tool to run.
    pub tool_id: ToolId,
    /// The SDD objective the session works (inline; the core records it).
    pub objective: Objective,
    /// Fresh or pre-existing environment.
    pub origin: Origin,
    /// Required when `origin == PreExisting` (FR-002a).
    pub worktree: Option<WorktreeRef>,
    /// Which backend kind to provision on.
    pub backend: BackendKind,
    /// Resource limits to apply to the environment (spec edge case "Resource
    /// exhaustion"); defaults leave every ceiling at the backend default.
    #[serde(default)]
    pub limits: ResourceLimits,
}

/// A compact session row for the fleet view (FR-025).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSummary {
    /// Session id.
    pub id: SessionId,
    /// Tool name.
    pub tool_name: String,
    /// Objective summary.
    pub objective: String,
    /// Backend kind hosting it.
    pub backend: BackendKind,
    /// Fresh vs pre-existing.
    pub origin: Origin,
    /// Current status.
    pub status: SessionStatus,
    /// Whether it accepts interactive input.
    pub accepts_input: bool,
    /// The SDD spec reference the row's branch sub-line shows (the tracked `tasks.md`).
    #[serde(default)]
    pub spec: String,
    /// Tracked tasks done (the fleet progress readout).
    #[serde(default)]
    pub tasks_done: usize,
    /// Tracked tasks total.
    #[serde(default)]
    pub tasks_total: usize,
}

/// Size of a session's persisted output capture (FR-016a/018): lets the terminal show
/// "showing last N of M lines · full log persisted" when its bounded scrollback trims.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureStats {
    /// Total lines in the persisted capture file (a trailing unterminated line counts).
    pub total_lines: u64,
}

/// Full detail for the session screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionDetail {
    /// The session record.
    pub session: Session,
    /// The tool it runs.
    pub tool: AgenticTool,
    /// The objective it works.
    pub objective: Objective,
    /// Its environment.
    pub environment: SandboxEnvironment,
    /// The current task board.
    pub tasks: Vec<TrackedTask>,
    /// Latest resource metrics.
    pub metrics: Vec<ResourceUsageMetric>,
    /// Persisted-capture size, for the trimmed-output notice (FR-016a).
    #[serde(default)]
    pub capture: CaptureStats,
}

/// One tracked task with its session context, for the aggregate tasks board (FR-025a):
/// the task is the unit; the session is demoted to a field the operator filters/jumps by
/// (design `tasks.jsx` / `data.js allTasks()`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AggregateTask {
    /// The tracked task (id, description, status, owning session).
    pub task: TrackedTask,
    /// Owning session (repeated from `task` for convenient field access).
    pub session: SessionId,
    /// The session's objective line (the session-ref chip text).
    pub objective: String,
    /// The session's current status (drives the ref chip dot + attention).
    pub session_status: SessionStatus,
    /// Spec reference for the row sub-line (the objective's artifact root).
    pub spec: String,
    /// Tool name.
    pub tool: String,
    /// Backend kind hosting the session.
    pub backend: BackendKind,
    /// Set when the session is blocked on the operator (waiting for input, awaiting
    /// confirmation, stalled, disconnected, or failed) — the board's attention flag.
    pub attention: Option<AttentionKind>,
}

/// Aggregate-board filter (FR-025a): every `None` field matches everything.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TasksFilter {
    /// Only tasks with this status.
    pub status: Option<TaskStatus>,
    /// Only tasks of this session.
    pub session: Option<SessionId>,
    /// Only tasks run by this tool (by name).
    pub tool: Option<String>,
    /// Only tasks hosted on this backend kind.
    pub backend: Option<BackendKind>,
}

impl TasksFilter {
    /// Whether a row passes the filter.
    #[must_use]
    pub fn matches(&self, task: &AggregateTask) -> bool {
        self.status.is_none_or(|s| task.task.status == s)
            && self.session.is_none_or(|s| task.session == s)
            && self.tool.as_deref().is_none_or(|t| task.tool == t)
            && self.backend.is_none_or(|b| task.backend == b)
    }
}

/// Backend availability row (FR-028).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendStatus {
    /// Backend id.
    pub id: BackendId,
    /// Backend kind.
    pub kind: BackendKind,
    /// Current availability.
    pub availability: Availability,
    /// Stated reason when degraded/unavailable (FR-028), e.g. "high memory pressure".
    #[serde(default)]
    pub availability_reason: Option<String>,
}

/// Why a session is in the "Needs you" queue (FR-021a; data-model `AttentionItem.kind`).
/// Variants are declared in queue order: most answerable first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttentionKind {
    /// The agent asked a question and is blocked on input (FR-015b).
    WaitingForInput,
    /// Clean exit with tracked tasks unfinished — confirm completion (FR-015a).
    AwaitingConfirmation,
    /// No output/progress — may be stuck (FR-020).
    Stalled,
    /// Contact with the environment lost (`SessionStatus::Unknown`, FR-020).
    Disconnected,
    /// The run failed — needs a call (FR-005).
    Failed,
}

impl AttentionKind {
    /// The attention kind for a session status, or `None` for statuses that do not need
    /// the operator.
    #[must_use]
    pub fn from_status(status: SessionStatus) -> Option<Self> {
        match status {
            SessionStatus::WaitingForInput => Some(Self::WaitingForInput),
            SessionStatus::AwaitingConfirmation => Some(Self::AwaitingConfirmation),
            SessionStatus::Stalled => Some(Self::Stalled),
            SessionStatus::Unknown => Some(Self::Disconnected),
            SessionStatus::Failed => Some(Self::Failed),
            SessionStatus::Starting
            | SessionStatus::Running
            | SessionStatus::Completed
            | SessionStatus::Stopped => None,
        }
    }

    /// Queue rank: most answerable first (data-model AttentionItem ordering).
    #[must_use]
    pub fn rank(self) -> u8 {
        self as u8
    }
}

/// The cost of leaving a blocked session waiting (FR-021b).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostIndication {
    /// A live environment with a configured backend idle rate: the accrued estimate
    /// (idle rate × waiting time, in the operator's configured currency-per-hour unit).
    Idle(f64),
    /// The agent has exited but the environment is still held (awaiting confirmation).
    EnvHeld,
    /// No idle rate configured — only the waiting time is shown.
    None,
}

/// One "Needs you" entry, derived from live session state — never persisted (FR-021a/b).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttentionItem {
    /// The blocked session; activation deep-links to it.
    pub session_id: SessionId,
    /// Why it needs the operator.
    pub kind: AttentionKind,
    /// The reason line: pending question (input), exit summary (confirmation), or the
    /// stall/fail/drop reason.
    pub cue: String,
    /// How long it has been waiting (from `waiting_since`, or time in state).
    pub waiting: std::time::Duration,
    /// Waiting-cost indication.
    pub cost: CostIndication,
}

/// A notification for a terminal or abnormal session event (FR-021).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalOrAbnormal {
    /// Which session.
    pub session: SessionId,
    /// The new status (terminal or attention).
    pub status: SessionStatus,
    /// Optional reason/detail.
    pub note: Option<String>,
}

/// Push events that drive live UI updates without polling (contract `app-api.md`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppEvent {
    /// A session's status changed (FR-015/020/021).
    SessionStatusChanged {
        /// Which session.
        id: SessionId,
        /// New status.
        status: SessionStatus,
    },
    /// Secret-redacted terminal output (FR-016, R9).
    Output {
        /// Which session.
        id: SessionId,
        /// The redacted chunk.
        chunk: RedactedBytes,
    },
    /// A tracked task changed status (FR-017, ≤5s SC-010).
    TaskStatusChanged {
        /// Which session.
        id: SessionId,
        /// Which task.
        task: TaskId,
        /// New task status.
        status: TaskStatus,
    },
    /// Sources/sessions appeared or left.
    DiscoveryChanged,
    /// A backend's availability changed.
    BackendAvailabilityChanged {
        /// Which backend.
        id: BackendId,
        /// New availability.
        availability: Availability,
    },
    /// A terminal/abnormal notification (FR-021).
    Notification(TerminalOrAbnormal),
}
