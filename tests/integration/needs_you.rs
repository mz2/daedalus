//! Integration test for the "Needs you" attention queue (T071, US6, FR-021a/b,
//! SC-014/015): every session blocked on the operator appears with its kind, cue, waiting
//! duration, and waiting-cost indication, ordered most-answerable-first; healthy sessions
//! never appear; entering a blocked state is announced event-driven (no polling).

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use daedalus_app::AppQuery;
use daedalus_core::AgentSignal;
use daedalus_proto::{
    AppEvent, AttentionKind, BackendKind, CostIndication, SessionId, SessionStatus, Timestamp,
    ToolId,
};
use daedalus_tests::Fixture;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_millis() as i64
}

async fn start(fx: &Fixture, tool: ToolId) -> SessionId {
    fx.core.start_session(fx.fresh_request(tool)).await.unwrap()
}

/// Stream output chunks through the capture pipeline so an exit summary exists.
async fn stream_output(fx: &Fixture, id: SessionId, chunks: Vec<&'static [u8]>) {
    let n = chunks.len();
    fx.terminal
        .seed(id, chunks.into_iter().map(Bytes::from_static).collect());
    let mut channel = fx.core.attach_and_capture(id).await.unwrap();
    for _ in 0..n {
        channel.output.recv().await.unwrap();
    }
}

/// Drive a session into `AwaitingConfirmation` with a known exit summary.
async fn make_confirm(fx: &Fixture, tool: ToolId, summary: &'static [u8]) -> SessionId {
    let id = start(fx, tool).await;
    stream_output(fx, id, vec![summary]).await;
    let status = fx
        .core
        .apply_signal(id, AgentSignal::ExitedCleanly)
        .await
        .unwrap();
    assert_eq!(status, SessionStatus::AwaitingConfirmation);
    id
}

#[tokio::test]
async fn blocked_states_appear_and_healthy_states_do_not() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");

    let waiting = start(&fx, tool).await;
    fx.core
        .mark_waiting_for_input(waiting, "Continue? (y/n)")
        .unwrap();
    let confirm = make_confirm(&fx, tool, b"2 of 4 tracked tasks done\n").await;
    let stalled = start(&fx, tool).await;
    fx.core
        .apply_signal(stalled, AgentSignal::NoProgress)
        .await
        .unwrap();
    let disconnected = start(&fx, tool).await;
    fx.core.mark_connection_lost(disconnected).unwrap();
    let failed = start(&fx, tool).await;
    fx.core
        .apply_signal(failed, AgentSignal::Crashed)
        .await
        .unwrap();

    // Healthy / resolved sessions never appear (FR-021a).
    let running = start(&fx, tool).await;
    let stopped = start(&fx, tool).await;
    fx.core.stop_session(stopped).await.unwrap();
    let req = fx.fresh_request(tool);
    let objective = req.objective.clone();
    fx.write_tasks(&objective, "- [x] T001 Done\n");
    let completed = fx.core.start_session(req).await.unwrap();
    fx.core.refresh_task_board(completed).unwrap();
    assert_eq!(
        fx.app.session(completed).unwrap().session.status,
        SessionStatus::Completed
    );

    let queue = fx.app.needs_you();
    let ids: Vec<SessionId> = queue.iter().map(|i| i.session_id).collect();
    for (id, kind) in [
        (waiting, AttentionKind::WaitingForInput),
        (confirm, AttentionKind::AwaitingConfirmation),
        (stalled, AttentionKind::Stalled),
        (disconnected, AttentionKind::Disconnected),
        (failed, AttentionKind::Failed),
    ] {
        let item = queue
            .iter()
            .find(|i| i.session_id == id)
            .unwrap_or_else(|| panic!("{kind:?} session must be in the queue"));
        assert_eq!(item.kind, kind);
    }
    for absent in [running, stopped, completed] {
        assert!(
            !ids.contains(&absent),
            "healthy/resolved sessions never need the operator"
        );
    }
}

