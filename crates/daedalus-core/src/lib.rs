//! Daedalus orchestration core.
//!
//! Owns all orchestration — sessions and the lifecycle state machine, the backend
//! registry, discovery coordination, persistence, reconciliation, and the redacted output
//! pipeline. Surfaces (the GPUI desktop, and a future web UI) issue commands and queries
//! and subscribe to [`AppEvent`]s; they hold no orchestration logic (FR-007a,
//! `contracts/app-api.md`).

pub mod attention;
pub mod backends;
pub mod clock;
pub mod fleet;
pub mod persist;
pub mod reconcile;
pub mod redact;
pub mod session;
pub mod tasks;
pub mod tools;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use thiserror::Error;
use tokio::sync::{broadcast, mpsc};

use daedalus_backend::BackendError;
use daedalus_discovery::DiscoveryCoordinator;
use daedalus_proto::{
    AppEvent, BackendKind, DiscoveredSession, DiscoveredSessionId, EnvironmentId, EventKind,
    EventPayload, EventRecord, RedactedBytes, ResourceUsageMetric, SessionDetail, SessionId,
    SessionStatus, TrackedTask,
};
use daedalus_zellij::{TerminalAttach, TerminalChannel};

pub use backends::BackendRegistry;
pub use persist::{Store, StoreError};
pub use session::outcome::AgentSignal;
pub use session::state::{can_transition, transition, Trigger};

/// Default stall interval (FR-020, data-model: 120s).
pub const DEFAULT_STALL_INTERVAL_SECS: u64 = 120;

/// Errors the orchestration core can return, each with a stated reason (C-A1/C-A2).
#[derive(Debug, Error)]
pub enum CoreError {
    /// No tool with the given id is registered.
    #[error("unknown tool")]
    UnknownTool,
    /// A tool with this name is already registered.
    #[error("a tool named '{0}' is already registered")]
    DuplicateTool(String),
    /// The tool definition is invalid (e.g. empty name/program).
    #[error("invalid tool definition: {0}")]
    InvalidTool(String),
    /// No available backend of the requested kind (FR-028). No session record is created.
    #[error("backend '{0:?}' is unavailable")]
    BackendUnavailable(BackendKind),
    /// The configured concurrency limit was reached (FR-026).
    #[error("concurrency limit reached ({0} sessions)")]
    ConcurrencyLimitReached(usize),
    /// Provisioning/acquire failed; no session record is created (C-A1, C-B2).
    #[error("could not provision an environment: {0}")]
    Acquire(String),
    /// Starting the agent failed; the environment is torn down and no record remains (C-B2).
    #[error("could not start the agent: {0}")]
    StartFailed(String),
    /// No session with the given id.
    #[error("unknown session")]
    UnknownSession,
    /// The tool does not accept interactive input (FR-023, C-A2).
    #[error("this session does not accept input")]
    InputNotAccepted,
    /// The session/discovered session cannot be attached (C-D3, C-T2).
    #[error("not attachable: {0}")]
    NotAttachable(String),
    /// The requested lifecycle transition is illegal from the current state.
    #[error("{0}")]
    IllegalTransition(String),
    /// A persistence failure.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// A backend failure.
    #[error(transparent)]
    Backend(#[from] BackendError),
}

/// What happens when a session's environment exceeds one of its resource limits (spec
/// edge case "Resource exhaustion"): the breach is always surfaced to the operator; the
/// policy governs whether the session is also stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LimitPolicy {
    /// Surface the breach and stop the session (default).
    #[default]
    StopSession,
    /// Surface the breach but leave the session running.
    NotifyOnly,
}

/// Tunable core policy.
#[derive(Debug, Clone)]
pub struct CoreConfig {
    /// Seconds without output/progress before a running session is marked `Stalled`.
    pub stall_interval_secs: u64,
    /// Maximum concurrent (non-terminal) sessions; `None` = unlimited (FR-026).
    pub concurrency_limit: Option<usize>,
    /// Response to a session exceeding its resource limits.
    pub limit_policy: LimitPolicy,
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            stall_interval_secs: DEFAULT_STALL_INTERVAL_SECS,
            concurrency_limit: None,
            limit_policy: LimitPolicy::default(),
        }
    }
}

/// In-memory bookkeeping for a live session (lost on restart; rebuilt by reconciliation).
pub(crate) struct RuntimeHandle {
    pub(crate) backend_id: daedalus_proto::BackendId,
    pub(crate) environment_id: EnvironmentId,
    #[allow(dead_code)]
    pub(crate) zellij_session: String,
    /// Resource limits the environment was acquired with, enforced against observed
    /// usage per the configured [`LimitPolicy`].
    pub(crate) limits: daedalus_proto::ResourceLimits,
}

