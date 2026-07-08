//! Integration test for intervening in a session's lifecycle (US4): stop, send-input, and
//! clean-up — and that other sessions are unaffected. Every key operator action also
//! lands on the session's persisted event timeline (FR-019a, T082).

use bytes::Bytes;
use daedalus_app::AppQuery;
use daedalus_proto::{EventKind, EventPayload, OperatorAction, SessionId, SessionStatus};
use daedalus_tests::Fixture;

fn operator_actions(fx: &Fixture, id: SessionId) -> Vec<OperatorAction> {
    fx.core
        .store()
        .list_events(id)
        .unwrap()
        .iter()
        .filter_map(|e| match &e.payload {
            EventPayload::OperatorAction { action } => {
                assert_eq!(e.kind, EventKind::OperatorAction);
                Some(*action)
            }
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn operator_actions_are_recorded_on_the_session_timeline() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");
    let id = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();

    fx.core
        .send_input(id, Bytes::from_static(b"y\n"))
        .await
        .unwrap();
    fx.core.stop_session(id).await.unwrap();
    fx.core.clean_up(id).await.unwrap();

    // start / stop / clean-up, in order (FR-019a). Input is deliberately NOT an
    // operator-action event: live typing would spam one entry per keystroke.
    assert_eq!(
        operator_actions(&fx, id),
        vec![
            OperatorAction::Start,
            OperatorAction::Stop,
            OperatorAction::CleanUp,
        ]
    );

    // Confirm-completion is recorded too.
    let req = fx.fresh_request(tool);
    fx.write_tasks(&req.objective, "- [ ] T001 Implement\n");
    let confirm = fx.core.start_session(req).await.unwrap();
    fx.core
        .apply_signal(confirm, daedalus_core::AgentSignal::ExitedCleanly)
        .await
        .unwrap();
    fx.core.confirm_completion(confirm).await.unwrap();
    assert!(operator_actions(&fx, confirm).contains(&OperatorAction::ConfirmCompletion));
}

#[tokio::test]
async fn stop_send_input_and_cleanup_are_isolated() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");
    let s1 = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
    let s2 = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
    assert_eq!(fx.backend.live_environment_count(), 2);

    // Send-input is delivered and reflected.
    fx.core
        .send_input(s1, Bytes::from_static(b"yes\n"))
        .await
        .unwrap();
    let delivered = fx.core.delivered_input(s1);
    assert_eq!(delivered, vec![Bytes::from_static(b"yes\n")]);

    // Stop halts s1 and records stopped-by-operator; s2 keeps running.
    fx.core.stop_session(s1).await.unwrap();
    let d1 = fx.app.session(s1).unwrap();
    assert_eq!(d1.session.status, SessionStatus::Stopped);
    assert_eq!(
        d1.session.terminal_outcome,
        Some(daedalus_proto::Outcome::reason("stopped by operator"))
    );
    assert_eq!(
        fx.app.session(s2).unwrap().session.status,
        SessionStatus::Running
    );

    // Clean-up frees s1's environment without affecting s2.
    let s2_env = fx.app.session(s2).unwrap().environment.id;
    fx.core.clean_up(s1).await.unwrap();
    assert_eq!(fx.backend.live_environment_count(), 1, "s1 env released");
    assert!(fx.backend.agent_running(&s2_env), "s2 keeps running");
}

#[tokio::test]
async fn send_input_reaches_the_attached_terminal_channel() {
    // FR-023: operator input is delivered INTO the live attach (the PTY bridge for real
    // backends), not just recorded — the regression behind "typed input never appears
    // in the terminal" (2026-07-08).
    use bytes::Bytes;

    let fx = daedalus_tests::Fixture::new();
    let tool = fx.register_sample_tool("claude");
    let req = fx.fresh_request(tool);
    let objective = req.objective.clone();
    fx.write_tasks(&objective, "- [ ] T001 Work\n");
    let id = fx.core.start_session(req).await.unwrap();

    fx.terminal
        .seed(id, vec![Bytes::from_static(b"agent output\n")]);
    let _channel = fx.core.attach_and_capture(id).await.unwrap();

    fx.core
        .send_input(id, Bytes::from_static(b"yes\n"))
        .await
        .unwrap();
    // The attached channel's sink received the bytes (async delivery: allow a beat).
    for _ in 0..50 {
        if !fx.terminal.input_received(id).is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(
        fx.terminal.input_received(id),
        vec![Bytes::from_static(b"yes\n")]
    );
}