#[tokio::test]
async fn queue_orders_most_answerable_first_then_longest_waiting() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");

    // Created in reverse order to prove sorting is by kind, not insertion.
    let failed = start(&fx, tool).await;
    fx.core
        .apply_signal(failed, AgentSignal::Crashed)
        .await
        .unwrap();
    let disconnected = start(&fx, tool).await;
    fx.core.mark_connection_lost(disconnected).unwrap();
    let stalled = start(&fx, tool).await;
    fx.core
        .apply_signal(stalled, AgentSignal::NoProgress)
        .await
        .unwrap();
    let confirm = make_confirm(&fx, tool, b"done for review\n").await;
    // Two waiting sessions: the longer-waiting one lists first (newest last).
    let waiting_newer = start(&fx, tool).await;
    fx.core
        .mark_waiting_for_input(waiting_newer, "Newer question?")
        .unwrap();
    fx.core
        .store()
        .set_session_waiting(
            waiting_newer,
            Some("Newer question?"),
            Some(Timestamp::from_millis(now_ms() - 60 * 60 * 1000)),
        )
        .unwrap();
    let waiting_older = start(&fx, tool).await;
    fx.core
        .mark_waiting_for_input(waiting_older, "Older question?")
        .unwrap();
    fx.core
        .store()
        .set_session_waiting(
            waiting_older,
            Some("Older question?"),
            Some(Timestamp::from_millis(now_ms() - 2 * 60 * 60 * 1000)),
        )
        .unwrap();

    let queue = fx.app.needs_you();
    let order: Vec<(SessionId, AttentionKind)> =
        queue.iter().map(|i| (i.session_id, i.kind)).collect();
    assert_eq!(
        order,
        vec![
            (waiting_older, AttentionKind::WaitingForInput),
            (waiting_newer, AttentionKind::WaitingForInput),
            (confirm, AttentionKind::AwaitingConfirmation),
            (stalled, AttentionKind::Stalled),
            (disconnected, AttentionKind::Disconnected),
            (failed, AttentionKind::Failed),
        ],
        "most answerable first; within a kind, newest-waiting last"
    );
}

#[tokio::test]
async fn items_carry_cue_and_waiting_duration() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");

    let waiting = start(&fx, tool).await;
    fx.core
        .mark_waiting_for_input(waiting, "Which database should the service use?")
        .unwrap();
    fx.core
        .store()
        .set_session_waiting(
            waiting,
            Some("Which database should the service use?"),
            Some(Timestamp::from_millis(now_ms() - 2 * 60 * 60 * 1000)),
        )
        .unwrap();
    let confirm = make_confirm(&fx, tool, b"2 of 4 tracked tasks done\n").await;
    let stalled = start(&fx, tool).await;
    fx.core
        .apply_signal(stalled, AgentSignal::NoProgress)
        .await
        .unwrap();
    let disconnected = start(&fx, tool).await;
    fx.core.mark_connection_lost(disconnected).unwrap();
    let failed = start(&fx, tool).await;
    fx.core
        .apply_signal(failed, AgentSignal::Crashed)
        .await
        .unwrap();

    let queue = fx.app.needs_you();
    let item = |id: SessionId| queue.iter().find(|i| i.session_id == id).unwrap();

    // The cue is the question / exit summary / reason line (SC-015, data-model).
    assert_eq!(item(waiting).cue, "Which database should the service use?");
    assert_eq!(item(confirm).cue, "2 of 4 tracked tasks done");
    assert!(item(stalled).cue.to_lowercase().contains("no progress"));
    assert!(item(failed).cue.to_lowercase().contains("crashed"));
    assert!(item(disconnected)
        .cue
        .to_lowercase()
        .contains("connection lost"));

    // Waiting durations: anchored on waiting_since when set, time-in-state otherwise.
    let two_hours = Duration::from_secs(2 * 60 * 60);
    let slack = Duration::from_secs(60);
    assert!(item(waiting).waiting >= two_hours - slack);
    assert!(item(waiting).waiting <= two_hours + slack);
    for id in [confirm, stalled, disconnected, failed] {
        assert!(
            item(id).waiting < slack,
            "a just-blocked session has only just started waiting"
        );
    }
}

#[tokio::test]
async fn waiting_cost_shows_idle_estimate_env_held_and_nothing_without_a_rate() {
    // A backend with a configured idle rate: a live blocked env accrues an estimate.
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");
    fx.core
        .backend_registry()
        .set_idle_rate(BackendKind::Fake, Some(0.60));

    let waiting = start(&fx, tool).await;
    fx.core
        .mark_waiting_for_input(waiting, "Continue? (y/n)")
        .unwrap();
    fx.core
        .store()
        .set_session_waiting(
            waiting,
            Some("Continue? (y/n)"),
            Some(Timestamp::from_millis(now_ms() - 2 * 60 * 60 * 1000)),
        )
        .unwrap();
    let confirm = make_confirm(&fx, tool, b"done\n").await;

    let queue = fx.app.needs_you();
    let item = |id: SessionId| queue.iter().find(|i| i.session_id == id).unwrap();
    match item(waiting).cost {
        CostIndication::Idle(estimate) => {
            // ≈ rate × waiting hours = 0.60 × 2h = 1.20.
            assert!(
                (estimate - 1.20).abs() < 0.05,
                "idle estimate ≈ rate × waiting-hours, got {estimate}"
            );
        }
        ref other => panic!("a live blocked env with a rate shows Idle(_), got {other:?}"),
    }
    // The agent has exited but the environment is still held (FR-021b).
    assert_eq!(item(confirm).cost, CostIndication::EnvHeld);

    // No configured rate ⇒ no cost estimate (time is still shown).
    let bare = Fixture::new();
    let tool = bare.register_sample_tool("claude");
    let waiting = start(&bare, tool).await;
    bare.core
        .mark_waiting_for_input(waiting, "Continue? (y/n)")
        .unwrap();
    let queue = bare.app.needs_you();
    let item = queue.iter().find(|i| i.session_id == waiting).unwrap();
    assert_eq!(item.cost, CostIndication::None);
    assert!(item.waiting <= Duration::from_secs(60));
}

