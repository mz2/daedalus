//! Integration test for restart reconciliation (FR-029/030, SC-007): after a restart, each
//! non-terminal session's status is re-derived from backend + zellij liveness, with no
//! history lost.

use std::sync::Arc;

use daedalus_app::AppQuery;
use daedalus_backend::Backend;
use daedalus_backend_fake::FakeBackend;
use daedalus_core::{BackendRegistry, Core, CoreConfig, Store};
use daedalus_proto::SessionStatus;
use daedalus_tests::Fixture;

#[tokio::test]
async fn restart_reconciles_lost_sessions_without_losing_history() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");
    let id = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
    assert_eq!(
        fx.app.session(id).unwrap().session.status,
        SessionStatus::Running
    );
    let events_before = fx.core.store().list_events(id).unwrap().len();

    // Simulate a restart: a brand-new Core over the SAME SQLite store, but a fresh backend
    // (the previous in-memory environment is gone — contact is lost).
    let store = Arc::new(Store::open(fx.dir.path()).unwrap());
    let backend = Arc::new(FakeBackend::new());
    let registry = BackendRegistry::new(vec![backend as Arc<dyn Backend>]);
    let restarted = Core::new(
        store,
        registry,
        Arc::new(daedalus_zellij::InMemoryTerminal::new()),
        None,
        CoreConfig::default(),
    );

    let corrected = restarted.reconcile().await.unwrap();
    assert_eq!(corrected, 1);

    // Status re-derived to an attention state (Stalled), history preserved.
    let detail = restarted.session_detail(id).unwrap();
    assert_eq!(detail.session.status, SessionStatus::Stalled);
    assert!(restarted.store().list_events(id).unwrap().len() > events_before);
}
