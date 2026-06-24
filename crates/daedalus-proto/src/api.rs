//! Surface-facing DTOs and the push event enum (contract `contracts/app-api.md`).
//!
//! These live in `daedalus-proto` so the core, the app service, and every surface share
//! them verbatim. The `Command` enum and the `AppQuery` trait live in `daedalus-app`; the
//! data they carry is defined here.

use serde::{Deserialize, Serialize};

use crate::entities::{
    AgenticTool, Capabilities, InvocationSpec, Objective, ResourceUsageMetric, SandboxEnvironment,
    Session, TrackedTask, WorktreeRef,
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
}

/// Backend availability row (FR-028).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendStatus {
    /// Backend id.
    pub id: BackendId,
    /// Backend kind.
    pub kind: BackendKind,
    /// Current availability.
    pub availability: Availability,
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
