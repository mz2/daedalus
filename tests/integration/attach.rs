//! Integration tests for the surface-facing attach path (#15): the `App::attach` wrapper
//! must drive the core capture pipeline (redact → persist → observe → broadcast), and the
//! fake local-testing terminal (`ScriptedLiveTerminal`) must produce a real captured stream
//! whose secrets are redacted in the persisted capture (Principle III, FR-008/016/018).

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use daedalus_app::{App, AppEvent};
use daedalus_backend::Backend;
use daedalus_backend_fake::FakeBackend;
use daedalus_core::{BackendRegistry, Core, CoreConfig, Store};
use daedalus_proto::SessionId;
use daedalus_tests::Fixture;
use daedalus_zellij::ScriptedLiveTerminal;

/// Part A: `App::attach` drives the core capture pipeline — the returned channel streams the
/// redacted output, the secret never reaches the surface or the persisted capture, and an
/// `AppEvent::Output` is broadcast.
#[tokio::test]
async fn app_attach_streams_redacted_output_persists_and_broadcasts() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");
    let id = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
    let mut events = fx.app.subscribe();

    fx.terminal.seed(
        id,
        vec![Bytes::from_static(
            b"building...\nexport API_KEY=sk-demo-123456\ndone\n",
        )],
    );

    // The App-level wrapper returns the same duplex channel as the core.
    let mut channel = fx.app.attach(id).await.expect("attach via App");
    let mut streamed = String::new();
    while let Ok(Some(chunk)) =
        tokio::time::timeout(Duration::from_secs(5), channel.output.recv()).await
    {
        streamed.push_str(&String::from_utf8_lossy(&chunk));
    }

    // The surface-visible stream is redacted.
    assert!(streamed.contains("[REDACTED]"), "stream: {streamed:?}");
    assert!(
        !streamed.contains("sk-demo-123456"),
        "secret leaked to surface"
    );

    // It was persisted (redacted) too.
    let persisted = fx.core.session_output(id, 8192);
    assert!(persisted.contains("[REDACTED]"));
    assert!(
        !persisted.contains("sk-demo-123456"),
        "secret leaked to the persisted capture"
    );

    // And an Output event was broadcast to the surfaces.
    let mut saw_output = false;
    while let Ok(evt) = events.try_recv() {
        if matches!(evt, AppEvent::Output { .. }) {
            saw_output = true;
        }
    }
    assert!(saw_output, "an AppEvent::Output was broadcast");
}

/// Part B: the fake backend's live terminal (`ScriptedLiveTerminal`) yields a real captured
/// stream over time whose fake secret is redacted in the persisted capture.
#[tokio::test]
async fn scripted_live_terminal_capture_is_redacted() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = Arc::new(Store::open(dir.path()).unwrap());
    let backend = Arc::new(FakeBackend::new());
    let registry = BackendRegistry::new(vec![backend as Arc<dyn Backend>]);
    let terminal = Arc::new(ScriptedLiveTerminal::new());
    let core = Arc::new(Core::new(
        store,
        registry,
        terminal,
        None,
        CoreConfig::default(),
    ));
    let app = App::new(core.clone());

    // A live session id — the scripted terminal streams for any attached session.
    let id = SessionId::new();
    let mut channel = app.attach(id).await.expect("attach scripted terminal");

    // Drain the whole scripted stream (it emits over a few seconds).
    while let Ok(Some(_)) =
        tokio::time::timeout(Duration::from_secs(10), channel.output.recv()).await
    {}

    let persisted = core.session_output(id, 65536);
    assert!(
        !persisted.is_empty(),
        "the scripted terminal produced captured output"
    );
    assert!(
        persisted.contains("[REDACTED]"),
        "the fake secret was redacted: {persisted:?}"
    );
    assert!(
        !persisted.contains("sk-demo-123456"),
        "the raw secret must never appear in the persisted capture"
    );
}
