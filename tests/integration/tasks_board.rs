//! Integration test: the aggregate tasks query across ALL sessions (FR-025a, T079).
//!
//! The board's unit is the tracked task, with the session demoted to a field: every row
//! carries its task id/description/status plus session id/objective/status, tool, and
//! backend; it is filterable by task status / session / tool / backend; tasks whose
//! session is blocked on the operator carry the attention flag with its kind; and rows
//! come grouped by task status with stable ordering inside each group.

use daedalus_app::AppQuery;
use daedalus_core::AgentSignal;
use daedalus_proto::{
    AggregateTask, AttentionKind, BackendKind, SessionId, SessionStatus, TaskStatus, TasksFilter,
};
use daedalus_tests::Fixture;

/// S1 regression: a session that finishes all its tasks while blocked on the operator
/// (WaitingForInput) must still complete — the board hands the decision to the state
/// machine, which allows `(WaitingForInput, AllTasksDone) => Completed` (FR-015a).
#[tokio::test]
async fn all_tasks_done_completes_a_waiting_for_input_session() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");
    let req = fx.fresh_request(tool);
    let objective = req.objective.clone();
    fx.write_tasks(&objective, "- [x] T001 Setup\n- [ ] T002 Implement\n");
    let id = fx.core.start_session(req).await.unwrap();
    fx.core.refresh_task_board(id).unwrap();

    // The agent blocks on a question: the session enters WaitingForInput.
    fx.core
        .mark_waiting_for_input(id, "Continue? (y/n)")
        .unwrap();
    assert_eq!(
        fx.app.session(id).unwrap().session.status,
        SessionStatus::WaitingForInput
    );

    // The remaining task is now ticked off in tasks.md while still waiting.
    fx.write_tasks(&objective, "- [x] T001 Setup\n- [x] T002 Implement\n");
    fx.core.refresh_task_board(id).unwrap();

    assert_eq!(
        fx.app.session(id).unwrap().session.status,
        SessionStatus::Completed,
        "all-tasks-done completes even from WaitingForInput"
    );
}

/// Completing from a waiting state clears the now-stale waiting columns: a session that was
/// `WaitingForInput` (so it carries a `pending_prompt` + `waiting_since`) and then finishes
/// all its tasks lands in `Completed` with BOTH columns NULLed — the board goes through the
/// canonical `set_session_state` writer, which normalizes the waiting columns from the target
/// (non-waiting) status so no stale prompt/anchor lingers on the finished session.
#[tokio::test]
async fn completing_from_a_waiting_state_clears_the_stale_waiting_columns() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");
    let req = fx.fresh_request(tool);
    let objective = req.objective.clone();
    fx.write_tasks(&objective, "- [x] T001 Setup\n- [ ] T002 Implement\n");
    let id = fx.core.start_session(req).await.unwrap();
    fx.core.refresh_task_board(id).unwrap();

    // Block on a question, so pending_prompt + waiting_since are set.
    fx.core
        .mark_waiting_for_input(id, "Continue? (y/n)")
        .unwrap();
    let waiting = fx.app.session(id).unwrap().session;
    assert_eq!(waiting.status, SessionStatus::WaitingForInput);
    assert_eq!(waiting.pending_prompt.as_deref(), Some("Continue? (y/n)"));
    assert!(waiting.waiting_since.is_some());

    // Finish the remaining task while still waiting → completion.
    fx.write_tasks(&objective, "- [x] T001 Setup\n- [x] T002 Implement\n");
    fx.core.refresh_task_board(id).unwrap();

    let done = fx.app.session(id).unwrap().session;
    assert_eq!(done.status, SessionStatus::Completed);
    assert_eq!(
        done.pending_prompt, None,
        "completion clears the stale pending prompt"
    );
    assert_eq!(
        done.waiting_since, None,
        "completion clears the stale waiting anchor"
    );
}

/// A `Starting` session with an all-done board must NOT be spuriously completed: the state
/// machine rejects `(Starting, AllTasksDone)`, so the board leaves it untouched.
#[tokio::test]
async fn all_tasks_done_does_not_complete_a_starting_session() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");
    let req = fx.fresh_request(tool);
    let objective = req.objective.clone();
    fx.write_tasks(&objective, "- [x] T001 Setup\n- [x] T002 Implement\n");
    let id = fx.core.start_session(req).await.unwrap();

    // Force the session back to the transient Starting state.
    fx.core
        .store()
        .set_session_status(id, SessionStatus::Starting, None, None)
        .unwrap();

    fx.core.refresh_task_board(id).unwrap();
    assert_eq!(
        fx.app.session(id).unwrap().session.status,
        SessionStatus::Starting,
        "a Starting session is never completed by the board"
    );
}

/// Two sessions with distinct tools and boards: `claude` working T001/T002 (in progress /
/// todo) and `goose` with T001 done + T002 blocked.
async fn two_sessions(fx: &Fixture) -> (SessionId, SessionId) {
    let claude = fx.register_sample_tool("claude");
    let req = fx.fresh_request(claude);
    fx.write_tasks(
        &req.objective,
        "- [~] T001 Wire middleware\n- [ ] T002 Tests\n",
    );
    let a = fx.core.start_session(req).await.unwrap();
    fx.core.refresh_task_board(a).unwrap();

    let goose = fx.register_sample_tool("goose");
    let req = fx.fresh_request(goose);
    fx.write_tasks(&req.objective, "- [x] T001 Audit\n- [!] T002 Load test\n");
    let b = fx.core.start_session(req).await.unwrap();
    fx.core.refresh_task_board(b).unwrap();
    (a, b)
}

