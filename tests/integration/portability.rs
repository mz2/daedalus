//! Backend-portability test (SC-008): the same session-lifecycle workflow — start → monitor
//! → stop/clean-up — runs **unchanged** against the fake backend (and any real backend that
//! is available), asserting identical operator workflow/outcomes.

use std::sync::Arc;

use bytes::Bytes;
use daedalus_app::{App, AppQuery, Command, CommandResult};
use daedalus_backend::Backend;
use daedalus_core::{BackendRegistry, Core, CoreConfig, Store};
use daedalus_proto::{
    ArtifactRef, BackendKind, Capabilities, InvocationSpec, Objective, ObjectiveId, Origin,
    SessionStatus, StartSessionRequest, ToolDef,
};
use daedalus_zellij::InMemoryTerminal;
use tempfile::TempDir;

/// The backend-agnostic operator workflow. Identical regardless of backend.
async fn run_lifecycle(app: &App, backend: BackendKind, tasks_dir: &std::path::Path) {
    let CommandResult::ToolRegistered(tool) = app
        .execute(Command::RegisterTool(ToolDef {
            name: "claude".into(),
            invocation: InvocationSpec {
                program: "echo".into(),
                args: vec!["hi".into()],
                env: vec![],
            },
            capabilities: Capabilities {
                accepts_interactive_input: true,
                prompt_convention: None,
            },
        }))
        .await
        .unwrap()
    else {
        panic!("expected ToolRegistered");
    };

    let objective = Objective {
        id: ObjectiveId::new(),
        artifact_ref: ArtifactRef {
            root: tasks_dir.to_string_lossy().into(),
            tasks_file: format!("{}/tasks.md", tasks_dir.display()),
        },
        description: "portable objective".into(),
    };
    let CommandResult::SessionStarted(id) = app
        .execute(Command::StartSession(StartSessionRequest {
            tool_id: tool,
            objective,
            origin: Origin::Fresh,
            worktree: None,
            backend,
            limits: daedalus_proto::ResourceLimits::default(),
        }))
        .await
        .unwrap()
    else {
        panic!("expected SessionStarted");
    };

    // Monitor: the session is running and reachable through the same queries.
    assert_eq!(
        app.session(id).unwrap().session.status,
        SessionStatus::Running
    );

    // Intervene: send input, stop, clean up — same commands on every backend.
    app.execute(Command::SendInput {
        session: id,
        data: Bytes::from_static(b"go\n"),
    })
    .await
    .unwrap();
    app.execute(Command::StopSession(id)).await.unwrap();
    assert_eq!(
        app.session(id).unwrap().session.status,
        SessionStatus::Stopped
    );
    app.execute(Command::CleanUp(id)).await.unwrap();
}

fn app_for(backend: Arc<dyn Backend>) -> (App, TempDir) {
    let dir = TempDir::new().unwrap();
    let store = Arc::new(Store::open(dir.path()).unwrap());
    let registry = BackendRegistry::new(vec![backend]);
    let terminal = Arc::new(InMemoryTerminal::new());
    let core = Arc::new(Core::new(
        store,
        registry,
        terminal,
        None,
        CoreConfig::default(),
    ));
    (App::new(core), dir)
}

#[tokio::test]
async fn lifecycle_is_identical_on_the_fake_backend() {
    let backend = Arc::new(daedalus_backend_fake::FakeBackend::new());
    let (app, dir) = app_for(backend);
    run_lifecycle(&app, BackendKind::Fake, dir.path()).await;
}

#[tokio::test]
async fn lifecycle_is_identical_on_the_workshop_backend() {
    // Only runs where a Workshop control interface is reachable (SC-008 real-backend leg).
    let backend = Arc::new(daedalus_backend_workshop::WorkshopBackend::default());
    if backend.availability().await != daedalus_proto::Availability::Available {
        return;
    }
    let (app, dir) = app_for(backend);
    run_lifecycle(&app, BackendKind::Workshop, dir.path()).await;
}
