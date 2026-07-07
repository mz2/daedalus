//! Start-session flow (FR-001/002/002a/003/005/006) + the concurrency limit (FR-026).
//!
//! Ordering is chosen so that **any** failure before the agent is running leaves no session
//! record and no orphaned environment (contract C-A1, C-B2): the environment is acquired
//! first; only on a successful agent start is the session persisted as `Running` and a
//! runtime handle recorded. A failed start tears the environment down and removes the
//! transient record.

use daedalus_backend::AcquireRequest;
use daedalus_proto::{
    Availability, EnvLifecycle, Session, SessionId, SessionStatus, Source, SourceId, SourceKind,
    StartSessionRequest,
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
        if let Some(limit) = self.config.lock().expect("poisoned").concurrency_limit {
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
            limits: req.limits,
        };
        let env = backend
            .acquire(acquire)
            .await
            .map_err(|e| CoreError::Acquire(e.to_string()))?;

        // From here the environment is LIVE: any failure before the agent is confirmed
        // running MUST tear it down so nothing is orphaned (C-B2). The provisioning work
        // runs in a fallible block whose `Err` funnels into the single teardown site below;
        // `created` carries the transient session id (once persisted) so it is removed too.
        let mut created: Option<SessionId> = None;
        let launched: Result<(SessionId, daedalus_backend::AgentHandle), CoreError> = async {
            // 5. Persist the supporting records and a transient `Starting` session.
            self.store.upsert_objective(&req.objective)?;
            self.store.upsert_environment(&env)?;
            let source = Source {
                id: SourceId::new(),
                kind: SourceKind::Local,
                availability: Availability::Available,
                availability_reason: None,
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
                pending_prompt: None,
                waiting_since: None,
                work_item_ref: None,
                last_known_status: None,
            };

            // 5a. Reserve the concurrency slot atomically (FR-026): the active count and the
            // record that makes THIS session count are taken under a single config-lock, so
            // two concurrent starts cannot both pass the limit (the pre-check at step 2 is
            // only a fast-path rejection and races). No `.await` is held across the guard.
            {
                let cfg = self.config.lock().expect("poisoned");
                if let Some(limit) = cfg.concurrency_limit {
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
                self.store.upsert_session(&session)?;
            }
            created = Some(id);
            self.record_lifecycle(id, SessionStatus::Starting, None);

            // 6. Launch the agent.
            let handle = backend
                .start_agent(&env.id, &tool.invocation)
                .await
                .map_err(|e| CoreError::StartFailed(e.to_string()))?;
            Ok((id, handle))
        }
        .await;

        match launched {
            Ok((id, handle)) => {
                let running = transition(SessionStatus::Starting, Trigger::Started)?;
                self.store.set_session_started(id, clock::now())?;
                self.store
                    .set_session_state(id, running, None, None, None, None)?;
                self.runtime.lock().expect("poisoned").insert(
                    id,
                    RuntimeHandle {
                        backend_id: backend.id(),
                        environment_id: env.id,
                        zellij_session: handle.zellij_session,
                        limits: req.limits,
                    },
                );
                self.record_operator_action(id, daedalus_proto::OperatorAction::Start);
                self.record_lifecycle(id, running, None);
                Ok(id)
            }
            Err(e) => {
                // ANY post-acquire failure tears the environment down and removes the
                // transient record — no orphaned environment or session remains (C-B2).
                let _ = backend.teardown(&env.id).await;
                if let Some(id) = created {
                    let _ = self.store.delete_session(id);
                }
                let _ = self
                    .store
                    .set_environment_lifecycle(env.id, EnvLifecycle::Released);
                Err(e)
            }
        }
    }
}
