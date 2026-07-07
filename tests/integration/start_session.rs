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

/// D2(a) regression: a store/persist failure AFTER a successful `acquire` must still tear
/// the environment down — a `?`-return from any of the post-acquire upserts previously
/// leaked the acquired environment (C-B2, mirrors AS4). We force the persist error by
/// making the store directory read-only, so the first post-acquire write (the journal it
/// needs) fails while `acquire` (in-memory) has already succeeded.
#[cfg(unix)]
#[tokio::test]
async fn persist_failure_after_acquire_leaves_no_orphan() {
    use std::os::unix::fs::PermissionsExt;

    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");

    // Make the store's SQLite directory read-only: writes now fail (rollback journal can no
    // longer be created), but reads and the in-memory backend `acquire` still work.
    let db_dir = fx.dir.path().to_path_buf();
    std::fs::set_permissions(&db_dir, std::fs::Permissions::from_mode(0o555)).unwrap();

    let result = fx.core.start_session(fx.fresh_request(tool)).await;

    // Restore permissions so the temp dir can be cleaned up regardless of the outcome.
    std::fs::set_permissions(&db_dir, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert!(result.is_err(), "the persist failure surfaces as an error");
    assert!(
        fx.app.fleet().is_empty(),
        "no session record survives the failed start"
    );
    assert_eq!(
        fx.backend.live_environment_count(),
        0,
        "the acquired environment is torn down — no orphan (C-B2)"
    );
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