/// The orchestration core.
pub struct Core {
    pub(crate) store: Arc<Store>,
    pub(crate) backends: BackendRegistry,
    pub(crate) terminal: Arc<dyn TerminalAttach>,
    pub(crate) discovery: Option<Arc<DiscoveryCoordinator>>,
    pub(crate) config: Mutex<CoreConfig>,
    pub(crate) events: broadcast::Sender<AppEvent>,
    pub(crate) runtime: Mutex<HashMap<SessionId, RuntimeHandle>>,
    pub(crate) delivered_input: Mutex<HashMap<SessionId, Vec<Bytes>>>,
}

impl Core {
    /// Build a core over a store, backend registry, terminal attach implementation, and an
    /// optional discovery coordinator.
    #[must_use]
    pub fn new(
        store: Arc<Store>,
        backends: BackendRegistry,
        terminal: Arc<dyn TerminalAttach>,
        discovery: Option<Arc<DiscoveryCoordinator>>,
        config: CoreConfig,
    ) -> Self {
        let (events, _) = broadcast::channel(1024);
        // Re-seed persisted operator configuration into the registry (FR-021b): idle
        // rates survive restart.
        if let Ok(rates) = store.backend_idle_rates() {
            for (kind, rate) in rates {
                backends.set_idle_rate(kind, Some(rate));
            }
        }
        Self {
            store,
            backends,
            terminal,
            discovery,
            config: Mutex::new(config),
            events,
            runtime: Mutex::new(HashMap::new()),
            delivered_input: Mutex::new(HashMap::new()),
        }
    }

    /// Current core configuration snapshot.
    #[must_use]
    pub fn config(&self) -> CoreConfig {
        self.config.lock().expect("poisoned").clone()
    }

    /// Update the concurrency limit at runtime (FR-026).
    pub fn set_concurrency_limit(&self, limit: Option<usize>) {
        self.config.lock().expect("poisoned").concurrency_limit = limit;
    }

    /// Update the stall interval (seconds) at runtime (FR-020).
    pub fn set_stall_interval(&self, secs: u64) {
        self.config.lock().expect("poisoned").stall_interval_secs = secs;
    }

    /// Set (or clear) the per-backend idle rate used for waiting-cost estimates
    /// (FR-021b): applied to the registry immediately and persisted so it survives
    /// restart.
    pub fn set_idle_rate(&self, kind: BackendKind, rate: Option<f64>) -> Result<(), CoreError> {
        self.backends.set_idle_rate(kind, rate);
        self.store.set_backend_idle_rate(kind, rate)?;
        Ok(())
    }

