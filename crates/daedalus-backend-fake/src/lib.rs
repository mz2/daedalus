//! An in-memory [`Backend`] implementation.
//!
//! It lets the whole core — sessions, lifecycle, discovery, persistence — be exercised on a
//! developer's (or agent's) machine without Canonical Workshop or any remote access
//! (Constitution Principle III). It also asserts the *shape* of the isolation contract:
//! the fake never exposes any host handle, so a "hostile" agent has nothing to reach
//! (contract C-B3; real isolation is proven against the real backends).

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;

use daedalus_backend::{AcquireRequest, AgentHandle, Backend, BackendError, UsageSample};
use daedalus_proto::{
    Availability, BackendId, BackendKind, EnvLifecycle, EnvironmentId, InvocationSpec, MetricKind,
    Origin, SandboxEnvironment,
};

/// Per-environment bookkeeping inside the fake.
#[derive(Debug, Clone)]
struct EnvState {
    /// Whether an agent is currently running in this environment.
    agent_running: bool,
}

#[derive(Debug, Default)]
struct Inner {
    envs: HashMap<EnvironmentId, EnvState>,
    /// When set, the next `acquire` fails with [`BackendError::ProvisionFailed`].
    fail_next_acquire: Option<String>,
    /// When set, a `PreExisting` acquire fails with [`BackendError::WorktreeUnavailable`].
    fail_worktree: Option<String>,
    /// When set, the next `start_agent` fails (and the environment is torn down).
    fail_next_start: Option<String>,
}

/// Configurable in-memory backend.
pub struct FakeBackend {
    id: BackendId,
    availability: Mutex<(Availability, Option<String>)>,
    inner: Mutex<Inner>,
}

impl Default for FakeBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeBackend {
    /// A fresh, available fake backend.
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: BackendId::new(),
            availability: Mutex::new((Availability::Available, None)),
            inner: Mutex::new(Inner::default()),
        }
    }

    /// Override the availability the backend reports (FR-028 testing).
    pub fn set_availability(&self, availability: Availability) {
        self.set_availability_with_reason(availability, None);
    }

    /// Override the availability together with its stated reason (FR-028): degraded or
    /// unavailable backends state *why* (e.g. resource pressure).
    pub fn set_availability_with_reason(&self, availability: Availability, reason: Option<&str>) {
        *self.availability.lock().expect("poisoned") = (availability, reason.map(str::to_string));
    }

    /// Make the next `acquire` fail as if provisioning was impossible (C-A1, C-B2).
    pub fn fail_next_acquire(&self, reason: impl Into<String>) {
        self.inner.lock().expect("poisoned").fail_next_acquire = Some(reason.into());
    }

    /// Make `PreExisting` acquires fail as if a worktree could not be created (C-B1).
    pub fn fail_worktree(&self, reason: impl Into<String>) {
        self.inner.lock().expect("poisoned").fail_worktree = Some(reason.into());
    }

    /// Make the next `start_agent` fail (the environment is torn down — C-B2).
    pub fn fail_next_start(&self, reason: impl Into<String>) {
        self.inner.lock().expect("poisoned").fail_next_start = Some(reason.into());
    }

    /// Number of environments currently held (0 proves nothing was orphaned — C-B2).
    #[must_use]
    pub fn live_environment_count(&self) -> usize {
        self.inner.lock().expect("poisoned").envs.len()
    }

    /// Whether an agent is currently running in the given environment.
    #[must_use]
    pub fn agent_running(&self, env: &EnvironmentId) -> bool {
        self.inner
            .lock()
            .expect("poisoned")
            .envs
            .get(env)
            .is_some_and(|e| e.agent_running)
    }
}

#[async_trait]
impl Backend for FakeBackend {
    fn id(&self) -> BackendId {
        self.id
    }

    fn kind(&self) -> BackendKind {
        BackendKind::Fake
    }

    async fn availability(&self) -> Availability {
        self.availability.lock().expect("poisoned").0
    }

    async fn availability_reason(&self) -> Option<String> {
        self.availability.lock().expect("poisoned").1.clone()
    }

    async fn acquire(&self, req: AcquireRequest) -> Result<SandboxEnvironment, BackendError> {
        let mut inner = self.inner.lock().expect("poisoned");

        if let Some(reason) = inner.fail_next_acquire.take() {
            // Failure leaves NO environment behind (C-B2).
            return Err(BackendError::ProvisionFailed(reason));
        }

        if req.origin == Origin::PreExisting {
            if let Some(reason) = inner.fail_worktree.clone() {
                // Un-creatable worktree ⇒ WorktreeUnavailable, no environment (C-B1).
                return Err(BackendError::WorktreeUnavailable(reason));
            }
            if req.worktree.is_none() {
                return Err(BackendError::WorktreeUnavailable(
                    "pre-existing environment requires a worktree reference".into(),
                ));
            }
        }

        let env = SandboxEnvironment {
            id: EnvironmentId::new(),
            backend_id: self.id,
            origin: req.origin,
            worktree_ref: req.worktree,
            lifecycle: EnvLifecycle::Ready,
        };
        inner.envs.insert(
            env.id,
            EnvState {
                agent_running: false,
            },
        );
        Ok(env)
    }

    async fn start_agent(
        &self,
        env: &EnvironmentId,
        _tool: &InvocationSpec,
    ) -> Result<AgentHandle, BackendError> {
        let mut inner = self.inner.lock().expect("poisoned");
        if !inner.envs.contains_key(env) {
            return Err(BackendError::NotFound);
        }
        if let Some(reason) = inner.fail_next_start.take() {
            // A failed start tears down the environment — no orphan (C-B2).
            inner.envs.remove(env);
            return Err(BackendError::StartFailed(reason));
        }
        let state = inner.envs.get_mut(env).expect("checked above");
        state.agent_running = true;
        Ok(AgentHandle {
            env: *env,
            zellij_session: format!("daedalus-{env}"),
        })
    }

    async fn resource_usage(&self, env: &EnvironmentId) -> Result<Vec<UsageSample>, BackendError> {
        let inner = self.inner.lock().expect("poisoned");
        if !inner.envs.contains_key(env) {
            return Err(BackendError::NotFound);
        }
        // Deterministic, plausible samples for local testing.
        Ok(vec![
            UsageSample {
                metric: MetricKind::Cpu,
                value: 12.5,
            },
            UsageSample {
                metric: MetricKind::Memory,
                value: 256.0 * 1024.0 * 1024.0,
            },
            UsageSample {
                metric: MetricKind::Disk,
                value: 64.0 * 1024.0 * 1024.0,
            },
            UsageSample {
                metric: MetricKind::Time,
                value: 1.0,
            },
        ])
    }

    async fn stop(&self, env: &EnvironmentId) -> Result<(), BackendError> {
        // Idempotent: stopping an unknown / already-stopped env is Ok (C-B5).
        if let Some(state) = self.inner.lock().expect("poisoned").envs.get_mut(env) {
            state.agent_running = false;
        }
        Ok(())
    }

    async fn teardown(&self, env: &EnvironmentId) -> Result<(), BackendError> {
        // Idempotent and isolated to one environment (C-B5).
        self.inner.lock().expect("poisoned").envs.remove(env);
        Ok(())
    }
}
