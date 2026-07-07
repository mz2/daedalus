//! Entity structs per `data-model.md`. These are the serde contract types persisted by
//! `daedalus-core` and rendered by the surfaces.

use serde::{Deserialize, Serialize};

use crate::ids::{
    BackendId, DiscoveredSessionId, EnvironmentId, EventId, ObjectiveId, SessionId, SourceId,
    TaskId, ToolId,
};
use crate::status::{
    Availability, BackendKind, EnvLifecycle, EventKind, MetricKind, OperatorAction, Origin,
    SessionStatus, SourceKind, TaskStatus,
};

/// Unix epoch milliseconds. Kept as a plain integer to stay dependency-light and
/// serde/SQLite-friendly; the surfaces format it for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Timestamp(pub i64);

impl Timestamp {
    /// Construct from epoch milliseconds.
    #[must_use]
    pub fn from_millis(ms: i64) -> Self {
        Self(ms)
    }

    /// Epoch milliseconds.
    #[must_use]
    pub fn millis(self) -> i64 {
        self.0
    }

    /// The duration from `earlier` to `self`, clamped to zero when `earlier` is later
    /// (clock skew never yields a negative wait).
    #[must_use]
    pub fn saturating_duration_since(&self, earlier: &Timestamp) -> std::time::Duration {
        std::time::Duration::from_millis((self.0 - earlier.0).max(0) as u64)
    }
}

/// Location of a session's SDD artifacts (spec/plan/tasks) in the workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRef {
    /// Filesystem (or workspace-relative) path to the directory holding the SDD artifacts.
    pub root: String,
    /// Path to the `tasks.md` (or equivalent) artifact whose checkbox state is read (FR-017).
    pub tasks_file: String,
}

/// An isolated git worktree backing a `PreExisting` environment (FR-002a).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeRef {
    /// Path of the worktree checkout.
    pub path: String,
    /// Branch the worktree is checked out on.
    pub branch: String,
}

/// How to launch an agentic tool inside a sandbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvocationSpec {
    /// Program/command to run.
    pub program: String,
    /// Arguments passed verbatim.
    pub args: Vec<String>,
    /// Extra environment variables (never secrets — FR-031; operator pre-provisions those).
    #[serde(default)]
    pub env: Vec<(String, String)>,
}

/// How a tool's pending input prompts are recognized, driving `WaitingForInput` detection
/// (FR-001a, FR-015b). Only tools that accept interactive input may declare one.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptConvention {
    /// The in-Workshop SDK signals the waiting state (with the pending question).
    SdkSignal,
    /// A regex matched against the streamed terminal output; the match (or its first
    /// capture group) is the pending question.
    PromptPattern(String),
}

/// Declared capabilities of an agentic tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Capabilities {
    /// Whether the tool accepts interactive input while running (FR-023).
    pub accepts_interactive_input: bool,
    /// How pending input prompts are recognized (FR-015b); only meaningful — and only
    /// valid — when `accepts_interactive_input` is true. `None` ⇒ the tool never enters
    /// `WaitingForInput`.
    #[serde(default)]
    pub prompt_convention: Option<PromptConvention>,
}

/// An operator-registered, declaratively-defined agentic tool (FR-001a).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgenticTool {
    /// Identifier.
    pub id: ToolId,
    /// Unique display/identifier name.
    pub name: String,
    /// How to launch inside a sandbox.
    pub invocation: InvocationSpec,
    /// Declared capabilities.
    pub capabilities: Capabilities,
}

/// The overall goal for a session, expressed via an SDD convention (FR-001).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Objective {
    /// Identifier.
    pub id: ObjectiveId,
    /// Location of the SDD artifacts.
    pub artifact_ref: ArtifactRef,
    /// Short human summary for display.
    pub description: String,
}

