//! Daedalus orchestration core.
//!
//! Owns all orchestration — sessions and the lifecycle state machine, the backend
//! registry, discovery coordination, persistence, reconciliation, and the redacted output
//! pipeline. Surfaces (the GPUI desktop, and a future web UI) issue commands and queries
//! and subscribe to [`AppEvent`]s; they hold no orchestration logic (FR-007a,
//! `contracts/app-api.md`).

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

/// Tunable core policy.
#[derive(Debug, Clone)]
pub struct CoreConfig {
    /// Seconds without output/progress before a running session is marked `Stalled`.
    pub stall_interval_secs: u64,
    /// Maximum concurrent (non-terminal) sessions; `None` = unlimited (FR-026).
    pub concurrency_limit: Option<usize>,
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            stall_interval_secs: DEFAULT_STALL_INTERVAL_SECS,
            concurrency_limit: None,
        }
    }
}

/// In-memory bookkeeping for a live session (lost on restart; rebuilt by reconciliation).
pub(crate) struct RuntimeHandle {
    pub(crate) backend_id: daedalus_proto::BackendId,
    pub(crate) environment_id: EnvironmentId,
    #[allow(dead_code)]
    pub(crate) zellij_session: String,
}

/// The orchestration core.
pub struct Core {
    pub(crate) store: Arc<Store>,
    pub(crate) backends: BackendRegistry,
    pub(crate) terminal: Arc<dyn TerminalAttach>,
    pub(crate) discovery: Option<Arc<DiscoveryCoordinator>>,
    pub(crate) config: CoreConfig,
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
        Self {
            store,
            backends,
            terminal,
            discovery,
            config,
            events,
            runtime: Mutex::new(HashMap::new()),
            delivered_input: Mutex::new(HashMap::new()),
        }
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

    // ----- Queries (read current state) -----

    /// Full detail for the session screen.
    pub fn session_detail(&self, id: SessionId) -> Result<SessionDetail, CoreError> {
        let session = self.store.get_session(id).map_err(map_not_found)?;
        let tool = self.store.get_tool(session.tool_id)?;
        let objective = self.store.get_objective(session.objective_id)?;
        let environment = self.store.get_environment(session.environment_id)?;
        let tasks = self.store.list_tasks(id)?;
        let metrics = self.store.latest_metrics(id)?;
        Ok(SessionDetail {
            session,
            tool,
            objective,
            environment,
            tasks,
            metrics,
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

    /// The tail of a session's redacted captured output, for display (FR-016).
    #[must_use]
    pub fn session_output(&self, id: SessionId, max_bytes: usize) -> String {
        self.store.output_tail(id, max_bytes)
    }

    /// Backend availability rows (FR-028).
    pub async fn backends_status(&self) -> Vec<daedalus_proto::BackendStatus> {
        self.backends.statuses().await
    }

    /// Discovered sessions across all sources, de-duplicated (FR-010/013/014).
    pub async fn discovered(&self) -> Vec<DiscoveredSession> {
        match &self.discovery {
            Some(coord) => coord.discover().await,
            None => Vec::new(),
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

    /// Sample the backend for a session's resource usage and persist it (FR-019, T034).
    pub async fn record_resource_usage(&self, id: SessionId) -> Result<(), CoreError> {
        let (backend_id, env) = {
            let rt = self.runtime.lock().expect("poisoned");
            match rt.get(&id) {
                Some(h) => (h.backend_id, h.environment_id),
                None => return Ok(()),
            }
        };
        let Some(backend) = self.backends.by_id(backend_id) else {
            return Ok(());
        };
        let samples = backend.resource_usage(&env).await?;
        let now = clock::now();
        for s in samples {
            let metric = ResourceUsageMetric {
                session_id: id,
                metric: s.metric,
                value: s.value,
                timestamp: now,
            };
            self.store.insert_metric(&metric)?;
        }
        Ok(())
    }

    /// Attach to a session's live terminal, redact + persist the captured stream, and
    /// return a channel whose output is the redacted stream for the surface to render
    /// (FR-016, FR-018; contract `terminal-attach.md`, C-T4).
    pub async fn attach_and_capture(&self, id: SessionId) -> Result<TerminalChannel, CoreError> {
        let channel = self
            .terminal
            .attach(id)
            .await
            .map_err(|e| CoreError::NotAttachable(e.to_string()))?;

        let (ui_tx, ui_rx) = mpsc::channel::<Bytes>(1024);
        let store = Arc::clone(&self.store);
        let events = self.events.clone();
        let mut output = channel.output;
        tokio::spawn(async move {
            while let Some(chunk) = output.recv().await {
                let redacted = redact::redact_bytes(&chunk);
                let _ = store.append_output(id, clock::now(), &redacted);
                let _ = events.send(AppEvent::Output {
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

/// Map a store `NotFound` to the orchestration-level [`CoreError::UnknownSession`].
pub(crate) fn map_not_found(e: StoreError) -> CoreError {
    match e {
        StoreError::NotFound => CoreError::UnknownSession,
        other => CoreError::Store(other),
    }
}
