//! Contract tests for the `Backend` trait (C-B1…C-B5), exercised against the fake backend.

use daedalus_backend::{AcquireRequest, Backend, BackendError};
use daedalus_backend_fake::FakeBackend;
use daedalus_proto::{Origin, ResourceLimits, WorktreeRef};

fn fresh_req() -> AcquireRequest {
    AcquireRequest {
        origin: Origin::Fresh,
        worktree: None,
        limits: ResourceLimits::default(),
    }
}

#[tokio::test]
async fn c_b1_preexisting_without_worktree_fails_and_leaves_nothing() {
    let backend = FakeBackend::new();
    backend.fail_worktree("cannot create worktree");
    let req = AcquireRequest {
        origin: Origin::PreExisting,
        worktree: Some(WorktreeRef {
            path: "/tmp/wt".into(),
            branch: "feat".into(),
        }),
        limits: ResourceLimits::default(),
    };
    let err = backend.acquire(req).await.unwrap_err();
    assert!(matches!(err, BackendError::WorktreeUnavailable(_)));
    assert_eq!(backend.live_environment_count(), 0, "no orphan environment");
}

#[tokio::test]
async fn c_b2_failed_start_leaves_no_orphan() {
    let backend = FakeBackend::new();
    let env = backend.acquire(fresh_req()).await.unwrap();
    backend.fail_next_start("boom");
    let tool = daedalus_proto::InvocationSpec {
        program: "echo".into(),
        args: vec![],
        env: vec![],
    };
    let err = backend.start_agent(&env.id, &tool).await.unwrap_err();
    assert!(matches!(err, BackendError::StartFailed(_)));
    assert_eq!(
        backend.live_environment_count(),
        0,
        "env torn down on failed start"
    );
}

#[tokio::test]
async fn c_b4_availability_never_errors_and_reflects_down_state() {
    let backend = FakeBackend::new();
    assert_eq!(
        backend.availability().await,
        daedalus_proto::Availability::Available
    );
    backend.set_availability(daedalus_proto::Availability::Unavailable);
    // No panic / no error type — availability is a value.
    assert_eq!(
        backend.availability().await,
        daedalus_proto::Availability::Unavailable
    );
}

#[tokio::test]
async fn c_b5_stop_and_teardown_are_idempotent_and_isolated() {
    let backend = FakeBackend::new();
    let a = backend.acquire(fresh_req()).await.unwrap();
    let b = backend.acquire(fresh_req()).await.unwrap();
    let tool = daedalus_proto::InvocationSpec {
        program: "echo".into(),
        args: vec![],
        env: vec![],
    };
    backend.start_agent(&a.id, &tool).await.unwrap();
    backend.start_agent(&b.id, &tool).await.unwrap();

    // Idempotent stop/teardown on A; B is unaffected.
    backend.stop(&a.id).await.unwrap();
    backend.stop(&a.id).await.unwrap();
    backend.teardown(&a.id).await.unwrap();
    backend.teardown(&a.id).await.unwrap();

    assert!(!backend.agent_running(&a.id));
    assert!(
        backend.agent_running(&b.id),
        "tearing down A must not touch B"
    );
    assert_eq!(backend.live_environment_count(), 1);
}
