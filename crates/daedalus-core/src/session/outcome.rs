//! Terminal-state, stall, and failure detection, including the `AwaitingConfirmation` path
//! (FR-015/015a/020). The core never marks a session `Completed` on agent exit alone —
//! that requires all tracked tasks done or an explicit confirmation.

use daedalus_proto::{SessionId, SessionStatus};

use crate::session::state::{transition, Trigger};
use crate::{clock, map_not_found, Core, CoreError};

/// A signal observed about a running agent that may move its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentSignal {
    /// The agent process exited cleanly (zero exit). The resulting state depends on whether
    /// all tracked tasks are done (`Completed`) or not (`AwaitingConfirmation`, FR-015a).
    ExitedCleanly,
    /// The agent crashed / exited abnormally ⇒ `Failed` (FR-005).
    Crashed,
    /// No output/progress for the configured stall interval ⇒ `Stalled` (FR-020).
    NoProgress,
    /// Output/progress resumed ⇒ `Stalled → Running`.
    Progress,
}

impl Core {
    /// Apply an observed [`AgentSignal`] to a session, advancing the state machine and
    /// persisting the new status (with an `ended_at` for terminal states).
    pub async fn apply_signal(
        &self,
        id: SessionId,
        signal: AgentSignal,
    ) -> Result<SessionStatus, CoreError> {
        let session = self.store.get_session(id).map_err(map_not_found)?;

        let trigger = match signal {
            AgentSignal::Crashed => Trigger::AgentCrashed,
            AgentSignal::NoProgress => Trigger::StallDetected,
            AgentSignal::Progress => Trigger::ProgressResumed,
            AgentSignal::ExitedCleanly => {
                let tasks = self.store.list_tasks(id)?;
                let all_done = !tasks.is_empty()
                    && tasks
                        .iter()
                        .all(|t| t.status == daedalus_proto::TaskStatus::Done);
                if all_done {
                    Trigger::AllTasksDone
                } else {
                    Trigger::AgentExitedWithUnfinishedTasks
                }
            }
        };

        let next = transition(session.status, trigger)
            .map_err(|e| CoreError::IllegalTransition(e.to_string()))?;

        let note = match signal {
            AgentSignal::Crashed => Some("agent crashed or exited abnormally".to_string()),
            AgentSignal::NoProgress => Some(format!(
                "no progress for {}s",
                self.config.stall_interval_secs
            )),
            _ => None,
        };
        let ended = next.is_terminal().then(clock::now);
        self.store
            .set_session_status(id, next, ended, note.as_deref())?;
        self.record_lifecycle(id, next, note);
        Ok(next)
    }
}
