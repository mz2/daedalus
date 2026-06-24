//! Startup reconciliation: re-derive each non-terminal session's status from backend +
//! zellij liveness after a Daedalus restart, with no history lost (FR-029/030, SC-007).
//!
//! Loss of contact with an environment is surfaced (the session becomes `Stalled`, or
//! `Failed` if it never reached `Running`) and the last-known record is preserved (FR-020).

use daedalus_proto::SessionStatus;

use crate::session::state::{transition, Trigger};
use crate::{clock, Core, CoreError};

impl Core {
    /// Reconcile all non-terminal sessions. Returns the number whose status was corrected.
    pub async fn reconcile(&self) -> Result<usize, CoreError> {
        let sessions = self.store.list_sessions()?;
        let mut corrected = 0;

        for session in sessions {
            if session.status.is_terminal() {
                continue;
            }
            // A live in-process runtime handle means we still own the session — leave it.
            if self
                .runtime
                .lock()
                .expect("poisoned")
                .contains_key(&session.id)
            {
                continue;
            }

            // Try to confirm the environment is still reachable via its backend.
            let reachable = match self.store.get_environment(session.environment_id) {
                Ok(env) => match self.backends.by_id(env.backend_id) {
                    Some(backend) => {
                        backend.availability().await != daedalus_proto::Availability::Unavailable
                            && backend.resource_usage(&env.id).await.is_ok()
                    }
                    None => false,
                },
                Err(_) => false,
            };
            if reachable {
                continue;
            }

            // Contact lost: prefer Stalled (attention, recoverable); fall back to Failed for
            // sessions that never reached Running.
            let note = "contact lost — reconciled after restart".to_string();
            let next = transition(session.status, Trigger::StallDetected)
                .or_else(|_| transition(session.status, Trigger::AgentCrashed));
            if let Ok(next) = next {
                let ended = next.is_terminal().then(clock::now);
                self.store
                    .set_session_status(session.id, next, ended, Some(&note))?;
                self.record_lifecycle(session.id, next, Some(note));
                corrected += 1;
            } else if session.status == SessionStatus::Starting {
                // Defensive: force-fail a stuck Starting session.
                self.store.set_session_status(
                    session.id,
                    SessionStatus::Failed,
                    Some(clock::now()),
                    Some(&note),
                )?;
                self.record_lifecycle(session.id, SessionStatus::Failed, Some(note));
                corrected += 1;
            }
        }

        Ok(corrected)
    }
}
