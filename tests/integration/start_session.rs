//! Integration tests for starting a session (AS1, AS2, AS4; C-B1, C-B2).

use daedalus_app::AppQuery;
use daedalus_proto::{Origin, SessionStatus, StartSessionRequest, WorktreeRef};
use daedalus_tests::Fixture;

#[tokio::test]
async fn as1_fresh_session_is_running_with_unique_id() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");

    let a = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
    let b = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
    assert_ne!(a, b, "each session has a unique id");

    let detail = fx.app.session(a).unwrap();
    assert_eq!(detail.session.status, SessionStatus::Running);
    assert!(detail.session.started_at.is_some());
}

#[tokio::test]
async fn as2_preexisting_session_carries_isolated_worktree() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");
    let worktree = WorktreeRef {
        path: format!("{}/wt", fx.dir.path().display()),
        branch: "feature".into(),
    };
    let req = StartSessionRequest {
        tool_id: tool,
        objective: fx.sample_objective(),
        origin: Origin::PreExisting,
        worktree: Some(worktree.clone()),
        backend: daedalus_proto::BackendKind::Fake,
        limits: daedalus_proto::ResourceLimits::default(),
    };
    let id = fx.core.start_session(req).await.unwrap();
    let detail = fx.app.session(id).unwrap();
    assert_eq!(detail.environment.origin, Origin::PreExisting);
    assert_eq!(detail.environment.worktree_ref, Some(worktree));
}

#[tokio::test]
async fn as4_unprovisionable_fails_cleanly_with_no_orphan() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");
    fx.backend.fail_next_acquire("backend at capacity");

    let err = fx
        .core
        .start_session(fx.fresh_request(tool))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("provision"));
    assert!(fx.app.fleet().is_empty());
    assert_eq!(fx.backend.live_environment_count(), 0);
}

#[tokio::test]
async fn preexisting_without_worktree_is_rejected() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");
    let req = StartSessionRequest {
        tool_id: tool,
        objective: fx.sample_objective(),
        origin: Origin::PreExisting,
        worktree: None,
        backend: daedalus_proto::BackendKind::Fake,
        limits: daedalus_proto::ResourceLimits::default(),
    };
    assert!(fx.core.start_session(req).await.is_err());
    assert!(fx.app.fleet().is_empty());
}
