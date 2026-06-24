//! Integration test: the operator is notified when a session reaches a terminal/abnormal
//! state (FR-021, T036).

use daedalus_core::AgentSignal;
use daedalus_proto::{AppEvent, SessionStatus};
use daedalus_tests::Fixture;

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
