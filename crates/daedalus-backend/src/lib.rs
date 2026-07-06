//! The backend abstraction every sandbox provider implements.
//!
//! A backend turns operator intent into an *isolated* environment and a running agent,
//! and reports usage and lifecycle. Backend-specific behaviour stays behind this trait so
//! adding a backend never changes the operator workflow or the surfaces (FR-027,
//! contract `contracts/backend.md`).

use async_trait::async_trait;
use thiserror::Error;

use daedalus_proto::{
    Availability, BackendId, BackendKind, EnvLifecycle, EnvironmentId, InvocationSpec, MetricKind,
    Origin, ResourceLimits, SandboxEnvironment, WorktreeRef,
};

/// What the operator asked for when acquiring an environment.
#[derive(Debug, Clone, PartialEq)]
pub struct AcquireRequest {
    /// Fresh environment or attach to a pre-existing one.
    pub origin: Origin,
    /// For [`Origin::PreExisting`], the worktree to isolate into (FR-002a). Required when
    /// `origin == PreExisting`; a backend that cannot create it returns
    /// [`BackendError::WorktreeUnavailable`].
    pub worktree: Option<WorktreeRef>,
    /// Resource policy to apply.
    pub limits: ResourceLimits,
}

impl AcquireRequest {
    /// A freshly-provisioned, ready [`SandboxEnvironment`] honouring this request's
    /// origin/worktree — the shape every backend returns from a successful `acquire`.
    #[must_use]
    pub fn into_ready_environment(self, backend: BackendId) -> SandboxEnvironment {
        SandboxEnvironment {
            id: EnvironmentId::new(),
            backend_id: backend,
            origin: self.origin,
            worktree_ref: self.worktree,
            lifecycle: EnvLifecycle::Ready,
        }
    }
}

/// Reject a [`Origin::PreExisting`] acquire that carries no worktree reference — an
/// isolated worktree is required and its absence leaves no environment behind
/// (FR-002a, contract C-B1).
pub fn require_worktree(req: &AcquireRequest) -> Result<(), BackendError> {
    if req.origin == Origin::PreExisting && req.worktree.is_none() {
        return Err(BackendError::WorktreeUnavailable(
            "pre-existing environment requires a worktree reference".into(),
        ));
    }
    Ok(())
}

/// A handle to a launched agent, including the zellij session it runs under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentHandle {
    /// The environment the agent runs in.
    pub env: EnvironmentId,
    /// The named zellij session the agent's terminal is attached to.
    pub zellij_session: String,
}

/// A single resource-usage sample for an environment.
///
/// Backend-local (no session id / timestamp): the core stamps those when it maps a sample
/// onto a [`daedalus_proto::ResourceUsageMetric`] for a specific session (FR-019).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UsageSample {
    /// Which metric this samples.
    pub metric: MetricKind,
    /// The observed value (units per [`MetricKind`]).
    pub value: f64,
}

/// Failures a backend can report. Availability is *not* among them — an unavailable
/// backend is a value, never an error (FR-028, contract C-B4).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum BackendError {
    /// `acquire` with [`Origin::PreExisting`] could not establish an isolated worktree.
    /// No environment is left behind (FR-002a, contract C-B1).
    #[error("could not establish an isolated worktree: {0}")]
    WorktreeUnavailable(String),
    /// Provisioning a fresh environment failed; nothing is left behind (FR-005, C-B2).
    #[error("could not provision an environment: {0}")]
    ProvisionFailed(String),
    /// The agent could not be started; the environment is torn down (FR-005, C-B2).
    #[error("could not start the agent: {0}")]
    StartFailed(String),
    /// The referenced environment is unknown to this backend.
    #[error("no such environment")]
    NotFound,
    /// The operation is not supported by this backend.
    #[error("operation not supported by this backend: {0}")]
    Unsupported(String),
    /// A lower-level I/O or control-surface failure.
    #[error("backend i/o error: {0}")]
    Io(String),
}

/// One common async trait for all sandbox backends (`contracts/backend.md`).
#[async_trait]
pub trait Backend: Send + Sync {
    /// This backend's stable identifier.
    fn id(&self) -> BackendId;

    /// Which kind of backend this is.
    fn kind(&self) -> BackendKind;

    /// Current availability; MUST NOT error — unavailability is a value (FR-028, C-B4).
    async fn availability(&self) -> Availability;

    /// Stated reason when degraded/unavailable (FR-028), e.g. "high memory pressure".
    /// `None` for a healthy backend (and for backends that cannot state one).
    async fn availability_reason(&self) -> Option<String> {
        None
    }

    /// Provision a fresh isolated environment, or attach a pre-existing one.
    ///
    /// For [`Origin::PreExisting`], MUST establish an isolated git worktree; if it cannot,
    /// return [`BackendError::WorktreeUnavailable`] and leave NO orphaned environment
    /// (FR-002a, FR-005, contract C-B1/C-B2).
    async fn acquire(&self, req: AcquireRequest) -> Result<SandboxEnvironment, BackendError>;

    /// Launch the agentic tool inside the environment, under zellij, confined to the
    /// sandbox (FR-004). On failure the environment is torn down (C-B2).
    async fn start_agent(
        &self,
        env: &EnvironmentId,
        tool: &InvocationSpec,
    ) -> Result<AgentHandle, BackendError>;

    /// Observed resource usage for the environment (FR-019).
    async fn resource_usage(&self, env: &EnvironmentId) -> Result<Vec<UsageSample>, BackendError>;

    /// Stop the running agent (FR-022). MUST be idempotent (contract C-B5).
    async fn stop(&self, env: &EnvironmentId) -> Result<(), BackendError>;

    /// Release/tear down the environment, freeing resources without affecting others
    /// (FR-024). MUST be idempotent and isolated to one environment (C-B5).
    async fn teardown(&self, env: &EnvironmentId) -> Result<(), BackendError>;
}
