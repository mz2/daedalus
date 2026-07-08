//! The surface ↔ core API: the [`Command`] enum, the [`AppQuery`] read trait, and a
//! re-export of the push [`AppEvent`] stream (contract `contracts/app-api.md`).
//!
//! The surface holds **no orchestration logic** — it issues commands, runs queries, and
//! subscribes to events. Because the GPUI surface is in-process Rust, this is a direct API
//! with no FFI/serialization boundary; `daedalus-proto` types are shared verbatim.

use bytes::Bytes;

use daedalus_proto::{
    AggregateTask, AttentionItem, BackendStatus, DiscoveredSession, DiscoveredSessionId,
    EventRecord, MetricKind, ResourceUsageMetric, SessionDetail, SessionId, SessionSummary,
    StartSessionRequest, ToolDef, ToolId, TrackedTask,
};

pub use daedalus_proto::AppEvent;

/// Operator intent. Each variant maps to one core action and returns a stated reason on
/// failure.
#[derive(Debug, Clone)]
pub enum Command {
    /// Register a declaratively-defined agentic tool (FR-001a).
    RegisterTool(ToolDef),
    /// Start a session (FR-001/002/006).
    StartSession(StartSessionRequest),
    /// Stop a running session (FR-022).
    StopSession(SessionId),
    /// Send input to a session that accepts it (FR-023).
    SendInput {
        /// Target session.
        session: SessionId,
        /// Bytes to deliver.
        data: Bytes,
    },
    /// Confirm completion of a session awaiting confirmation (FR-015a).
    ConfirmCompletion(SessionId),
    /// Clean up a session's environment (FR-024).
    CleanUp(SessionId),
    /// Delete a session record (FR-030a).
    DeleteRecord(SessionId),
    /// Connect to a discovered session by attaching via zellij (FR-011).
    ConnectDiscovered(DiscoveredSessionId),
    /// Set (or clear with `None`) a backend's idle rate (per hour) used for waiting-cost
    /// estimates (FR-021b); persisted so it survives restart.
    SetIdleRate {
        /// Which backend kind the rate applies to.
        backend: daedalus_proto::BackendKind,
        /// Currency-per-hour rate; `None` clears it (no cost estimate shown).
        rate: Option<f64>,
    },
    /// Set (or clear with `None` = unlimited) the maximum concurrent sessions (FR-026);
    /// persisted so it survives restart.
    SetConcurrencyLimit(Option<usize>),
    /// Set (or clear with `None`/empty) a backend's sandbox image (e.g. OpenShell's
    /// `sandbox create --from`); persisted so operator configuration lives in the app,
    /// not shell environment variables.
    SetBackendImage {
        /// Which backend kind the image applies to.
        backend: daedalus_proto::BackendKind,
        /// Image reference / community name / Dockerfile path; `None` restores the
        /// backend's default image.
        image: Option<String>,
    },
    /// Set the stall interval in seconds (FR-020); persisted so it survives restart.
    SetStallInterval(u64),
}

/// The result of a successful [`Command`].
#[derive(Debug, Clone)]
pub enum CommandResult {
    /// A tool was registered.
    ToolRegistered(ToolId),
    /// A session was started.
    SessionStarted(SessionId),
    /// A discovered session was connected (carries the resolved attach info).
    Connected(Box<DiscoveredSession>),
    /// The command completed with no return value.
    Done,
}

/// Synchronous read queries over current state (contract `app-api.md`). These never error
/// to the surface — they return the current state (empty/`None` if unknown).
pub trait AppQuery {
    /// The unified fleet view (FR-025).
    fn fleet(&self) -> Vec<SessionSummary>;
    /// Full detail for one session.
    fn session(&self, id: SessionId) -> Option<SessionDetail>;
    /// The task board for one session (FR-009/017).
    fn task_board(&self, id: SessionId) -> Vec<TrackedTask>;
    /// The aggregate tasks board across ALL sessions (FR-025a) — the landing view. Rows
    /// arrive grouped by task status with a stable order inside each group; filter with
    /// [`daedalus_proto::TasksFilter`].
    fn all_tasks(&self) -> Vec<AggregateTask>;
    /// Latest resource usage for one session (FR-019).
    fn resource_usage(&self, id: SessionId) -> Vec<ResourceUsageMetric>;
    /// Recent history of one metric, oldest → newest — the telemetry rail's sparklines
    /// (FR-019 trends).
    fn resource_history(
        &self,
        id: SessionId,
        metric: MetricKind,
        limit: usize,
    ) -> Vec<ResourceUsageMetric>;
    /// The session's persisted events, oldest first — the lifecycle/operator-action
    /// timeline (FR-018, FR-019a).
    fn session_events(&self, id: SessionId) -> Vec<EventRecord>;
    /// The "Needs you" queue: every session blocked on the operator, most answerable
    /// first (FR-021a/b). Empty means nothing needs you; surfaces count items for badges.
    fn needs_you(&self) -> Vec<AttentionItem>;
}

/// Async queries that must reach out to sources/backends.
pub trait AppQueryAsync {
    /// De-duplicated discovered sessions (FR-010/013/014).
    fn discovered(&self) -> impl std::future::Future<Output = Vec<DiscoveredSession>> + Send;
    /// Backend availability rows (FR-028).
    fn backends(&self) -> impl std::future::Future<Output = Vec<BackendStatus>> + Send;
}
