//! Daedalus native desktop surface.
//!
//! This crate ports the locked design system (`design/`) to typed Rust tokens and builds
//! render-agnostic **view-models** from the [`daedalus_app::App`] query/command/event API.
//! The actual GPUI rendering lives behind the non-default `gpui` feature (research R-UI:
//! GPUI needs a GPU/display and per-platform validation), so the design system and screen
//! logic stay compiled and unit-tested without a GPU. The library API here is what the
//! GPUI views consume.

pub mod app;
pub mod components;
pub mod screens;
pub mod theme;

#[cfg(feature = "gpui")]
pub mod gpui_ui;

use std::sync::Arc;

use daedalus_app::App;
use daedalus_backend::Backend;
use daedalus_backend_fake::FakeBackend;
use daedalus_backend_openshell::OpenShellBackend;
use daedalus_backend_workshop::WorkshopBackend;
use daedalus_core::{BackendRegistry, Core, CoreConfig, Store};
use daedalus_discovery::{DiscoveryCoordinator, LocalSource, MdnsSource};
use daedalus_proto::BackendKind;
use daedalus_zellij::{AttachError, ProcessTerminal, ScriptedLiveTerminal, TerminalAttach};

/// Build the app service wired to all v1 backends (fake + Workshop + OpenShell) over a
/// store at `data_dir`. Terminal attach routes by each SESSION's backend (fake → scripted
/// live stream, OpenShell → in-sandbox zellij bridge, others → stated pending reason,
/// contract C-T2), and the OpenShell sandbox image is the app-persisted Settings value
/// with `DAEDALUS_OPENSHELL_FROM` as fallback — no environment configuration required.
pub fn build_app(data_dir: &std::path::Path) -> std::io::Result<App> {
    let store = Arc::new(Store::open(data_dir).map_err(|e| std::io::Error::other(e.to_string()))?);
    // The OpenShell sandbox image is operator configuration owned by the app: the
    // persisted Settings value first, DAEDALUS_OPENSHELL_FROM only as a fallback.
    let image_store = store.clone();
    // Zero-config default: when the repo's own agent image is present in the local
    // Docker daemon, use it rather than the bare base image (no zellij / agent tools).
    // Probed once at startup; the persisted Setting and env var still take precedence.
    let local_default = std::process::Command::new("docker")
        .args(["image", "inspect", "daedalus/e2e-openshell:latest"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|_| "daedalus/e2e-openshell:latest".to_string());
    let openshell_control =
        daedalus_backend_openshell::CliOpenShellControl::with_image_source(Arc::new(move || {
            image_store
                .backend_image(BackendKind::OpenShell)
                .ok()
                .flatten()
                .or_else(|| {
                    std::env::var("DAEDALUS_OPENSHELL_FROM")
                        .ok()
                        .filter(|s| !s.is_empty())
                })
                .or_else(|| local_default.clone())
        }));
    let backends: Vec<Arc<dyn Backend>> = vec![
        Arc::new(FakeBackend::new()),
        Arc::new(WorkshopBackend::default()),
        Arc::new(OpenShellBackend::new(Arc::new(openshell_control))),
    ];
    let registry = BackendRegistry::new(backends);
    // Terminal attach routes by the SESSION'S backend — not by a launch-time default —
    // so an OpenShell session gets its bridge and a fake session its scripted stream
    // regardless of how the app was started.
    let terminal: Arc<dyn TerminalAttach> = Arc::new(RoutingTerminal {
        store: store.clone(),
        registry: registry.clone(),
        fake: ScriptedLiveTerminal::new(),
        openshell: ProcessTerminal::new(openshell_attach_resolver(store.clone())),
    });
    // Discover sessions on the local host (zellij) and the LAN (mDNS). Tunnel sources are
    // added when the operator configures them.
    let discovery = Arc::new(DiscoveryCoordinator::new(vec![
        Box::new(LocalSource::new()),
        Box::new(MdnsSource::new()),
    ]));
    let core = Arc::new(Core::new(
        store,
        registry,
        terminal,
        Some(discovery),
        CoreConfig::default(),
    ));
    Ok(App::new(core))
}

/// Session → attach-bridge command for OpenShell sandboxes: look the session's
/// environment up in the store, then bridge via
/// `openshell sandbox exec --tty -n <sandbox> -- zellij attach <session>` (issue #15).
/// Unknown sessions and a missing CLI yield clear reasons (C-T2), never a blank pane.
fn openshell_attach_resolver(store: Arc<Store>) -> daedalus_zellij::AttachCommandResolver {
    use daedalus_backend_openshell::{attach_command, CliOpenShellControl, OpenShellControl};
    Box::new(move |session| {
        let record = store
            .get_session(session)
            .map_err(|_| AttachError::NotFound)?;
        let binary = CliOpenShellControl::default()
            .control_binary()
            .ok_or_else(|| {
                AttachError::ZellijUnavailable(
                    "openshell CLI not found (install OpenShell or set DAEDALUS_OPENSHELL_CMD)"
                        .into(),
                )
            })?;
        Ok(attach_command(&binary, &record.environment_id))
    })
}

/// Routes each attach to the terminal implementation matching the session's backend:
/// fake → the scripted live stream (Principle III), OpenShell → the in-sandbox zellij
/// bridge; anything else states that its attach is pending (#11) — never a blank pane.
struct RoutingTerminal {
    store: Arc<Store>,
    registry: BackendRegistry,
    fake: ScriptedLiveTerminal,
    openshell: ProcessTerminal,
}

#[async_trait::async_trait]
impl TerminalAttach for RoutingTerminal {
    async fn attach(
        &self,
        session: daedalus_proto::SessionId,
    ) -> Result<daedalus_zellij::TerminalChannel, AttachError> {
        let record = self
            .store
            .get_session(session)
            .map_err(|_| AttachError::NotFound)?;
        let env = self
            .store
            .get_environment(record.environment_id)
            .map_err(|_| AttachError::NotFound)?;
        match self.registry.by_id(env.backend_id).map(|b| b.kind()) {
            Some(BackendKind::Fake) => self.fake.attach(session).await,
            Some(BackendKind::OpenShell) => self.openshell.attach(session).await,
            Some(other) => Err(AttachError::Unattachable(format!(
                "terminal attach for {other:?} lands with issue #11"
            ))),
            None => Err(AttachError::Unattachable(
                "the session's backend is not registered on this host".into(),
            )),
        }
    }
}

/// Resolve the default backend kind from `DAEDALUS_BACKEND` (defaults to `fake`).
#[must_use]
pub fn default_backend_kind() -> BackendKind {
    match std::env::var("DAEDALUS_BACKEND").as_deref() {
        Ok("workshop") => BackendKind::Workshop,
        Ok("openshell") => BackendKind::OpenShell,
        Ok("fake") | Err(_) => BackendKind::Fake,
        // An explicitly-set but unrecognized value (e.g. `macos`, which has no wired
        // backend yet) silently ran `fake` before — warn so the operator sees why (D7).
        Ok(other) => {
            tracing::warn!("DAEDALUS_BACKEND=\"{other}\" is not a recognized backend; using fake");
            BackendKind::Fake
        }
    }
}
