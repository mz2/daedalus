//! Integration test: the operator is notified when a session reaches a terminal/abnormal
//! state (FR-021, T036), including the blocked-on-operator states (waiting-for-input with
//! the question, awaiting-confirmation with the exit summary — T072, US6). Every
//! notification identifies the session so activation can jump to it.

use bytes::Bytes;
use daedalus_core::AgentSignal;
use daedalus_proto::{AppEvent, SessionId, SessionStatus, TerminalOrAbnormal};
use daedalus_tests::Fixture;

fn notifications(rx: &mut tokio::sync::broadcast::Receiver<AppEvent>) -> Vec<TerminalOrAbnormal> {
    std::iter::from_fn(|| rx.try_recv().ok())
        .filter_map(|e| match e {
            AppEvent::Notification(n) => Some(n),
            _ => None,
        })
        .collect()
}

/// Stream output chunks through the capture pipeline so an exit summary exists.
async fn stream_output(fx: &Fixture, id: SessionId, chunks: Vec<&'static [u8]>) {
    let n = chunks.len();
    fx.terminal
        .seed(id, chunks.into_iter().map(Bytes::from_static).collect());
    let mut channel = fx.core.attach_and_capture(id).await.unwrap();
    for _ in 0..n {
        channel.output.recv().await.unwrap();
    }
}

#[tokio::test]
async fn terminal_and_abnormal_states_notify() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");
    let id = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();

    let mut rx = fx.app.subscribe();
    // A crash is an abnormal, terminal state.
    fx.core
        .apply_signal(id, AgentSignal::Crashed)
        .await
        .unwrap();

    let mut saw_notification = false;
    while let Ok(ev) = rx.try_recv() {
        if let AppEvent::Notification(n) = ev {
            assert_eq!(n.session, id);
            assert_eq!(n.status, SessionStatus::Failed);
            saw_notification = true;
        }
    }
    assert!(
        saw_notification,
        "a terminal/abnormal state raises a notification"
    );
}

#[tokio::test]
async fn stall_is_an_attention_notification() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");
    let id = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();

    let mut rx = fx.app.subscribe();
    fx.core
        .apply_signal(id, AgentSignal::NoProgress)
        .await
        .unwrap();

    let notified = std::iter::from_fn(|| rx.try_recv().ok())
        .any(|e| matches!(e, AppEvent::Notification(n) if n.status == SessionStatus::Stalled));
    assert!(notified);
}

#[tokio::test]
async fn waiting_for_input_notifies_with_the_pending_question() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");
    let id = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();

    let mut rx = fx.app.subscribe();
    fx.core
        .mark_waiting_for_input(id, "Which database should the service use?")
        .unwrap();

    let ns = notifications(&mut rx);
    let n = ns
        .iter()
        .find(|n| n.status == SessionStatus::WaitingForInput)
        .expect("a blocked-on-operator session raises a notification (FR-021)");
    assert_eq!(n.session, id, "the notification jumps to the session");
    assert_eq!(
        n.note.as_deref(),
        Some("Which database should the service use?"),
        "the notification carries the agent's question"
    );
}

#[tokio::test]
async fn awaiting_confirmation_notifies_with_the_exit_summary() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");
    let id = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
    stream_output(&fx, id, vec![b"2 of 4 tracked tasks done\n"]).await;

    let mut rx = fx.app.subscribe();
    // Clean exit with unfinished tasks ⇒ AwaitingConfirmation (FR-015a).
    let status = fx
        .core
        .apply_signal(id, AgentSignal::ExitedCleanly)
        .await
        .unwrap();
    assert_eq!(status, SessionStatus::AwaitingConfirmation);

    let ns = notifications(&mut rx);
    let n = ns
        .iter()
        .find(|n| n.status == SessionStatus::AwaitingConfirmation)
        .expect("awaiting-confirmation is blocked on the operator and notifies (FR-021)");
    assert_eq!(n.session, id, "the notification jumps to the session");
    assert_eq!(
        n.note.as_deref(),
        Some("2 of 4 tracked tasks done"),
        "the notification carries the agent's exit summary"
    );
}

#[tokio::test]
async fn every_notification_identifies_the_session_to_jump_to() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");

    // One session per notifying transition, terminal and blocked alike.
    let mut expected = Vec::new();
    for signal in [
        AgentSignal::Crashed,
        AgentSignal::NoProgress,
        AgentSignal::ExitedCleanly,
    ] {
        let id = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
        let mut rx = fx.app.subscribe();
        fx.core.apply_signal(id, signal).await.unwrap();
        expected.push((id, notifications(&mut rx)));
    }
    let id = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
    let mut rx = fx.app.subscribe();
    fx.core.mark_waiting_for_input(id, "Continue?").unwrap();
    expected.push((id, notifications(&mut rx)));

    for (id, ns) in expected {
        assert!(!ns.is_empty(), "each transition raises a notification");
        for n in ns {
            assert_eq!(
                n.session, id,
                "every notification carries the session id for jump-to-session"
            );
        }
    }
}
