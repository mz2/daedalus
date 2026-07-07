//! The shipped desktop wiring (`build_app`) must, on the local-testing `fake` backend,
//! give the surface a real *live* capture stream: attaching drives the core pipeline
//! (redact → persist), so the embedded terminal renders redacted output and the persisted
//! capture never holds the raw secret (#15, FR-008/016/018, Principle III).
//!
//! This is the headless counterpart of the GPUI live launch: it exercises the exact
//! `build_app` code path the desktop's `open_session_focused` uses, without needing a GPU.

use std::time::Duration;

use daedalus_desktop::build_app;
use daedalus_proto::SessionId;

#[tokio::test]
async fn build_app_fake_backend_streams_redacted_live_capture() {
    // Default backend is `fake` when DAEDALUS_BACKEND is unset — the path that wires the
    // synthetic live terminal.
    std::env::remove_var("DAEDALUS_BACKEND");

    let dir = std::env::temp_dir().join(format!("daedalus-attach-test-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let app = build_app(&dir).expect("build_app");

    // The scripted live terminal streams for any attached session id.
    let id = SessionId::new();
    let mut channel = app.attach(id).await.expect("attach via shipped wiring");

    // Drain the whole live stream (it emits over a few seconds).
    let mut streamed = String::new();
    while let Ok(Some(chunk)) =
        tokio::time::timeout(Duration::from_secs(10), channel.output.recv()).await
    {
        streamed.push_str(&String::from_utf8_lossy(&chunk));
    }

    // The surface-visible stream is redacted, and so is the persisted capture.
    assert!(streamed.contains("[REDACTED]"), "stream: {streamed:?}");
    assert!(
        !streamed.contains("sk-demo-123456"),
        "secret reached the surface"
    );

    let persisted = app.core().session_output(id, 65536);
    assert!(persisted.contains("[REDACTED]"), "persisted: {persisted:?}");
    assert!(
        !persisted.contains("sk-demo-123456"),
        "raw secret must never be persisted"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
