//! Start-session flow (FR-001/002/002a/003/005/006) + the concurrency limit (FR-026).
//!
//! Ordering is chosen so that **any** failure before the agent is running leaves no session
//! record and no orphaned environment (contract C-A1, C-B2): the environment is acquired
//! first; only on a successful agent start is the session persisted as `Running` and a
//! runtime handle recorded. A failed start tears the environment down and removes the
//! transient record.

use daedalus_backend::AcquireRequest;
use daedalus_proto::{
    Availability, EnvLifecycle, ResourceLimits, Session, SessionId, SessionStatus, Source,
    SourceId, SourceKind, StartSessionRequest,
};

use crate::session::state::{transition, Trigger};
use crate::{clock, Core, CoreError, RuntimeHandle};

impl Core {
    /// Start a session: bind tool + objective + environment + source, assign a unique id,
    /// provision fresh or attach a pre-existing (worktree-isolated) environment, and launch
    /// the agent.
    pub async fn start_session(&self, req: StartSessionRequest) -> Result<SessionId, CoreError> {
        // 1. The tool must be registered.
        let tool = self.store.get_tool(req.tool_id).map_err(|e| match e {
            crate::StoreError::NotFound => CoreError::UnknownTool,
            other => CoreError::Store(other),
        })?;

        // 2. Enforce the configurable concurrency limit (FR-026).
        if let Some(limit) = self.config.concurrency_limit {
            let active = self
                .store
                .list_sessions()?
                .iter()
                .filter(|s| !s.status.is_terminal())
                .count();
            if active >= limit {
                return Err(CoreError::ConcurrencyLimitReached(limit));
            }
        }

        // 3. Select an available backend of the requested kind (FR-027/028).
        let backend = self
            .backends
            .by_kind(req.backend)
            .ok_or(CoreError::BackendUnavailable(req.backend))?;
        if backend.availability().await == Availability::Unavailable {
            return Err(CoreError::BackendUnavailable(req.backend));
        }

        // 4. Acquire the environment. On failure: NO session record is created (C-A1, C-B2).
        let acquire = AcquireRequest {
            origin: req.origin,
            worktree: req.worktree.clone(),
            limits: ResourceLimits::default(),
        };
        let env = backend
            .acquire(acquire)
            .await
            .map_err(|e| CoreError::Acquire(e.to_string()))?;

        // 5. Persist the supporting records and a transient `Starting` session.
        self.store.upsert_objective(&req.objective)?;
        self.store.upsert_environment(&env)?;
        let source = Source {
            id: SourceId::new(),
            kind: SourceKind::Local,
            availability: Availability::Available,
        };
        self.store.upsert_source(&source)?;

        let id = SessionId::new();
        let session = Session {
            id,
            tool_id: tool.id,
            objective_id: req.objective.id,
            environment_id: env.id,
            source_id: source.id,
            status: SessionStatus::Starting,
            created_at: clock::now(),
            started_at: None,
            ended_at: None,
            terminal_outcome: None,
            accepts_input: tool.capabilities.accepts_interactive_input,
        };
        self.store.upsert_session(&session)?;
        self.record_lifecycle(id, SessionStatus::Starting, None);

        // 6. Launch the agent. On failure: tear the environment down and remove the record.
        match backend.start_agent(&env.id, &tool.invocation).await {
            Ok(handle) => {
                let running = transition(SessionStatus::Starting, Trigger::Started)
                    .map_err(|e| CoreError::IllegalTransition(e.to_string()))?;
                self.store.set_session_started(id, clock::now())?;
                self.store.set_session_status(id, running, None, None)?;
                self.runtime.lock().expect("poisoned").insert(
                    id,
                    RuntimeHandle {
                        backend_id: backend.id(),
                        environment_id: env.id,
                        zellij_session: handle.zellij_session,
                    },
                );
                self.record_lifecycle(id, running, None);
                Ok(id)
            }
            Err(e) => {
                let _ = backend.teardown(&env.id).await;
                let _ = self.store.delete_session(id);
                let _ = self
                    .store
                    .set_environment_lifecycle(env.id, EnvLifecycle::Released);
                Err(CoreError::StartFailed(e.to_string()))
            }
        }
    }
}