    /// Subscribe to the push event stream (drives live UI updates without polling).
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.events.subscribe()
    }

    /// The backend registry (availability queries, etc.).
    #[must_use]
    pub fn backend_registry(&self) -> &BackendRegistry {
        &self.backends
    }

    /// The persistence store.
    #[must_use]
    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    pub(crate) fn emit(&self, event: AppEvent) {
        // A send error only means there are no subscribers — not a failure.
        let _ = self.events.send(event);
    }

    /// Record a lifecycle event and emit the matching status-change/notification events.
    pub(crate) fn record_lifecycle(
        &self,
        id: SessionId,
        status: SessionStatus,
        note: Option<String>,
    ) {
        let event = EventRecord {
            id: daedalus_proto::EventId::new(),
            session_id: id,
            timestamp: clock::now(),
            kind: EventKind::Lifecycle,
            payload: EventPayload::Lifecycle {
                status,
                note: note.clone(),
            },
        };
        let _ = self.store.insert_event(&event);
        self.emit(AppEvent::SessionStatusChanged { id, status });
        if status.is_terminal() || status.is_attention() {
            self.emit(AppEvent::Notification(daedalus_proto::TerminalOrAbnormal {
                session: id,
                status,
                note,
            }));
        }
    }

    /// Record a key operator action on the session's persisted timeline (FR-019a):
    /// start / stop / input-sent / confirm / clean-up.
    pub(crate) fn record_operator_action(
        &self,
        id: SessionId,
        action: daedalus_proto::OperatorAction,
    ) {
        let event = EventRecord {
            id: daedalus_proto::EventId::new(),
            session_id: id,
            timestamp: clock::now(),
            kind: EventKind::OperatorAction,
            payload: EventPayload::OperatorAction { action },
        };
        let _ = self.store.insert_event(&event);
    }

    // ----- Queries (read current state) -----

    /// Full detail for the session screen.
    pub fn session_detail(&self, id: SessionId) -> Result<SessionDetail, CoreError> {
        let session = self.store.get_session(id).map_err(map_not_found)?;
        let tool = self.store.get_tool(session.tool_id)?;
        let objective = self.store.get_objective(session.objective_id)?;
        let environment = self.store.get_environment(session.environment_id)?;
        let tasks = self.store.list_tasks(id)?;
        let metrics = self.store.latest_metrics(id)?;
        let capture = daedalus_proto::CaptureStats {
            total_lines: self.store.capture_line_count(id),
        };
        Ok(SessionDetail {
            session,
            tool,
            objective,
            environment,
            tasks,
            metrics,
            capture,
        })
    }

    /// The current task board for a session (FR-009/017).
    pub fn task_board(&self, id: SessionId) -> Result<Vec<TrackedTask>, CoreError> {
        Ok(self.store.list_tasks(id)?)
    }

    /// Latest resource usage for a session (FR-019).
    pub fn resource_usage(&self, id: SessionId) -> Result<Vec<ResourceUsageMetric>, CoreError> {
        Ok(self.store.latest_metrics(id)?)
    }

    /// Recent history of one metric for a session, oldest → newest (FR-019 trends).
    pub fn resource_history(
        &self,
        id: SessionId,
        metric: daedalus_proto::MetricKind,
        limit: usize,
    ) -> Result<Vec<ResourceUsageMetric>, CoreError> {
        Ok(self.store.metric_history(id, metric, limit)?)
    }

    /// The session's persisted event records, oldest first — the lifecycle/operator-action
    /// timeline (FR-018, FR-019a).
    pub fn session_events(&self, id: SessionId) -> Result<Vec<EventRecord>, CoreError> {
        Ok(self.store.list_events(id)?)
    }

    /// The tail of a session's redacted captured output, for display (FR-016).
    #[must_use]
    pub fn session_output(&self, id: SessionId, max_bytes: usize) -> String {
        self.store.output_tail(id, max_bytes)
    }

    /// Backend availability rows (FR-028).
    pub async fn backends_status(&self) -> Vec<daedalus_proto::BackendStatus> {
        self.backends.statuses().await
    }

    /// Every environment record (§6.7 — the Environments screen).
    pub fn environments(&self) -> Result<Vec<daedalus_proto::SandboxEnvironment>, CoreError> {
        Ok(self.store.list_environments()?)
    }

    /// Discovered sessions across all sources, de-duplicated (FR-010/013/014).
    pub async fn discovered(&self) -> Vec<DiscoveredSession> {
        self.discovered_counted().await.0
    }

    /// Like [`Self::discovered`], but also reports how many sessions were advertised from
    /// multiple sources and de-duplicated (FR-013 — the Discover footnote).
    pub async fn discovered_counted(&self) -> (Vec<DiscoveredSession>, usize) {
        match &self.discovery {
            Some(coord) => coord.discover_counted().await,
            None => (Vec::new(), 0),
        }
    }

    /// Connect to a discovered session by attaching via zellij (FR-011). Returns the
    /// resolved [`DiscoveredSession`] (which carries the zellij session to attach), or a
    /// clear non-attachable reason (C-D3).
    pub async fn connect_discovered(
        &self,
        id: DiscoveredSessionId,
    ) -> Result<DiscoveredSession, CoreError> {
        let discovered = self.discovered().await;
        let found = discovered
            .into_iter()
            .find(|d| d.id == id)
            .ok_or_else(|| CoreError::NotAttachable("no such discovered session".into()))?;
        if !found.attachable {
            return Err(CoreError::NotAttachable(
                found
                    .attach_reason
                    .clone()
                    .unwrap_or_else(|| "session is not attachable".into()),
            ));
        }
        self.emit(AppEvent::DiscoveryChanged);
        Ok(found)
    }

    /// Delete a session record (and best-effort clean up if it is still live). Records are
    /// retained until the operator deletes them — no auto-expiry (FR-030a).
    pub async fn delete_record(&self, id: SessionId) -> Result<(), CoreError> {
        // Best-effort cleanup of any live environment first.
        if self.store.get_session(id).is_ok() {
            let _ = self.clean_up(id).await;
        }
        self.store.delete_session(id)?;
        self.emit(AppEvent::DiscoveryChanged);
        Ok(())
    }

    /// Sample the backend for a session's resource usage, persist it (FR-019, T034), and
    /// enforce the environment's resource limits per the configured [`LimitPolicy`]
    /// (spec edge case "Resource exhaustion").
    pub async fn record_resource_usage(&self, id: SessionId) -> Result<(), CoreError> {
        let (backend_id, env, limits) = {
            let rt = self.runtime.lock().expect("poisoned");
            match rt.get(&id) {
                Some(h) => (h.backend_id, h.environment_id, h.limits),
                None => return Ok(()),
            }
        };
        let Some(backend) = self.backends.by_id(backend_id) else {
            return Ok(());
        };
        let samples = backend.resource_usage(&env).await?;
        let now = clock::now();
        for s in &samples {
            let metric = ResourceUsageMetric {
                session_id: id,
                metric: s.metric,
                value: s.value,
                timestamp: now,
            };
            self.store.insert_metric(&metric)?;
        }

        if let Some(breached) = samples.iter().find_map(|s| exceeded_limit(s, &limits)) {
            self.handle_limit_breach(id, breached).await?;
        }
        Ok(())
    }

    /// Surface a resource-limit breach to the operator and apply the configured
    /// [`LimitPolicy`] (spec edge case "Resource exhaustion"): the breach is always
    /// surfaced; `StopSession` also halts the agent and records the session as stopped
    /// with the stated reason — other sessions are unaffected.
    async fn handle_limit_breach(&self, id: SessionId, metric: &str) -> Result<(), CoreError> {
        let reason = format!("resource limit exceeded: {metric}");
        let session = self.store.get_session(id).map_err(map_not_found)?;
        if session.status.is_terminal() {
            return Ok(()); // already resolved — nothing to enforce
        }
        if self.config.lock().expect("poisoned").limit_policy == LimitPolicy::NotifyOnly {
            self.emit(AppEvent::Notification(daedalus_proto::TerminalOrAbnormal {
                session: id,
                status: session.status,
                note: Some(reason),
            }));
            return Ok(());
        }

        // Stop policy: halt the agent, then record the stop with its stated reason
        // (record_lifecycle emits the status change + operator notification).
        if let Some((backend_id, env, _)) = self.runtime_lookup(id) {
            if let Some(backend) = self.backends.by_id(backend_id) {
                let _ = backend.stop(&env).await;
            }
        }
        let next = transition(session.status, Trigger::OperatorStopped)
            .map_err(|e| CoreError::IllegalTransition(e.to_string()))?;
        self.store
            .set_session_status(id, next, Some(clock::now()), Some(&reason))?;
        self.record_lifecycle(id, next, Some(reason));
        Ok(())
    }

    /// Attach to a session's live terminal, redact + persist the captured stream, and
    /// return a channel whose output is the redacted stream for the surface to render
    /// (FR-016, FR-018; contract `terminal-attach.md`, C-T4). Each observed chunk is also
    /// matched against the tool's declared prompt pattern, so a blocked agent is surfaced
    /// as waiting-for-input (FR-015b).
    pub async fn attach_and_capture(
        self: &Arc<Self>,
        id: SessionId,
    ) -> Result<TerminalChannel, CoreError> {
        let channel = self
            .terminal
            .attach(id)
            .await
            .map_err(|e| CoreError::NotAttachable(e.to_string()))?;

        let (ui_tx, ui_rx) = mpsc::channel::<Bytes>(1024);
        let core = Arc::clone(self);
        let mut output = channel.output;
        tokio::spawn(async move {
            while let Some(chunk) = output.recv().await {
                let redacted = redact::redact_bytes(&chunk);
                let _ = core.store.append_output(id, clock::now(), &redacted);
                // Waiting-for-input detection over the observed (redacted) stream.
                let _ = core.observe_output(id, &String::from_utf8_lossy(&redacted));
                let _ = core.events.send(AppEvent::Output {
                    id,
                    chunk: RedactedBytes(redacted.to_vec()),
                });
                if ui_tx.send(redacted).await.is_err() {
                    break;
                }
            }
        });

        Ok(TerminalChannel {
            output: ui_rx,
            input: channel.input,
        })
    }
}

/// Whether a usage sample exceeds the matching resource limit; returns the human metric
/// name for the stated reason (e.g. "memory") when it does.
fn exceeded_limit(
    sample: &daedalus_backend::UsageSample,
    limits: &daedalus_proto::ResourceLimits,
) -> Option<&'static str> {
    use daedalus_proto::MetricKind;
    let (limit, name) = match sample.metric {
        MetricKind::Cpu => (limits.cpu_cores, "cpu"),
        MetricKind::Memory => (limits.memory_bytes.map(|b| b as f64), "memory"),
        MetricKind::Disk => (limits.disk_bytes.map(|b| b as f64), "disk"),
        MetricKind::Time => (limits.time_secs.map(|s| s as f64), "time"),
    };
    match limit {
        Some(limit) if sample.value > limit => Some(name),
        _ => None,
    }
}

/// Map a store `NotFound` to the orchestration-level [`CoreError::UnknownSession`].
pub(crate) fn map_not_found(e: StoreError) -> CoreError {
    match e {
        StoreError::NotFound => CoreError::UnknownSession,
        other => CoreError::Store(other),
    }
}
