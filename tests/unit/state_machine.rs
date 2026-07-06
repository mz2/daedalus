//! Unit tests for the session lifecycle state machine (T012, FR-015a; T069, US6:
//! `WaitingForInput` + `Unknown` — FR-015b/020).

use daedalus_core::{transition, AgentSignal, Trigger};
use daedalus_proto::SessionStatus::*;
use daedalus_tests::Fixture;

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

#[test]
fn running_and_waiting_for_input_are_mutually_reachable() {
    // Running ⇄ WaitingForInput (FR-015b): the agent asks, the operator answers.
    let waiting = transition(Running, Trigger::InputRequested).unwrap();
    assert_eq!(waiting, WaitingForInput);
    // A non-terminal attention state the operator resolves.
    assert!(!waiting.is_terminal());
    assert!(waiting.is_attention());
    assert_eq!(
        transition(waiting, Trigger::InputProvided).unwrap(),
        Running
    );
}

#[test]
fn waiting_for_input_is_never_reported_stalled() {
    // Distinct from Stalled — the stall trigger is illegal while waiting (FR-015b).
    assert!(transition(WaitingForInput, Trigger::StallDetected).is_err());
}

#[test]
fn contact_loss_derives_unknown_from_any_live_state() {
    for live in [
        Starting,
        Running,
        WaitingForInput,
        Stalled,
        AwaitingConfirmation,
    ] {
        assert_eq!(transition(live, Trigger::ContactLost).unwrap(), Unknown);
    }
    assert!(!Unknown.is_terminal());
    assert!(Unknown.is_attention());
    for terminal in [Completed, Failed, Stopped] {
        assert!(transition(terminal, Trigger::ContactLost).is_err());
    }
}

#[tokio::test]
async fn stall_detection_skips_a_session_waiting_for_input() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");
    let id = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
    fx.core
        .mark_waiting_for_input(id, "Continue? (y/n)")
        .unwrap();

    // Even a zero stall interval must not mark a waiting session stalled (FR-015b).
    fx.core.set_stall_interval(0);
    let status = fx
        .core
        .apply_signal(id, AgentSignal::NoProgress)
        .await
        .unwrap();
    assert_eq!(status, WaitingForInput);
    assert_eq!(
        fx.core.session_detail(id).unwrap().session.status,
        WaitingForInput
    );
}

#[tokio::test]
async fn connection_loss_preserves_last_known_state_and_rederives_on_reconcile() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");
    let id = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();

    // Contact lost ⇒ Unknown, with the last-known state preserved (FR-020).
    fx.core.mark_connection_lost(id).unwrap();
    let session = fx.core.session_detail(id).unwrap().session;
    assert_eq!(session.status, Unknown);
    assert_eq!(session.last_known_status, Some(Running));

    // The environment is still reachable here, so reconciliation re-derives the status.
    let corrected = fx.core.reconcile().await.unwrap();
    assert!(corrected >= 1);
    let session = fx.core.session_detail(id).unwrap().session;
    assert_eq!(session.status, Running);
    assert_eq!(session.last_known_status, None);
}
