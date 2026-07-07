//! Integration test for the fleet view (US5): lists every session with
//! tool/objective/backend/origin/status; enforces the concurrency limit (FR-026); and marks
//! a backend unavailable mid-flight while other sessions keep working.

use std::sync::Arc;

use daedalus_app::AppQuery;
use daedalus_backend::{AcquireRequest, AgentHandle, Backend, BackendError, UsageSample};
use daedalus_backend_fake::FakeBackend;
use daedalus_core::{BackendRegistry, Core, CoreConfig, CoreError, Store};
use daedalus_proto::{
    ArtifactRef, Availability, BackendId, BackendKind, Capabilities, EnvironmentId, InvocationSpec,
    Objective, ObjectiveId, Origin, ResourceLimits, SandboxEnvironment, SessionStatus,
    StartSessionRequest, ToolDef,
};
use daedalus_tests::Fixture;
use daedalus_zellij::InMemoryTerminal;

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

/// A fake backend whose `acquire` blocks on a 2-party barrier, so two concurrent
/// `start_session` calls both get past the pre-acquire concurrency pre-check *before*
/// either persists a session — deterministically forcing the check→insert race (FR-026).
struct BarrierBackend {
    inner: FakeBackend,
    gate: tokio::sync::Barrier,
}

impl BarrierBackend {
    fn new() -> Self {
        Self {
            inner: FakeBackend::new(),
            gate: tokio::sync::Barrier::new(2),
        }
    }
    fn live_environment_count(&self) -> usize {
        self.inner.live_environment_count()
    }
}

#[async_trait::async_trait]
impl Backend for BarrierBackend {
    fn id(&self) -> BackendId {
        self.inner.id()
    }
    fn kind(&self) -> BackendKind {
        self.inner.kind()
    }
    async fn availability(&self) -> Availability {
        self.inner.availability().await
    }
    async fn acquire(&self, req: AcquireRequest) -> Result<SandboxEnvironment, BackendError> {
        // Release only once BOTH concurrent starts have reached acquire — i.e. after both
        // passed the pre-check and before either persisted.
        self.gate.wait().await;
        self.inner.acquire(req).await
    }
    async fn start_agent(
        &self,
        env: &EnvironmentId,
        tool: &InvocationSpec,
    ) -> Result<AgentHandle, BackendError> {
        self.inner.start_agent(env, tool).await
    }
    async fn resource_usage(&self, env: &EnvironmentId) -> Result<Vec<UsageSample>, BackendError> {
        self.inner.resource_usage(env).await
    }
    async fn stop(&self, env: &EnvironmentId) -> Result<(), BackendError> {
        self.inner.stop(env).await
    }
    async fn teardown(&self, env: &EnvironmentId) -> Result<(), BackendError> {
        self.inner.teardown(env).await
    }
}

/// D2(b) regression: with limit 1, two *truly concurrent* starts cannot both slip past the
/// concurrency limit (FR-026). The barrier backend forces both to clear the pre-check
/// before either persists; the atomic slot reservation must then let exactly one through
/// and reject the other with `ConcurrencyLimitReached`, leaving no orphaned environment.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_starts_cannot_both_exceed_the_limit() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = Arc::new(Store::open(dir.path()).unwrap());
    let backend = Arc::new(BarrierBackend::new());
    let registry = BackendRegistry::new(vec![backend.clone() as Arc<dyn Backend>]);
    let terminal = Arc::new(InMemoryTerminal::new());
    let core = Arc::new(Core::new(
        store,
        registry,
        terminal,
        None,
        CoreConfig {
            concurrency_limit: Some(1),
            ..CoreConfig::default()
        },
    ));

    let tool = core
        .register_tool(ToolDef {
            name: "claude".into(),
            invocation: InvocationSpec {
                program: "echo".into(),
                args: vec!["hi".into()],
                env: Vec::new(),
            },
            capabilities: Capabilities {
                accepts_interactive_input: true,
                prompt_convention: None,
            },
        })
        .unwrap();

    let root = dir.path().to_string_lossy().into_owned();
    let make_request = || StartSessionRequest {
        tool_id: tool,
        objective: Objective {
            id: ObjectiveId::new(),
            artifact_ref: ArtifactRef {
                root: root.clone(),
                tasks_file: format!("{root}/tasks.md"),
            },
            description: "obj".into(),
        },
        origin: Origin::Fresh,
        worktree: None,
        backend: BackendKind::Fake,
        limits: ResourceLimits::default(),
    };

    let (c1, r1) = (core.clone(), make_request());
    let (c2, r2) = (core.clone(), make_request());
    let h1 = tokio::spawn(async move { c1.start_session(r1).await });
    let h2 = tokio::spawn(async move { c2.start_session(r2).await });
    let a = h1.await.unwrap();
    let b = h2.await.unwrap();

    let oks = [&a, &b].iter().filter(|r| r.is_ok()).count();
    let errs = [&a, &b].iter().filter(|r| r.is_err()).count();
    assert_eq!(
        oks, 1,
        "exactly one concurrent start succeeds under limit 1"
    );
    assert_eq!(errs, 1, "the other is rejected");

    let err = a.err().or(b.err()).unwrap();
    assert!(
        matches!(err, CoreError::ConcurrencyLimitReached(1)),
        "rejection cites the concurrency limit: {err}"
    );

    // No orphaned environment: the rejected start tore its acquired environment down.
    assert_eq!(
        backend.live_environment_count(),
        1,
        "only the winning session keeps a live environment"
    );
    // And exactly one non-terminal session persisted.
    let running = core
        .store()
        .list_sessions()
        .unwrap()
        .into_iter()
        .filter(|s| !s.status.is_terminal())
        .count();
    assert_eq!(running, 1, "only one session is persisted as active");
}
