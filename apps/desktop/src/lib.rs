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
use daedalus_zellij::{InMemoryTerminal, ScriptedLiveTerminal, TerminalAttach};

/// Build the app service wired to all v1 backends (fake + Workshop + OpenShell) over a store at
/// `data_dir`. The terminal attach is chosen from the default backend: the `fake` backend
/// (the local-testing path, Principle III) uses a synthetic *live* terminal so the surface
/// exercises the real capture pipeline (redact → persist → observe → stream); other backends
/// use the empty in-memory terminal, which reports a clear not-attachable reason until a
/// real zellij-backed attach lands (#11), never a silent blank (contract C-T2).
pub fn build_app(data_dir: &std::path::Path) -> std::io::Result<App> {
    let store = Arc::new(Store::open(data_dir).map_err(|e| std::io::Error::other(e.to_string()))?);
    let backends: Vec<Arc<dyn Backend>> = vec![
        Arc::new(FakeBackend::new()),
        Arc::new(WorkshopBackend::default()),
        Arc::new(OpenShellBackend::default()),
    ];
    let registry = BackendRegistry::new(backends);
    let terminal: Arc<dyn TerminalAttach> = match default_backend_kind() {
        BackendKind::Fake => Arc::new(ScriptedLiveTerminal::new()),
        _ => Arc::new(InMemoryTerminal::new()),
    };
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