#[tokio::test]
async fn every_tracked_task_appears_with_its_session_context() {
    let fx = Fixture::new();
    let (a, b) = two_sessions(&fx).await;

    let all = fx.app.all_tasks();
    assert_eq!(all.len(), 4, "every tracked task across all sessions");

    // Each row carries task id/description/status + session id/objective/status +
    // tool + backend (FR-025a).
    let row = all
        .iter()
        .find(|t| t.session == a && t.task.id.0 == "T001")
        .expect("claude T001");
    assert_eq!(row.task.description, "T001 Wire middleware");
    assert_eq!(row.task.status, TaskStatus::InProgress);
    assert_eq!(row.objective, "sample objective");
    assert_eq!(row.session_status, SessionStatus::Running);
    assert_eq!(row.tool, "claude");
    assert_eq!(row.backend, BackendKind::Fake);
    assert!(!row.spec.is_empty(), "spec ref for the row sub-line");

    let blocked = all
        .iter()
        .find(|t| t.session == b && t.task.id.0 == "T002")
        .expect("goose T002");
    assert_eq!(blocked.task.status, TaskStatus::Blocked);
    assert_eq!(blocked.tool, "goose");
}

#[tokio::test]
async fn the_board_is_filterable_by_status_session_tool_and_backend() {
    let fx = Fixture::new();
    let (a, _b) = two_sessions(&fx).await;
    let all = fx.app.all_tasks();

    let by_status: Vec<&AggregateTask> = all
        .iter()
        .filter(|t| {
            TasksFilter {
                status: Some(TaskStatus::Blocked),
                ..TasksFilter::default()
            }
            .matches(t)
        })
        .collect();
    assert_eq!(by_status.len(), 1);
    assert_eq!(by_status[0].task.id.0, "T002");

    let by_session: Vec<&AggregateTask> = all
        .iter()
        .filter(|t| {
            TasksFilter {
                session: Some(a),
                ..TasksFilter::default()
            }
            .matches(t)
        })
        .collect();
    assert_eq!(by_session.len(), 2);
    assert!(by_session.iter().all(|t| t.session == a));

    let by_tool: Vec<&AggregateTask> = all
        .iter()
        .filter(|t| {
            TasksFilter {
                tool: Some("goose".into()),
                ..TasksFilter::default()
            }
            .matches(t)
        })
        .collect();
    assert_eq!(by_tool.len(), 2);
    assert!(by_tool.iter().all(|t| t.tool == "goose"));

    let by_backend: Vec<&AggregateTask> = all
        .iter()
        .filter(|t| {
            TasksFilter {
                backend: Some(BackendKind::Fake),
                ..TasksFilter::default()
            }
            .matches(t)
        })
        .collect();
    assert_eq!(
        by_backend.len(),
        4,
        "all fixture sessions run on the fake backend"
    );

    // An unconstrained filter matches everything.
    assert!(all.iter().all(|t| TasksFilter::default().matches(t)));
}

#[tokio::test]
async fn tasks_of_sessions_blocked_on_the_operator_carry_the_attention_kind() {
    let fx = Fixture::new();
    let (a, b) = two_sessions(&fx).await;

    // Running sessions: no attention flag.
    assert!(fx.app.all_tasks().iter().all(|t| t.attention.is_none()));

    // `claude` asks a question; `goose` crashes — every one of their tasks now carries
    // the attention flag with the session's attention kind.
    fx.core
        .mark_waiting_for_input(a, "Continue? (y/n)")
        .unwrap();
    fx.core.apply_signal(b, AgentSignal::Crashed).await.unwrap();

    let all = fx.app.all_tasks();
    for t in &all {
        if t.session == a {
            assert_eq!(t.attention, Some(AttentionKind::WaitingForInput));
            assert_eq!(t.session_status, SessionStatus::WaitingForInput);
        } else {
            assert_eq!(t.attention, Some(AttentionKind::Failed));
            assert_eq!(t.session_status, SessionStatus::Failed);
        }
    }
}

#[tokio::test]
async fn rows_come_grouped_by_task_status_with_stable_order_inside_groups() {
    let fx = Fixture::new();
    let (a, b) = two_sessions(&fx).await;

    let all = fx.app.all_tasks();
    // Board group order (design `tasks.jsx` TASK_COLS): In progress, Blocked, To do, Done.
    let rank = |s: TaskStatus| match s {
        TaskStatus::InProgress => 0,
        TaskStatus::Blocked => 1,
        TaskStatus::Todo => 2,
        TaskStatus::Done => 3,
    };
    let ranks: Vec<u8> = all.iter().map(|t| rank(t.task.status)).collect();
    let mut sorted = ranks.clone();
    sorted.sort_unstable();
    assert_eq!(ranks, sorted, "rows arrive grouped by task status");

    // Stable inside a group: repeated queries keep the same order, and rows within a
    // group preserve session order then task-id order.
    assert_eq!(
        all.iter()
            .map(|t| (t.session, t.task.id.0.clone()))
            .collect::<Vec<_>>(),
        fx.app
            .all_tasks()
            .iter()
            .map(|t| (t.session, t.task.id.0.clone()))
            .collect::<Vec<_>>()
    );
    assert_eq!(all[0].session, a, "In progress: claude T001");
    assert_eq!(all[0].task.id.0, "T001");
    assert_eq!(all[1].task.status, TaskStatus::Blocked);
    assert_eq!(all[1].session, b);
}
