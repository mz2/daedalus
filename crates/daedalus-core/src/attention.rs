//! The "Needs you" attention queue (US6, FR-021a/b): derive an [`AttentionItem`] for every
//! session blocked on the operator, ordered most-answerable-first. Derived from live
//! session state on every query — never persisted (data-model `AttentionItem`).

use std::time::Duration;

use daedalus_proto::{
    AttentionItem, AttentionKind, CostIndication, EnvLifecycle, EventPayload, Session, Timestamp,
};

use crate::{clock, Core, CoreError};

impl Core {
    /// Every session blocked on the operator, most answerable first; within a kind the
    /// newest-waiting item lists last (FR-021a). An empty result means nothing needs you.
    pub fn needs_you(&self) -> Result<Vec<AttentionItem>, CoreError> {
        let now = clock::now();
        let mut items = Vec::new();
        for session in self.store.list_sessions()? {
            let Some(kind) = AttentionKind::from_status(session.status) else {
                continue;
            };
            let waiting = self.waiting_duration(&session, now);
            let cue = cue_for(&session, kind);
            let cost = self.cost_for(&session, kind, waiting);
            items.push(AttentionItem {
                session_id: session.id,
                kind,
                cue,
                waiting,
                cost,
            });
        }
        items.sort_by(|a, b| {
            a.kind
                .rank()
                .cmp(&b.kind.rank())
                .then(b.waiting.cmp(&a.waiting))
        });
        Ok(items)
    }

    /// How long the session has been blocked: from `waiting_since` when set
    /// (input/confirmation), otherwise from when it entered its current state (FR-021a).
    fn waiting_duration(&self, session: &Session, now: Timestamp) -> Duration {
        let entered = session
            .waiting_since
            .or_else(|| self.state_entered_at(session))
            .or(session.ended_at)
            .or(session.started_at)
            .unwrap_or(session.created_at);
        Duration::from_millis((now.millis() - entered.millis()).max(0) as u64)
    }

    /// When the session last entered its current status, per the lifecycle event history.
    fn state_entered_at(&self, session: &Session) -> Option<Timestamp> {
        let events = self.store.list_events(session.id).ok()?;
        events
            .iter()
            .rev()
            .find(|e| {
                matches!(
                    &e.payload,
                    EventPayload::Lifecycle { status, .. } if *status == session.status
                )
            })
            .map(|e| e.timestamp)
    }

    /// The waiting-cost indication (FR-021b): an idle estimate for a live blocked
    /// environment with a configured backend idle rate; env-held for an ended session
    /// still holding its environment; nothing when no rate is configured.
    fn cost_for(
        &self,
        session: &Session,
        kind: AttentionKind,
        waiting: Duration,
    ) -> CostIndication {
        match kind {
            AttentionKind::AwaitingConfirmation => {
                // The agent has exited; the environment is held until confirm/clean-up.
                match self.store.get_environment(session.environment_id) {
                    Ok(env)
                        if !matches!(
                            env.lifecycle,
                            EnvLifecycle::Released | EnvLifecycle::Releasing
                        ) =>
                    {
                        CostIndication::EnvHeld
                    }
                    _ => CostIndication::None,
                }
            }
            AttentionKind::WaitingForInput | AttentionKind::Stalled => {
                // Still holding a live environment: accrue idle rate × waiting time.
                let rate = self
                    .store
                    .get_environment(session.environment_id)
                    .ok()
                    .and_then(|env| self.backends.idle_rate_by_id(env.backend_id));
                match rate {
                    Some(rate) => CostIndication::Idle(rate * waiting.as_secs_f64() / 3600.0),
                    None => CostIndication::None,
                }
            }
            AttentionKind::Disconnected | AttentionKind::Failed => CostIndication::None,
        }
    }
}

/// The reason line shown for an item (SC-015): the pending question for input, the exit
/// summary for confirmation, the stall/fail/drop reason otherwise.
fn cue_for(session: &Session, kind: AttentionKind) -> String {
    let outcome = session.terminal_outcome.as_ref();
    match kind {
        AttentionKind::WaitingForInput => session
            .pending_prompt
            .clone()
            .unwrap_or_else(|| "asked a question".to_string()),
        AttentionKind::AwaitingConfirmation => outcome
            .and_then(|o| o.exit_summary.clone())
            .unwrap_or_else(|| "run ended — confirm completion".to_string()),
        AttentionKind::Stalled => outcome
            .and_then(|o| o.reason.clone())
            .unwrap_or_else(|| "no output — may be stuck".to_string()),
        AttentionKind::Failed => outcome
            .and_then(|o| o.reason.clone())
            .unwrap_or_else(|| "run failed — needs a call".to_string()),
        AttentionKind::Disconnected => "connection lost — last-known state preserved".to_string(),
    }
}
