//! Integration test for intervening in a session's lifecycle (US4): stop, send-input, and
//! clean-up — and that other sessions are unaffected.

use bytes::Bytes;
use daedalus_app::AppQuery;
use daedalus_proto::SessionStatus;
use daedalus_tests::Fixture;

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
        d1.session.terminal_outcome.as_deref(),
        Some("stopped by operator")
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
