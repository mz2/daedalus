//! Shared helpers for the Daedalus contract + integration tests.
//!
//! Test targets live under `tests/contract/`, `tests/integration/`, and `tests/unit/` and
//! are declared as `[[test]]` entries in `Cargo.toml`. This lib holds common fixtures so
//! every suite runs against the in-memory `fake` backend locally (Principle III).

use std::sync::Arc;

use tempfile::TempDir;

use daedalus_app::App;
use daedalus_backend::Backend;
use daedalus_backend_fake::FakeBackend;
use daedalus_core::{BackendRegistry, Core, CoreConfig, Store};
use daedalus_proto::{
    AgenticTool, ArtifactRef, BackendKind, Capabilities, InvocationSpec, Objective, ObjectiveId,
    Origin, StartSessionRequest, ToolDef,
};
use daedalus_zellij::InMemoryTerminal;

/// A ready-to-drive test fixture: an [`App`] over a [`Core`] backed by the fake backend, an
/// in-memory terminal you can seed, and a temp dir holding the SQLite store + captures.
pub struct Fixture {
    pub app: App,
    pub core: Arc<Core>,
    pub backend: Arc<FakeBackend>,
    pub terminal: Arc<InMemoryTerminal>,
    pub dir: TempDir,
}

impl Fixture {
    /// Build a fixture with the default core config.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(CoreConfig::default())
    }

    /// Build a fixture with a custom core config (e.g. a concurrency limit).
    #[must_use]
    pub fn with_config(config: CoreConfig) -> Self {
        let dir = TempDir::new().expect("tempdir");
        let store = Arc::new(Store::open(dir.path()).expect("store"));
        let backend = Arc::new(FakeBackend::new());
        let registry = BackendRegistry::new(vec![backend.clone() as Arc<dyn Backend>]);
        let terminal = Arc::new(InMemoryTerminal::new());
        let core = Arc::new(Core::new(store, registry, terminal.clone(), None, config));
        let app = App::new(core.clone());
        Self {
            app,
            core,
            backend,
            terminal,
            dir,
        }
    }

    /// Register the standard sample tool (accepts input) and return its id.
    pub fn register_sample_tool(&self, name: &str) -> daedalus_proto::ToolId {
        self.core
            .register_tool(ToolDef {
                name: name.to_string(),
                invocation: InvocationSpec {
                    program: "echo".to_string(),
                    args: vec!["hello".to_string()],
                    env: Vec::new(),
                },
                capabilities: Capabilities {
                    accepts_interactive_input: true,
                    prompt_convention: None,
                },
            })
            .expect("register tool")
    }

    /// Build a fresh-environment start request for a tool, with an objective whose
    /// `tasks.md` lives in the fixture's temp dir.
    pub fn fresh_request(&self, tool_id: daedalus_proto::ToolId) -> StartSessionRequest {
        StartSessionRequest {
            tool_id,
            objective: self.sample_objective(),
            origin: Origin::Fresh,
            worktree: None,
            backend: BackendKind::Fake,
            limits: daedalus_proto::ResourceLimits::default(),
        }
    }

    /// An objective whose tasks file is written into the fixture temp dir.
    pub fn sample_objective(&self) -> Objective {
        let root = self.dir.path().to_string_lossy().into_owned();
        Objective {
            id: ObjectiveId::new(),
            artifact_ref: ArtifactRef {
                root: root.clone(),
                tasks_file: format!("{root}/tasks.md"),
            },
            description: "sample objective".to_string(),
        }
    }

    /// Write a `tasks.md` for the given objective in the temp dir.
    pub fn write_tasks(&self, objective: &Objective, contents: &str) {
        std::fs::write(&objective.artifact_ref.tasks_file, contents).expect("write tasks.md");
    }
}

impl Default for Fixture {
    fn default() -> Self {
        Self::new()
    }
}

/// Build a [`Core`] wired to a discovery coordinator over the given sources, sharing the
/// fixture's store/backend/terminal. Returns the core and the temp dir to keep it alive.
#[must_use]
pub fn core_with_discovery(
    sources: Vec<Box<dyn daedalus_discovery::DiscoverySource>>,
) -> (Arc<Core>, TempDir) {
    let (core, _, dir) = core_with_discovery_and_backend(sources);
    (core, dir)
}

