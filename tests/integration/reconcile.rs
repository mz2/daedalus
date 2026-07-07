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

    // Contact lost ⇒ Unknown, with the last-known state preserved (never shown healthy —
    // FR-020) and history intact.
    let detail = restarted.session_detail(id).unwrap();
    assert_eq!(detail.session.status, SessionStatus::Unknown);
    assert_eq!(
        detail.session.last_known_status,
        Some(SessionStatus::Running)
    );
    assert!(restarted.store().list_events(id).unwrap().len() > events_before);
}

/// Guards reconciliation's writer choice (`set_session_status`, NOT the canonical
/// `set_session_state`): a session blocked on the operator that loses contact and is later
/// restored must keep its full waiting state. `set_session_state` would NULL the waiting
/// columns on the `Unknown` transition (Unknown is not a waiting state), so the restore
/// could not re-derive the prompt/anchor — reconciliation deliberately uses the status-only
/// writer to preserve `pending_prompt` + `waiting_since` (and `last_known_status`) across the
/// round-trip (FR-020).
#[tokio::test]
async fn unknown_restore_preserves_a_waiting_sessions_prompt_and_anchor() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");
    let id = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
    fx.core
        .mark_waiting_for_input(id, "Which database?")
        .unwrap();
    let waiting = fx.app.session(id).unwrap().session;
    let anchor = waiting.waiting_since.expect("waiting anchor set");

    // Contact lost: Unknown, last-known preserved, waiting columns intact.
    fx.core.mark_connection_lost(id).unwrap();
    let unknown = fx.app.session(id).unwrap().session;
    assert_eq!(unknown.status, SessionStatus::Unknown);
    assert_eq!(
        unknown.last_known_status,
        Some(SessionStatus::WaitingForInput)
    );
    assert_eq!(unknown.pending_prompt.as_deref(), Some("Which database?"));
    assert_eq!(unknown.waiting_since, Some(anchor));

    // Contact restored (the live runtime handle keeps the env reachable): the preserved
    // WaitingForInput state — prompt + anchor included — is re-derived.
    let corrected = fx.core.reconcile().await.unwrap();
    assert_eq!(corrected, 1);
    let restored = fx.app.session(id).unwrap().session;
    assert_eq!(restored.status, SessionStatus::WaitingForInput);
    assert_eq!(restored.last_known_status, None);
    assert_eq!(restored.pending_prompt.as_deref(), Some("Which database?"));
    assert_eq!(restored.waiting_since, Some(anchor));
}