/// One unit of work on the task board; status is *read from* SDD artifacts (FR-017).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackedTask {
    /// Identifier from the SDD artifact (e.g. `T001`).
    pub id: TaskId,
    /// Owning session.
    pub session_id: SessionId,
    /// Human description.
    pub description: String,
    /// Current status from the artifact checkbox state.
    pub status: TaskStatus,
    /// When the board last observed this value (SC-010: ≤5s).
    pub updated_at: Timestamp,
}

/// Resource limits applied to an environment.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct ResourceLimits {
    /// CPU cores (fractional allowed); `None` = backend default.
    pub cpu_cores: Option<f64>,
    /// Memory ceiling in bytes; `None` = backend default.
    pub memory_bytes: Option<u64>,
    /// Disk ceiling in bytes; `None` = backend default.
    pub disk_bytes: Option<u64>,
    /// Wall-clock ceiling in seconds; `None` = unbounded.
    pub time_secs: Option<u64>,
}

/// The isolated environment hosting one session (FR-004), fresh or pre-existing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxEnvironment {
    /// Identifier.
    pub id: EnvironmentId,
    /// Owning backend.
    pub backend_id: BackendId,
    /// Fresh vs pre-existing.
    pub origin: Origin,
    /// Required when `origin = PreExisting` (FR-002a).
    pub worktree_ref: Option<WorktreeRef>,
    /// Provisioning lifecycle.
    pub lifecycle: EnvLifecycle,
}

/// A source of sandbox environments behind the common abstraction (FR-027).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentBackend {
    /// Identifier.
    pub id: BackendId,
    /// Which backend kind.
    pub kind: BackendKind,
    /// Current availability (FR-028).
    pub availability: Availability,
    /// Stated reason when degraded/unavailable (FR-028), e.g. "high memory pressure".
    #[serde(default)]
    pub availability_reason: Option<String>,
    /// Optional operator-configured idle rate (per hour) for waiting-cost estimates
    /// (FR-021b); `None` ⇒ no cost shown.
    #[serde(default)]
    pub idle_rate: Option<f64>,
}

/// Where a session is discovered (FR-010).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    /// Identifier.
    pub id: SourceId,
    /// Local / mDNS / tunneled.
    pub kind: SourceKind,
    /// Unreachable when the host stops advertising / tunnel drops (FR-014).
    pub availability: Availability,
    /// Stated reason when degraded/unavailable, surfaced in the shell hosts indicator
    /// (FR-028).
    #[serde(default)]
    pub availability_reason: Option<String>,
}

/// An optional external work item (e.g. a GitHub issue or Jira key) shown as a display
/// chip alongside a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkItemRef {
    /// Which tracker the reference lives in (e.g. "github", "jira").
    pub tracker: String,
    /// The reference within that tracker (e.g. "daedalus#42", "DAE-7").
    pub reference: String,
}

/// How a session's run ended (or paused for confirmation): reason text for
/// failed/stalled/stopped, exit code + agent exit summary for `AwaitingConfirmation`
/// (FR-015a).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Outcome {
    /// Human-readable reason (failed/stalled/stopped).
    pub reason: Option<String>,
    /// The agent process exit code, when it exited.
    pub exit_code: Option<i32>,
    /// The agent's exit summary, surfaced while awaiting confirmation (FR-015a).
    pub exit_summary: Option<String>,
}

impl Outcome {
    /// An outcome that is only a reason text (the failed/stalled/stopped shape).
    #[must_use]
    pub fn reason(text: impl Into<String>) -> Self {
        Self {
            reason: Some(text.into()),
            exit_code: None,
            exit_summary: None,
        }
    }
}