/// Like [`core_with_discovery`], but also hands back the fake backend so a test can drive
/// its availability (FR-028).
#[must_use]
pub fn core_with_discovery_and_backend(
    sources: Vec<Box<dyn daedalus_discovery::DiscoverySource>>,
) -> (Arc<Core>, Arc<FakeBackend>, TempDir) {
    let dir = TempDir::new().expect("tempdir");
    let store = Arc::new(Store::open(dir.path()).expect("store"));
    let backend = Arc::new(FakeBackend::new());
    let registry = BackendRegistry::new(vec![backend.clone() as Arc<dyn Backend>]);
    let terminal = Arc::new(InMemoryTerminal::new());
    let coord = Arc::new(daedalus_discovery::DiscoveryCoordinator::new(sources));
    let core = Arc::new(Core::new(
        store,
        registry,
        terminal,
        Some(coord),
        CoreConfig::default(),
    ));
    (core, backend, dir)
}

/// A configurable in-memory discovery source for the discovery tests.
pub struct TestDiscoverySource {
    id: daedalus_proto::SourceId,
    kind: daedalus_proto::SourceKind,
    sessions: std::sync::Mutex<Vec<daedalus_proto::DiscoveredSession>>,
    availability: std::sync::Mutex<daedalus_proto::Availability>,
}

impl TestDiscoverySource {
    /// A new, available source of the given kind.
    #[must_use]
    pub fn new(kind: daedalus_proto::SourceKind) -> Self {
        Self {
            id: daedalus_proto::SourceId::new(),
            kind,
            sessions: std::sync::Mutex::new(Vec::new()),
            availability: std::sync::Mutex::new(daedalus_proto::Availability::Available),
        }
    }

    /// Replace the sessions this source advertises.
    pub fn set_sessions(&self, sessions: Vec<daedalus_proto::DiscoveredSession>) {
        *self.sessions.lock().unwrap() = sessions;
    }

    /// Override the source's availability (and, when unavailable, stop advertising).
    pub fn set_availability(&self, availability: daedalus_proto::Availability) {
        if availability == daedalus_proto::Availability::Unavailable {
            self.sessions.lock().unwrap().clear();
        }
        *self.availability.lock().unwrap() = availability;
    }

    /// Build a discovered session attributed to this source.
    #[must_use]
    pub fn make_session(
        &self,
        identity: &str,
        attachable: bool,
    ) -> daedalus_proto::DiscoveredSession {
        daedalus_proto::DiscoveredSession {
            id: daedalus_proto::DiscoveredSessionId {
                source: self.id,
                identity: daedalus_proto::SessionIdentity::new(identity),
            },
            kind: self.kind,
            source_availability: daedalus_proto::Availability::Available,
            zellij_session: format!("daedalus-{identity}"),
            host_label: "test-host".to_string(),
            status: None,
            attachable,
            attach_reason: if attachable {
                None
            } else {
                Some("zellij session has exited".to_string())
            },
            artifacts: None,
        }
    }
}

#[async_trait::async_trait]
impl daedalus_discovery::DiscoverySource for TestDiscoverySource {
    fn kind(&self) -> daedalus_proto::SourceKind {
        self.kind
    }
    fn source_id(&self) -> daedalus_proto::SourceId {
        self.id
    }
    async fn poll(&self) -> Vec<daedalus_proto::DiscoveredSession> {
        self.sessions.lock().unwrap().clone()
    }
    fn availability(&self) -> daedalus_proto::Availability {
        *self.availability.lock().unwrap()
    }
}

/// A discovery source that delegates to a shared [`TestDiscoverySource`], so a test can
/// keep a handle and mutate availability/sessions between coordinator polls.
pub struct SharedTestSource(pub Arc<TestDiscoverySource>);

#[async_trait::async_trait]
impl daedalus_discovery::DiscoverySource for SharedTestSource {
    fn kind(&self) -> daedalus_proto::SourceKind {
        self.0.kind()
    }
    fn source_id(&self) -> daedalus_proto::SourceId {
        self.0.source_id()
    }
    async fn poll(&self) -> Vec<daedalus_proto::DiscoveredSession> {
        self.0.poll().await
    }
    fn availability(&self) -> daedalus_proto::Availability {
        self.0.availability()
    }
}

/// A standalone sample tool record (not registered).
#[must_use]
pub fn sample_tool(name: &str) -> AgenticTool {
    AgenticTool {
        id: daedalus_proto::ToolId::new(),
        name: name.to_string(),
        invocation: InvocationSpec {
            program: "echo".to_string(),
            args: vec!["hello".to_string()],
            env: Vec::new(),
        },
        capabilities: Capabilities {
            accepts_interactive_input: true,
            prompt_convention: None,
        },
    }
}
