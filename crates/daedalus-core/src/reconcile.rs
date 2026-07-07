//! Startup reconciliation: re-derive each non-terminal session's status from backend +
//! zellij liveness after a Daedalus restart, with no history lost (FR-029/030, SC-007).
//!
//! Loss of contact with an environment ⇒ `Unknown`, with the last-known state preserved
//! and never shown as healthy (FR-020); a reconnect re-derives the preserved state.

use daedalus_proto::SessionStatus;

use crate::session::state::{transition, Trigger};
use crate::{map_not_found, Core, CoreError};

impl Core {
    /// Mark a session's environment as out of contact: the status becomes `Unknown` and
    /// the last-known state is preserved on the record (FR-020). Idempotent for a session
    /// that is already `Unknown`.
    pub fn mark_connection_lost(&self, id: daedalus_proto::SessionId) -> Result<(), CoreError> {
        let session = self.store.get_session(id).map_err(map_not_found)?;
        if session.status == SessionStatus::Unknown {
            return Ok(());
        }
        let next = transition(session.status, Trigger::ContactLost)?;
        self.store
            .set_session_last_known(id, Some(session.status))?;
        self.store.set_session_status(id, next, None, None)?;
        self.record_lifecycle(id, next, Some("connection lost".to_string()));
        Ok(())
    }

    /// Reconcile all non-terminal sessions. Returns the number whose status was corrected.
    pub async fn reconcile(&self) -> Result<usize, CoreError> {
        let sessions = self.store.list_sessions()?;
        let mut corrected = 0;

        for session in sessions {
            if session.status.is_terminal() {
                continue;
            }
            // A live in-process runtime handle means we still own the session.
            let owned = self
                .runtime
                .lock()
                .expect("poisoned")
                .contains_key(&session.id);

            // Try to confirm the environment is still reachable via its backend.
            let reachable = owned
                || match self.store.get_environment(session.environment_id) {
                    Ok(env) => match self.backends.by_id(env.backend_id) {
                        Some(backend) => {
                            backend.availability().await
                                != daedalus_proto::Availability::Unavailable
                                && backend.resource_usage(&env.id).await.is_ok()
                        }
                        None => false,
                    },
                    Err(_) => false,
                };

            if session.status == SessionStatus::Unknown {
                // Contact restored: re-derive the preserved last-known state (FR-020/030).
                if reachable {
                    let restored = session.last_known_status.unwrap_or(SessionStatus::Running);
                    self.store.set_session_last_known(session.id, None)?;
                    self.store
                        .set_session_status(session.id, restored, None, None)?;
                    self.record_lifecycle(
                        session.id,
                        restored,
                        Some("contact restored — status re-derived".to_string()),
                    );
                    corrected += 1;
                }
                continue;
            }

            if reachable {
                continue;
            }

            // Contact lost: surface it as Unknown, preserving the last-known state —
            // never shown as healthy (FR-020).
            self.mark_connection_lost(session.id)?;
            corrected += 1;
        }

        Ok(corrected)
    }
}
