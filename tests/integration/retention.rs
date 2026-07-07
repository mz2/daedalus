//! Integration test for retain-until-deleted records + `DeleteRecord` (FR-030a): ended
//! sessions are kept until the operator deletes them; there is no auto-expiry.

use daedalus_app::AppQuery;
use daedalus_proto::SessionStatus;
use daedalus_tests::Fixture;

#[tokio::test]
async fn ended_sessions_are_retained_until_deleted() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");
    let id = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();

    // End the session; its record is retained (still listed).
    fx.core.stop_session(id).await.unwrap();
    assert_eq!(
        fx.app.session(id).unwrap().session.status,
        SessionStatus::Stopped
    );
    assert!(
        fx.app.fleet().iter().any(|s| s.id == id),
        "retained after ending"
    );

    // Operator deletes it explicitly ⇒ now gone.
    fx.core.delete_record(id).await.unwrap();
    assert!(fx.app.fleet().iter().all(|s| s.id != id));
    assert!(fx.app.session(id).is_none());
}
