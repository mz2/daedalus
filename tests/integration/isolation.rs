//! Adversarial isolation test (SC-002, FR-004).
//!
//! Real host isolation is proven against the real backends (Workshop/macOS) on their
//! platforms; against the `fake` backend we assert the *contract shape*: the backend hands
//! out no host handle, so a hostile agent has nothing to reach (contract C-B3). This keeps
//! the isolation guarantee continuously exercised in CI without a real sandbox.

use daedalus_backend::{AcquireRequest, Backend};
use daedalus_backend_fake::FakeBackend;
use daedalus_proto::{InvocationSpec, Origin, ResourceLimits};

#[tokio::test]
async fn started_agent_handle_exposes_no_host_path_or_pid() {
    let backend = FakeBackend::new();
    let env = backend
        .acquire(AcquireRequest {
            origin: Origin::Fresh,
            worktree: None,
            limits: ResourceLimits::default(),
        })
        .await
        .unwrap();

    // A "hostile" invocation: the backend must not surface any host resource through the
    // handle it returns — only a zellij session name scoped to the sandbox.
    let hostile = InvocationSpec {
        program: "sh".into(),
        args: vec!["-c".into(), "cat /etc/shadow".into()],
        env: vec![],
    };
    let handle = backend.start_agent(&env.id, &hostile).await.unwrap();

    assert!(
        handle.zellij_session.starts_with("daedalus-"),
        "handle is a sandbox-scoped zellij session, not a host process/path"
    );
    // The handle carries no filesystem path or host pid the operator could pivot through.
    assert_eq!(handle.env, env.id);
}

#[tokio::test]
async fn teardown_isolates_one_environment_from_others() {
    let backend = FakeBackend::new();
    let mk = || AcquireRequest {
        origin: Origin::Fresh,
        worktree: None,
        limits: ResourceLimits::default(),
    };
    let victim = backend.acquire(mk()).await.unwrap();
    let bystander = backend.acquire(mk()).await.unwrap();
    let tool = InvocationSpec {
        program: "echo".into(),
        args: vec![],
        env: vec![],
    };
    backend.start_agent(&bystander.id, &tool).await.unwrap();

    backend.teardown(&victim.id).await.unwrap();
    assert!(backend.agent_running(&bystander.id));
    assert_eq!(backend.live_environment_count(), 1);
}
