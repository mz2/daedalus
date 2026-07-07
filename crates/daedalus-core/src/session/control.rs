//! Session control: stop, send-input, confirm-completion, and clean-up (FR-022/023/024,
//! FR-015a). No pause/resume — out of scope for v1.

use bytes::Bytes;

use daedalus_proto::{EnvLifecycle, OperatorAction, Outcome, SessionId};

use crate::session::state::{transition, Trigger};
use crate::{clock, map_not_found, Core, CoreError};

impl Core {
    /// Stop a running session: halt the agent, release nothing yet (clean-up does that),
    /// and record it as stopped-by-operator (FR-022). Idempotent.
    pub async fn stop_session(&self, id: SessionId) -> Result<(), CoreError> {
        self.stop_with_reason(id, "stopped by operator", true).await
    }

    /// The shared stop tail (FR-022, and the resource-limit stop policy): halt the agent,
    /// record the session as stopped with the stated reason, and — for an operator's stop
    /// — record the operator action on the timeline. Idempotent.
    pub(crate) async fn stop_with_reason(
        &self,
        id: SessionId,
        reason: &str,
        operator_action: bool,
    ) -> Result<(), CoreError> {
        let session = self.store.get_session(id).map_err(map_not_found)?;
        if session.status.is_terminal() {
            return Ok(()); // idempotent — already stopped/completed/failed
        }

        if let Some((backend_id, env, _)) = self.runtime_lookup(id) {
            if let Some(backend) = self.backends.by_id(backend_id) {
                let _ = backend.stop(&env).await; // idempotent at the backend too (C-B5)
            }
        }

        let next = transition(session.status, Trigger::OperatorStopped)?;
        // Stopping from a waiting state clears its pending prompt + anchor (S2).
        let outcome = Outcome::reason(reason);
        self.store
            .set_session_state(id, next, Some(clock::now()), Some(&outcome), None, None)?;
        if operator_action {
            self.record_operator_action(id, OperatorAction::Stop);
        }
        self.record_lifecycle(id, next, Some(reason.to_string()));
        Ok(())
    }

    /// Send input to the agent, if the tool accepts it (FR-023). Rejected with a clear
    /// reason otherwise (C-A2).
    pub async fn send_input(&self, id: SessionId, data: Bytes) -> Result<(), CoreError> {
        let session = self.store.get_session(id).map_err(map_not_found)?;
        if !session.accepts_input {
            return Err(CoreError::InputNotAccepted);
        }

        // Record delivery so the surface (and tests) can observe it was accepted/reflected.
        self.delivered_input
            .lock()
            .expect("poisoned")
            .entry(id)
            .or_default()
            .push(data.clone());

        // Best-effort: write the keystrokes into the live zellij session.
        if let Some((_, _, zellij_session)) = self.runtime_lookup(id) {
            if let Ok(text) = std::str::from_utf8(&data) {
                let _ = tokio::process::Command::new("zellij")
                    .arg("--session")
                    .arg(&zellij_session)
                    .arg("action")
                    .arg("write-chars")
                    .arg(text)
                    .output()
                    .await;
            }
        }

        self.record_operator_action(id, OperatorAction::InputSent);

        // Answering a waiting session returns it to Running and clears the pending
        // prompt + waiting anchor (FR-015b).
        if session.status == daedalus_proto::SessionStatus::WaitingForInput {
            self.clear_waiting_for_input(id)?;
        }
        Ok(())
    }

    /// Input chunks delivered to a session so far (observability / tests).
    #[must_use]
    pub fn delivered_input(&self, id: SessionId) -> Vec<Bytes> {
        self.delivered_input
            .lock()
            .expect("poisoned")
            .get(&id)
            .cloned()
            .unwrap_or_default()
    }

    /// Confirm completion of a session awaiting operator confirmation
    /// (`AwaitingConfirmation → Completed`, FR-015a).
    pub async fn confirm_completion(&self, id: SessionId) -> Result<(), CoreError> {
        let session = self.store.get_session(id).map_err(map_not_found)?;
        let next = transition(session.status, Trigger::OperatorConfirmed)?;
        // Preserve the AwaitingConfirmation outcome (exit summary) into Completed; clear the
        // waiting anchor.
        self.store
            .set_session_state(id, next, Some(clock::now()), None, None, None)?;
        self.record_operator_action(id, OperatorAction::ConfirmCompletion);
        self.record_lifecycle(id, next, None);
        Ok(())
    }

    /// Clean up a session: stop it if running, tear down its environment, and free its
    /// resources — without affecting other sessions (FR-024, C-B5).
    pub async fn clean_up(&self, id: SessionId) -> Result<(), CoreError> {
        let session = self.store.get_session(id).map_err(map_not_found)?;
        if !session.status.is_terminal() {
            let _ = self.stop_session(id).await;
        }
        self.record_operator_action(id, OperatorAction::CleanUp);
        let handle = self.runtime.lock().expect("poisoned").remove(&id);
        if let Some(handle) = handle {
            if let Some(backend) = self.backends.by_id(handle.backend_id) {
                let _ = backend.teardown(&handle.environment_id).await;
            }
            let _ = self
                .store
                .set_environment_lifecycle(handle.environment_id, EnvLifecycle::Released);
        }
        Ok(())
    }

    /// Look up the backend id, environment id, and zellij session for a live session.
    pub(crate) fn runtime_lookup(
        &self,
        id: SessionId,
    ) -> Option<(
        daedalus_proto::BackendId,
        daedalus_proto::EnvironmentId,
        String,
    )> {
        self.runtime
            .lock()
            .expect("poisoned")
            .get(&id)
            .map(|h| (h.backend_id, h.environment_id, h.zellij_session.clone()))
    }
}
