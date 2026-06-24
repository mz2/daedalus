//! Integration test for the fleet view (US5): lists every session with
//! tool/objective/backend/origin/status; enforces the concurrency limit (FR-026); and marks
//! a backend unavailable mid-flight while other sessions keep working.

use daedalus_app::AppQuery;
use daedalus_core::CoreConfig;
use daedalus_proto::{Availability, BackendKind, Origin, SessionStatus};
use daedalus_tests::Fixture;

#[tokio::test]
async fn fleet_lists_every_session_with_its_attributes() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");
    let a = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
    let _b = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();

    let fleet = fx.app.fleet();
    assert_eq!(fleet.len(), 2);
    let row = fleet.iter().find(|s| s.id == a).unwrap();
    assert_eq!(row.tool_name, "claude");
    assert_eq!(row.objective, "sample objective");
    assert_eq!(row.backend, BackendKind::Fake);
    assert_eq!(row.origin, Origin::Fresh);
    assert_eq!(row.status, SessionStatus::Running);
}

#[tokio::test]
async fn concurrency_limit_is_enforced_with_a_clear_reason() {
    let fx = Fixture::with_config(CoreConfig {
        concurrency_limit: Some(2),
        ..CoreConfig::default()
    });
    let tool = fx.register_sample_tool("claude");
    fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
    fx.core.start_session(fx.fresh_request(tool)).await.unwrap();

    let err = fx
        .core
        .start_session(fx.fresh_request(tool))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("concurrency limit"));
    assert_eq!(fx.app.fleet().len(), 2);
}

#[tokio::test]
async fn backend_unavailable_mid_session_is_marked_but_sessions_remain() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");
    let id = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();

    // The backend goes down after the session is running.
    fx.backend.set_availability(Availability::Unavailable);

    let statuses = fx.core.backends_status().await;
    assert!(statuses
        .iter()
        .all(|s| s.availability == Availability::Unavailable));
    // The already-running session is still listed (history not lost).
    assert!(fx.app.fleet().iter().any(|s| s.id == id));
}
