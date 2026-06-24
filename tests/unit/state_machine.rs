//! Unit tests for the session lifecycle state machine (T012, FR-015a).

use daedalus_core::{transition, Trigger};
use daedalus_proto::SessionStatus::*;

#[test]
fn starting_runs_then_completes_only_when_all_tasks_done() {
    let running = transition(Starting, Trigger::Started).unwrap();
    assert_eq!(running, Running);
    assert_eq!(
        transition(running, Trigger::AllTasksDone).unwrap(),
        Completed
    );
}

#[test]
fn clean_exit_with_unfinished_tasks_awaits_confirmation_then_confirms() {
    let awaiting = transition(Running, Trigger::AgentExitedWithUnfinishedTasks).unwrap();
    assert_eq!(awaiting, AwaitingConfirmation);
    assert_eq!(
        transition(awaiting, Trigger::OperatorConfirmed).unwrap(),
        Completed
    );
}

#[test]
fn provision_failure_and_crash_lead_to_failed() {
    assert_eq!(
        transition(Starting, Trigger::ProvisionFailed).unwrap(),
        Failed
    );
    assert_eq!(transition(Running, Trigger::AgentCrashed).unwrap(), Failed);
}

#[test]
fn stall_is_recoverable() {
    let stalled = transition(Running, Trigger::StallDetected).unwrap();
    assert_eq!(stalled, Stalled);
    assert_eq!(
        transition(stalled, Trigger::ProgressResumed).unwrap(),
        Running
    );
}

#[test]
fn terminal_states_reject_all_triggers() {
    for terminal in [Completed, Failed, Stopped] {
        for trigger in [
            Trigger::Started,
            Trigger::AllTasksDone,
            Trigger::OperatorStopped,
            Trigger::OperatorConfirmed,
        ] {
            assert!(transition(terminal, trigger).is_err());
        }
    }
}