#[tokio::test]
async fn an_idle_rate_set_through_settings_feeds_the_next_needs_you_cost() {
    // FR-021b (T086): the Settings path — Command::SetIdleRate — configures the
    // per-backend idle rate used by the very next needs_you() cost derivation, and
    // clearing it removes the estimate again.
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");

    let waiting = start(&fx, tool).await;
    fx.core
        .mark_waiting_for_input(waiting, "Continue? (y/n)")
        .unwrap();
    fx.core
        .store()
        .set_session_waiting(
            waiting,
            Some("Continue? (y/n)"),
            Some(Timestamp::from_millis(now_ms() - 2 * 60 * 60 * 1000)),
        )
        .unwrap();

    // No rate configured yet ⇒ no cost estimate.
    assert_eq!(
        fx.app.needs_you()[0].cost,
        CostIndication::None,
        "without a configured rate only the waiting time is shown"
    );

    // Configure through the surface command path (Settings drives this).
    fx.app
        .execute(daedalus_app::Command::SetIdleRate {
            backend: BackendKind::Fake,
            rate: Some(0.60),
        })
        .await
        .unwrap();
    match fx.app.needs_you()[0].cost {
        CostIndication::Idle(estimate) => {
            assert!(
                (estimate - 1.20).abs() < 0.05,
                "idle estimate ≈ rate × waiting-hours, got {estimate}"
            );
        }
        ref other => panic!("a configured rate yields Idle(_), got {other:?}"),
    }

    // Clearing the rate removes the estimate (the row is clearable in Settings).
    fx.app
        .execute(daedalus_app::Command::SetIdleRate {
            backend: BackendKind::Fake,
            rate: None,
        })
        .await
        .unwrap();
    assert_eq!(fx.app.needs_you()[0].cost, CostIndication::None);
}

#[tokio::test]
async fn configured_idle_rates_survive_a_core_restart() {
    // FR-021b (T086): the rate set through the app path is persisted and re-seeded into
    // the backend registry when the core is rebuilt over the same store.
    use std::sync::Arc;

    use daedalus_backend::Backend;
    use daedalus_backend_fake::FakeBackend;
    use daedalus_core::{BackendRegistry, Core, CoreConfig, Store};
    use daedalus_zellij::InMemoryTerminal;

    let dir = tempfile::TempDir::new().unwrap();
    let build = || {
        let store = Arc::new(Store::open(dir.path()).unwrap());
        let registry = BackendRegistry::new(vec![Arc::new(FakeBackend::new()) as Arc<dyn Backend>]);
        Arc::new(Core::new(
            store,
            registry,
            Arc::new(InMemoryTerminal::new()),
            None,
            CoreConfig::default(),
        ))
    };

    let core = build();
    core.set_idle_rate(BackendKind::Fake, Some(0.60)).unwrap();
    drop(core);

    let restarted = build();
    assert_eq!(
        restarted.backend_registry().idle_rate(BackendKind::Fake),
        Some(0.60),
        "idle rates are configuration — they survive restart"
    );
}

#[tokio::test]
async fn no_blocked_sessions_means_an_empty_queue() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");
    start(&fx, tool).await; // running — not blocked

    // "Nothing needs you" is a clean empty result, not an error or a sentinel.
    assert!(fx.app.needs_you().is_empty());
}

#[tokio::test]
async fn entering_a_blocked_state_is_announced_event_driven() {
    // SC-014's ≤5s appearance budget is met by push events, not polling: entering any
    // blocked state emits an AppEvent identifying the session and the new status.
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");
    let id = start(&fx, tool).await;

    let mut rx = fx.app.subscribe();
    fx.core
        .mark_waiting_for_input(id, "Continue? (y/n)")
        .unwrap();

    let announced = std::iter::from_fn(|| rx.try_recv().ok()).any(|e| {
        matches!(
            e,
            AppEvent::SessionStatusChanged {
                id: sid,
                status: SessionStatus::WaitingForInput,
            } if sid == id
        )
    });
    assert!(
        announced,
        "a session entering a blocked state is pushed to the surfaces immediately"
    );
    assert_eq!(fx.app.needs_you().len(), 1);
}
