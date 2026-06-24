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
use daedalus_backend_macos::MacosBackend;
use daedalus_backend_workshop::WorkshopBackend;
use daedalus_core::{BackendRegistry, Core, CoreConfig, Store};
use daedalus_proto::BackendKind;
use daedalus_zellij::InMemoryTerminal;

/// Build the app service wired to all v1 backends (fake + macOS + Workshop) over a store
/// at `data_dir`. The terminal attach is the in-memory implementation for the headless
/// build; the `gpui` feature swaps in a zellij-backed terminal view.
pub fn build_app(data_dir: &std::path::Path) -> std::io::Result<App> {
    let store = Arc::new(Store::open(data_dir).map_err(|e| std::io::Error::other(e.to_string()))?);
    let backends: Vec<Arc<dyn Backend>> = vec![
        Arc::new(FakeBackend::new()),
        Arc::new(MacosBackend::new()),
        Arc::new(WorkshopBackend::default()),
    ];
    let registry = BackendRegistry::new(backends);
    let terminal = Arc::new(InMemoryTerminal::new());
    let core = Arc::new(Core::new(
        store,
        registry,
        terminal,
        None,
        CoreConfig::default(),
    ));
    Ok(App::new(core))
}

/// Resolve the default backend kind from `DAEDALUS_BACKEND` (defaults to `fake`).
#[must_use]
pub fn default_backend_kind() -> BackendKind {
    match std::env::var("DAEDALUS_BACKEND").as_deref() {
        Ok("macos") => BackendKind::MacosSandbox,
        Ok("workshop") => BackendKind::Workshop,
        _ => BackendKind::Fake,
    }
}
