//! Integration test for monitoring (FR-015a/016/017/018): output streams, the board
//! reflects task changes, terminal state (incl. awaiting-confirmation) is determined, and
//! the session record persists output + task history + outcome.

use bytes::Bytes;
use daedalus_app::AppQuery;
use daedalus_core::AgentSignal;
use daedalus_proto::{AppEvent, SessionStatus, TaskStatus};
use daedalus_tests::Fixture;

fn drain(rx: &mut tokio::sync::broadcast::Receiver<AppEvent>) -> Vec<AppEvent> {
    let mut out = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        out.push(ev);
    }
    out
}

#[tokio::test]
async fn output_streams_board_updates_and_state_is_determined() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");
    let req = fx.fresh_request(tool);
    let objective = req.objective.clone();
    fx.write_tasks(&objective, "- [x] T001 Setup\n- [ ] T002 Implement\n");
    let id = fx.core.start_session(req).await.unwrap();

    // Output streams through the redacted capture pipeline (FR-016, FR-018).
    fx.terminal
        .seed(id, vec![Bytes::from_static(b"working...\n")]);
    let mut channel = fx.core.attach_and_capture(id).await.unwrap();
    let chunk = channel.output.recv().await.unwrap();
    assert_eq!(&chunk[..], b"working...\n");

    // The board reflects tasks.md (FR-017).
    let mut rx = fx.app.subscribe();
    let tasks = fx.core.refresh_task_board(id).unwrap();
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0].status, TaskStatus::Done);
    assert_eq!(tasks[1].status, TaskStatus::Todo);
    let events = drain(&mut rx);
    assert!(events
        .iter()
        .any(|e| matches!(e, AppEvent::TaskStatusChanged { .. })));

    // Clean exit with an unfinished task ⇒ AwaitingConfirmation, never Completed (FR-015a).
    let outcome = fx
        .core
        .apply_signal(id, AgentSignal::ExitedCleanly)
        .await
        .unwrap();
    assert_eq!(outcome, SessionStatus::AwaitingConfirmation);

    // Output + task history + outcome are persisted (FR-018).
    assert!(!fx.core.store().list_events(id).unwrap().is_empty());
    assert_eq!(fx.app.task_board(id).len(), 2);
    assert_eq!(
        fx.app.session(id).unwrap().session.status,
        SessionStatus::AwaitingConfirmation
    );
}

#[tokio::test]
async fn session_detail_carries_capture_stats_for_the_trim_notice() {
    // FR-016a: the terminal keeps a bounded scrollback; the session record query exposes
    // how many lines the full persisted capture holds so the surface can show
    // "showing last N of M lines · full log persisted".
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");
    let id = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();

    assert_eq!(fx.app.session(id).unwrap().capture.total_lines, 0);

    let now = daedalus_proto::Timestamp::from_millis(0);
    fx.core
        .store()
        .append_output(id, now, b"line 1\nline 2\n")
        .unwrap();
    fx.core.store().append_output(id, now, b"line 3\n").unwrap();
    assert_eq!(fx.app.session(id).unwrap().capture.total_lines, 3);

    // A trailing unterminated line still counts.
    fx.core.store().append_output(id, now, b"partial").unwrap();
    assert_eq!(fx.app.session(id).unwrap().capture.total_lines, 4);
}

#[tokio::test]
async fn all_tasks_done_completes_the_session() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");
    let req = fx.fresh_request(tool);
    let objective = req.objective.clone();
    fx.write_tasks(&objective, "- [x] T001 Setup\n- [x] T002 Implement\n");
    let id = fx.core.start_session(req).await.unwrap();

    fx.core.refresh_task_board(id).unwrap();
    assert_eq!(
        fx.app.session(id).unwrap().session.status,
        SessionStatus::Completed
    );
}