/// A single invocation of an agentic tool against an objective within an environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// Unique, stable identity (FR-003, FR-013).
    pub id: SessionId,
    /// The tool being run.
    pub tool_id: ToolId,
    /// The objective being worked.
    pub objective_id: ObjectiveId,
    /// The environment hosting it.
    pub environment_id: EnvironmentId,
    /// The source/host it is reached through.
    pub source_id: SourceId,
    /// Lifecycle status (persisted — FR-015).
    pub status: SessionStatus,
    /// When the record was created.
    pub created_at: Timestamp,
    /// When the agent started running.
    pub started_at: Option<Timestamp>,
    /// When the session reached a terminal state.
    pub ended_at: Option<Timestamp>,
    /// How the run ended: reason for failed/stalled/stopped, exit code + summary for
    /// awaiting-confirmation (FR-015a).
    pub terminal_outcome: Option<Outcome>,
    /// Mirrors the tool's input capability (FR-023).
    pub accepts_input: bool,
    /// The agent's pending question while `WaitingForInput` (FR-015b); cleared on answer.
    #[serde(default)]
    pub pending_prompt: Option<String>,
    /// Set on entering `WaitingForInput`/`AwaitingConfirmation`; drives waiting duration +
    /// idle cost (FR-021a/b).
    #[serde(default)]
    pub waiting_since: Option<Timestamp>,
    /// Optional external work item for display chips.
    #[serde(default)]
    pub work_item_ref: Option<WorkItemRef>,
    /// While `Unknown`, the state the session was last known to be in (FR-020).
    #[serde(default)]
    pub last_known_status: Option<SessionStatus>,
}

/// Captured output, task-status changes, and lifecycle events for a session (FR-018).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventRecord {
    /// Monotonic per session.
    pub id: EventId,
    /// Owning session.
    pub session_id: SessionId,
    /// When it happened.
    pub timestamp: Timestamp,
    /// Output / task-status-change / lifecycle.
    pub kind: EventKind,
    /// Payload; output is stored as a capture-file reference, not an inline blob.
    pub payload: EventPayload,
}

/// Payload of an [`EventRecord`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventPayload {
    /// A reference (byte offset / length) into the per-session capture file (FR-018).
    Output {
        /// Path to the capture file.
        capture_file: String,
        /// Byte offset of this chunk.
        offset: u64,
        /// Byte length of this chunk (post-redaction).
        len: u64,
    },
    /// A task changed status.
    TaskStatusChange {
        /// Which task.
        task: TaskId,
        /// Its new status.
        status: TaskStatus,
    },
    /// A lifecycle transition with a human-readable note.
    Lifecycle {
        /// The new session status.
        status: SessionStatus,
        /// Optional reason text.
        note: Option<String>,
    },
    /// A key operator action (start/stop/input-sent/confirm/clean-up — FR-019a).
    OperatorAction {
        /// Which action.
        action: OperatorAction,
    },
}

/// A session as seen through a discovery source, de-duplicated across sources (FR-010,
/// FR-013). A known-unreachable source is never presented as healthy (FR-014, C-A4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredSession {
    /// Source + stable identity.
    pub id: DiscoveredSessionId,
    /// Which kind of source advertised it.
    pub kind: SourceKind,
    /// Availability of the advertising source (unreachable ⇒ not healthy — FR-014).
    pub source_availability: Availability,
    /// The named zellij session to attach to.
    pub zellij_session: String,
    /// Human label for the host/environment.
    pub host_label: String,
    /// Last-known session status, if the source reports one.
    pub status: Option<SessionStatus>,
    /// Whether the session can currently be attached via zellij (C-D3).
    pub attachable: bool,
    /// When not attachable, the reason shown to the operator (never silently dropped).
    pub attach_reason: Option<String>,
    /// Workspace location of the SDD artifacts, when advertised.
    pub artifacts: Option<ArtifactRef>,
}

/// Observed consumption for a session's environment (FR-019).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ResourceUsageMetric {
    /// Owning session.
    pub session_id: SessionId,
    /// Which metric.
    pub metric: MetricKind,
    /// Observed value (units per `MetricKind`).
    pub value: f64,
    /// When observed.
    pub timestamp: Timestamp,
}
