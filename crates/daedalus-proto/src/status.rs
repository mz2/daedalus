//! Status enums shared across the core and surfaces. These map 1:1 to the locked design
//! system's status palette (`design/README.md`) and drive the state machine in
//! `daedalus-core`.

use serde::{Deserialize, Serialize};

/// Lifecycle status of a [`crate::entities::Session`].
///
/// State machine (see `data-model.md`):
/// `Starting → Running ⇄ WaitingForInput; Running → {Completed | Failed | Stalled | Stopped |
/// AwaitingConfirmation}`; plus `Unknown` (contact lost — derived from liveness, FR-020).
///
/// Terminal states: [`Completed`](SessionStatus::Completed),
/// [`Failed`](SessionStatus::Failed), [`Stopped`](SessionStatus::Stopped).
/// [`Stalled`](SessionStatus::Stalled), [`WaitingForInput`](SessionStatus::WaitingForInput),
/// [`AwaitingConfirmation`](SessionStatus::AwaitingConfirmation), and
/// [`Unknown`](SessionStatus::Unknown) are non-terminal attention states the operator
/// resolves — together with `Failed` they feed the Needs-you queue (FR-021a).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Provisioning / launching.
    Starting,
    /// Agent is live.
    Running,
    /// The agent is blocked on a question/approval from the operator (FR-015b). Entered
    /// only for tools that accept interactive input, per their declared prompt convention.
    WaitingForInput,
    /// All tracked tasks done, or operator confirmed (FR-015a, SC-004).
    Completed,
    /// Crash / non-zero exit / provisioning failure (FR-005).
    Failed,
    /// No output/progress for the configured stall interval.
    Stalled,
    /// Stopped by the operator (FR-022).
    Stopped,
    /// Clean agent exit with tracked tasks unfinished — needs operator confirmation (FR-015a).
    AwaitingConfirmation,
    /// Contact with the environment lost — last-known state preserved, never shown healthy
    /// (FR-020). Derived from liveness, not operator-settable.
    Unknown,
}

impl SessionStatus {
    /// True for states that never transition again: `Completed`, `Failed`, `Stopped`.
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            SessionStatus::Completed | SessionStatus::Failed | SessionStatus::Stopped
        )
    }

    /// True once the agent run is over: every terminal state plus
    /// [`AwaitingConfirmation`](SessionStatus::AwaitingConfirmation) — the agent exited;
    /// at most the operator's call is pending (FR-015a).
    #[must_use]
    pub fn has_ended(self) -> bool {
        self.is_terminal() || self == SessionStatus::AwaitingConfirmation
    }

    /// True for states that ask the operator for attention but can still progress.
    #[must_use]
    pub fn is_attention(self) -> bool {
        matches!(
            self,
            SessionStatus::Stalled
                | SessionStatus::AwaitingConfirmation
                | SessionStatus::WaitingForInput
                | SessionStatus::Unknown
        )
    }

    /// Stable lowercase token matching the design-system status palette key
    /// (`data-model.md` terminology mapping; persistence uses the serde token instead).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            SessionStatus::Starting => "starting",
            SessionStatus::Running => "running",
            SessionStatus::WaitingForInput => "awaiting",
            SessionStatus::Completed => "completed",
            SessionStatus::Failed => "failed",
            SessionStatus::Stalled => "stalled",
            SessionStatus::Stopped => "stopped",
            SessionStatus::AwaitingConfirmation => "confirm",
            SessionStatus::Unknown => "unknown",
        }
    }
}

/// Status of a single [`crate::entities::TrackedTask`], read from SDD artifacts (FR-017).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// Not started (`- [ ]`).
    Todo,
    /// In progress (marker convention `- [~]` / `- [-]`).
    InProgress,
    /// Done (`- [x]` / `- [X]`).
    Done,
    /// Explicitly blocked (`- [!]`).
    Blocked,
}

impl TaskStatus {
    /// Stable lowercase token for the status palette / persistence.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            TaskStatus::Todo => "todo",
            TaskStatus::InProgress => "in_progress",
            TaskStatus::Done => "done",
            TaskStatus::Blocked => "blocked",
        }
    }
}

/// Availability of a backend or a discovery source. Unavailability is a *value*, never an
/// error (FR-028, contract C-B4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    /// Reachable and healthy.
    Available,
    /// Reachable but impaired (e.g. partial capability).
    Degraded,
    /// Not reachable (host stopped advertising / tunnel dropped / backend down).
    Unavailable,
}

/// Whether an environment is freshly provisioned or attached to a pre-existing one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// A brand-new isolated environment.
    Fresh,
    /// An operator-selected pre-existing environment; requires an isolated git worktree
    /// (FR-002a).
    PreExisting,
}

/// The kind of source a session was discovered through (FR-010).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// Local zellij sessions on this host.
    Local,
    /// Advertised on the LAN over mDNS (`_daedalus._tcp`).
    Mdns,
    /// Reached via an operator-established tunnel into a Workshop host.
    TunneledWorkshop,
}

/// Which backend provides an environment (FR-027).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    /// Canonical Workshop (Linux).
    Workshop,
    /// NVIDIA OpenShell — kernel-sandboxed agent runtime; Linux hosts (GPU-capable via the
    /// NVIDIA Container Toolkit) and macOS on Apple silicon (enforcement inside the Docker
    /// Desktop Linux VM, no GPU). Issue #9.
    #[serde(rename = "openshell")]
    OpenShell,
    /// In-memory fake backend for local testing (Principle III).
    Fake,
}

impl BackendKind {
    /// Stable lowercase token (matches `DAEDALUS_BACKEND` env values).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            BackendKind::Workshop => "workshop",
            BackendKind::OpenShell => "openshell",
            BackendKind::Fake => "fake",
        }
    }
}

/// Lifecycle of a sandbox environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvLifecycle {
    /// Being provisioned.
    Provisioning,
    /// Ready to host an agent.
    Ready,
    /// Being torn down.
    Releasing,
    /// Fully released.
    Released,
    /// No longer reachable.
    Unavailable,
}

/// Kind of an [`crate::entities::EventRecord`] (FR-018).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// Captured terminal output (stored as a capture-file reference).
    Output,
    /// A tracked task changed status.
    TaskStatusChange,
    /// A lifecycle/state transition.
    Lifecycle,
    /// A key operator action — feeds the session timeline (FR-019a).
    OperatorAction,
}

/// A key operator action recorded on the session timeline (FR-019a):
/// start / stop / input-sent / confirm / clean-up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatorAction {
    /// The operator started the session.
    Start,
    /// The operator stopped the session (FR-022).
    Stop,
    /// The operator sent input to the agent (FR-023).
    InputSent,
    /// The operator confirmed completion (FR-015a).
    ConfirmCompletion,
    /// The operator cleaned up the environment (FR-024).
    CleanUp,
}

/// Kind of a [`crate::entities::ResourceUsageMetric`] (FR-019).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricKind {
    /// CPU utilisation (percent).
    Cpu,
    /// Memory used (bytes).
    Memory,
    /// Disk used (bytes).
    Disk,
    /// Wall-clock time consumed (seconds).
    Time,
}
