//! The session lifecycle state machine (`data-model.md`).
//!
//! `Starting → Running → {Completed | Failed | Stalled | Stopped | AwaitingConfirmation}`.
//! Completion is reached **only** when all tracked tasks are done or the operator confirms
//! — never on agent exit alone (FR-015a, SC-004).

use thiserror::Error;

use daedalus_proto::SessionStatus;

/// Events that drive a session's status forward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// Environment provisioned and the agent launched: `Starting → Running`.
    Started,
    /// Every tracked task reported done: `Running → Completed` (FR-015a).
    AllTasksDone,
    /// The agent exited cleanly with tracked tasks unfinished:
    /// `Running → AwaitingConfirmation` (FR-015a).
    AgentExitedWithUnfinishedTasks,
    /// The agent crashed / exited abnormally: `→ Failed` (FR-005).
    AgentCrashed,
    /// The environment could not be provisioned: `Starting → Failed` (FR-005).
    ProvisionFailed,
    /// No output/progress for the configured stall interval: `Running → Stalled`.
    StallDetected,
    /// Output/progress resumed: `Stalled → Running`.
    ProgressResumed,
    /// Operator stopped the session: `→ Stopped` (FR-022).
    OperatorStopped,
    /// Operator confirmed completion: `AwaitingConfirmation → Completed` (FR-015a).
    OperatorConfirmed,
}

/// An illegal transition was attempted.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("illegal transition: {trigger:?} is not valid from {from:?}")]
pub struct TransitionError {
    /// The state the session was in.
    pub from: SessionStatus,
    /// The trigger that was rejected.
    pub trigger: Trigger,
}

/// Compute the next status for `current` given `trigger`, or reject the transition.
///
/// Terminal states (`Completed`, `Failed`, `Stopped`) accept no further triggers.
pub fn transition(
    current: SessionStatus,
    trigger: Trigger,
) -> Result<SessionStatus, TransitionError> {
    use SessionStatus::*;
    use Trigger::*;

    let next = match (current, trigger) {
        // From Starting.
        (Starting, Started) => Running,
        (Starting, ProvisionFailed) => Failed,
        (Starting, AgentCrashed) => Failed,
        (Starting, OperatorStopped) => Stopped,

        // From Running.
        (Running, AllTasksDone) => Completed,
        (Running, AgentExitedWithUnfinishedTasks) => AwaitingConfirmation,
        (Running, AgentCrashed) => Failed,
        (Running, StallDetected) => Stalled,
        (Running, OperatorStopped) => Stopped,

        // From Stalled (a non-terminal attention state).
        (Stalled, ProgressResumed) => Running,
        (Stalled, AllTasksDone) => Completed,
        (Stalled, AgentExitedWithUnfinishedTasks) => AwaitingConfirmation,
        (Stalled, AgentCrashed) => Failed,
        (Stalled, OperatorStopped) => Stopped,

        // From AwaitingConfirmation (a non-terminal attention state).
        (AwaitingConfirmation, OperatorConfirmed) => Completed,
        (AwaitingConfirmation, OperatorStopped) => Stopped,
        (AwaitingConfirmation, AllTasksDone) => Completed,

        // Anything else (incl. all triggers from terminal states) is illegal.
        (from, trigger) => return Err(TransitionError { from, trigger }),
    };

    Ok(next)
}

/// Whether `trigger` is legal from `current` without computing the target.
#[must_use]
pub fn can_transition(current: SessionStatus, trigger: Trigger) -> bool {
    transition(current, trigger).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use SessionStatus::*;
    use Trigger::*;

    #[test]
    fn happy_path_to_completed() {
        let s = transition(Starting, Started).unwrap();
        assert_eq!(s, Running);
        assert_eq!(transition(s, AllTasksDone).unwrap(), Completed);
    }

    #[test]
    fn clean_exit_with_unfinished_tasks_awaits_confirmation() {
        assert_eq!(
            transition(Running, AgentExitedWithUnfinishedTasks).unwrap(),
            AwaitingConfirmation
        );
        assert_eq!(
            transition(AwaitingConfirmation, OperatorConfirmed).unwrap(),
            Completed
        );
    }

    #[test]
    fn terminal_states_are_frozen() {
        for terminal in [Completed, Failed, Stopped] {
            assert!(transition(terminal, OperatorStopped).is_err());
            assert!(transition(terminal, AllTasksDone).is_err());
        }
    }

    #[test]
    fn agent_exit_alone_never_completes() {
        // Only AllTasksDone or OperatorConfirmed reach Completed.
        assert_ne!(
            transition(Running, AgentExitedWithUnfinishedTasks).unwrap(),
            Completed
        );
    }
}
