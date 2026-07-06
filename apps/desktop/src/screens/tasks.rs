//! Tasks board — the landing view (FR-025a, design `tasks.jsx`): every tracked task
//! across ALL sessions, grouped by task status with per-group counts. The session is
//! demoted to a field the operator filters/jumps by (a session-ref chip per row), the
//! filter toolbar carries status chips + session/tool/backend selects + a text query, and
//! Phase 9's `NeedsStrip` rides pinned on top.

use daedalus_app::{App, AppQuery};
use daedalus_proto::{
    AggregateTask, AttentionKind, BackendKind, SessionId, SessionStatus, TaskStatus, TasksFilter,
};

use crate::screens::needs::NeedsStrip;
use crate::screens::session::SessionLink;
use crate::theme::StatusTone;

/// The board groups in design order (`tasks.jsx` TASK_COLS).
pub const TASK_GROUPS: [(TaskStatus, &str); 4] = [
    (TaskStatus::InProgress, "In progress"),
    (TaskStatus::Blocked, "Blocked"),
    (TaskStatus::Todo, "To do"),
    (TaskStatus::Done, "Done"),
];

/// Filter state (prototype `TasksView`): task-status chips, the session/tool/backend
/// selects, the "Filter tasks…" text query, and the "Needs attention" chip.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TaskFilters {
    /// Active status chips (empty = all statuses).
    pub statuses: Vec<TaskStatus>,
    /// Session select (`None` = "All sessions").
    pub session: Option<SessionId>,
    /// Tool select (`None` = "All tools").
    pub tool: Option<String>,
    /// Backend select (`None` = "All backends").
    pub backend: Option<BackendKind>,
    /// Text query, matched case-insensitively against title, id, and objective.
    pub query: String,
    /// Show only rows whose session needs the operator.
    pub attention_only: bool,
}

impl TaskFilters {
    /// Toggle a status chip on/off.
    pub fn toggle_status(&mut self, status: TaskStatus) {
        if let Some(i) = self.statuses.iter().position(|s| *s == status) {
            self.statuses.remove(i);
        } else {
            self.statuses.push(status);
        }
    }

    /// Whether any filter is active (drives the "Clear filters" affordance).
    #[must_use]
    pub fn is_on(&self) -> bool {
        *self != Self::default()
    }

    /// Whether a row passes every active filter.
    #[must_use]
    pub fn matches(&self, t: &AggregateTask) -> bool {
        if !self.statuses.is_empty() && !self.statuses.contains(&t.task.status) {
            return false;
        }
        if self.attention_only && t.attention.is_none() {
            return false;
        }
        if !self.query.is_empty() {
            let q = self.query.to_lowercase();
            let hay =
                format!("{} {} {}", t.task.description, t.task.id.0, t.objective).to_lowercase();
            if !hay.contains(&q) {
                return false;
            }
        }
        TasksFilter {
            status: None,
            session: self.session,
            tool: self.tool.clone(),
            backend: self.backend,
        }
        .matches(t)
    }
}

/// The session reference chip on a row (`SessionRef` in `tasks.jsx`): the field the
/// operator filters/jumps by. Activating it opens the session.
#[derive(Debug, Clone)]
pub struct SessionRef {
    /// The session to open.
    pub session: SessionId,
    /// Objective line shown in the chip.
    pub objective: String,
    /// The session's status (the chip dot).
    pub status: SessionStatus,
    /// Palette tone for the dot.
    pub tone: StatusTone,
    /// Whether the chip renders in the attention treatment (session blocked on operator).
    pub attention: bool,
}

/// One row on the aggregate board.
#[derive(Debug, Clone)]
pub struct TaskRow {
    /// Owning session (row activation opens it).
    pub session: SessionId,
    /// Task id in mono (e.g. `T001`).
    pub task_id: String,
    /// Title with the leading id token stripped.
    pub title: String,
    /// The sub-line: "spec · tool · backend".
    pub sub_line: String,
    /// Task status.
    pub status: TaskStatus,
    /// Palette tone for the status glyph/dot.
    pub tone: StatusTone,
    /// The session-ref chip.
    pub session_ref: SessionRef,
    /// The needs-tag: set on in-progress/blocked rows whose session is blocked on the
    /// operator (prototype `TaskListGrouped` needs rows).
    pub needs: Option<AttentionKind>,
}

impl TaskRow {
    fn from_aggregate(t: &AggregateTask) -> Self {
        let id = t.task.id.0.clone();
        let title = t
            .task
            .description
            .strip_prefix(&format!("{id} "))
            .unwrap_or(&t.task.description)
            .to_string();
        let attention = t.attention.is_some();
        // The needs tag rides only on rows that are actually in flight (design rule:
        // doing/blocked rows of a blocked session).
        let needs = matches!(t.task.status, TaskStatus::InProgress | TaskStatus::Blocked)
            .then_some(t.attention)
            .flatten();
        Self {
            session: t.session,
            task_id: id,
            title,
            sub_line: format!("{} · {} · {}", t.spec, t.tool, t.backend.as_str()),
            status: t.task.status,
            tone: StatusTone::from_task(t.task.status),
            session_ref: SessionRef {
                session: t.session,
                objective: t.objective.clone(),
                status: t.session_status,
                tone: StatusTone::from_session(t.session_status),
                attention,
            },
            needs,
        }
    }

    /// Activation intent: open the session (a plain open — no answer focus).
    #[must_use]
    pub fn open(&self) -> SessionLink {
        SessionLink {
            session: self.session,
            focus: None,
        }
    }

    /// Screen-reader label for AccessKit (name/role/state — T065 pattern).
    #[must_use]
    pub fn accessible_label(&self) -> String {
        format!(
            "{} {} — {}. Session: {}.",
            self.task_id, self.title, self.sub_line, self.session_ref.objective
        )
    }
}

/// One status group with its count (`tlist-group` / `gcol`).
#[derive(Debug, Clone)]
pub struct TaskGroup {
    /// The grouped status.
    pub status: TaskStatus,
    /// Design label ("In progress" / "Blocked" / "To do" / "Done").
    pub label: &'static str,
    /// Row count shown next to the label.
    pub count: usize,
    /// Rows in stable order.
    pub rows: Vec<TaskRow>,
}

/// Header stats over the whole (unfiltered) board (`sh-stats`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BoardCounts {
    /// In progress.
    pub in_progress: usize,
    /// Blocked.
    pub blocked: usize,
    /// To do.
    pub todo: usize,
    /// Done.
    pub done: usize,
    /// Rows flagged for attention (the "Needs attention (n)" chip count).
    pub attention: usize,
}

/// The design empty state ("No tasks match").
#[derive(Debug, Clone, Copy)]
pub struct TasksEmpty {
    /// Title line.
    pub title: &'static str,
    /// Body line.
    pub body: &'static str,
    /// Whether a "Clear filters" action applies (filters are on).
    pub can_clear: bool,
}

/// The aggregate tasks board — the landing view.
#[derive(Debug, Clone)]
pub struct TasksBoardView {
    /// Screen title.
    pub title: &'static str,
    /// Header counts over the whole board (independent of active filters).
    pub counts: BoardCounts,
    /// The pinned Needs-you strip (Phase 9), absent when nothing needs the operator.
    pub needs_strip: Option<NeedsStrip>,
    /// Non-empty status groups in design order, after filtering.
    pub groups: Vec<TaskGroup>,
    /// The filter state the view was built with.
    pub filters: TaskFilters,
    /// The empty state when no rows match.
    pub empty: Option<TasksEmpty>,
}

impl TasksBoardView {
    /// Build the board from current state under a filter.
    #[must_use]
    pub fn build(app: &App, filters: &TaskFilters) -> Self {
        let all = app.all_tasks();
        let counts = BoardCounts {
            in_progress: count(&all, TaskStatus::InProgress),
            blocked: count(&all, TaskStatus::Blocked),
            todo: count(&all, TaskStatus::Todo),
            done: count(&all, TaskStatus::Done),
            attention: all.iter().filter(|t| t.attention.is_some()).count(),
        };

        let groups: Vec<TaskGroup> = TASK_GROUPS
            .into_iter()
            .filter_map(|(status, label)| {
                let rows: Vec<TaskRow> = all
                    .iter()
                    .filter(|t| t.task.status == status && filters.matches(t))
                    .map(TaskRow::from_aggregate)
                    .collect();
                (!rows.is_empty()).then_some(TaskGroup {
                    status,
                    label,
                    count: rows.len(),
                    rows,
                })
            })
            .collect();

        let empty = groups.is_empty().then_some(TasksEmpty {
            title: "No tasks match",
            body: "Try clearing filters or search across the fleet.",
            can_clear: filters.is_on(),
        });

        Self {
            title: "Tasks",
            counts,
            needs_strip: NeedsStrip::build(app),
            groups,
            filters: filters.clone(),
            empty,
        }
    }

    /// The header stats joined for rendering, blocked omitted when zero (prototype
    /// `sh-stats`): e.g. "3 in progress · 1 blocked · 4 to do · 2 done".
    #[must_use]
    pub fn stats_line(&self) -> String {
        let c = self.counts;
        [
            Some(format!("{} in progress", c.in_progress)),
            (c.blocked > 0).then(|| format!("{} blocked", c.blocked)),
            Some(format!("{} to do", c.todo)),
            Some(format!("{} done", c.done)),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ")
    }
}

fn count(all: &[AggregateTask], status: TaskStatus) -> usize {
    all.iter().filter(|t| t.task.status == status).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use daedalus_proto::SessionStatus;
    use daedalus_tests::Fixture;

    /// Two sessions: `claude` with T001 in progress + T002 todo; `goose` with T001 done +
    /// T002 blocked.
    async fn two_sessions(fx: &Fixture) -> (daedalus_proto::SessionId, daedalus_proto::SessionId) {
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
    async fn groups_carry_design_labels_counts_and_rows_in_board_order() {
        let fx = Fixture::new();
        let (a, b) = two_sessions(&fx).await;

        let view = TasksBoardView::build(&fx.app, &TaskFilters::default());
        assert_eq!(view.title, "Tasks");

        // Grouped by task status in design order, with per-group counts; empty groups
        // are skipped in the grouped list (prototype `TaskListGrouped`).
        let labels: Vec<(&str, usize)> = view.groups.iter().map(|g| (g.label, g.count)).collect();
        assert_eq!(
            labels,
            vec![
                ("In progress", 1),
                ("Blocked", 1),
                ("To do", 1),
                ("Done", 1)
            ]
        );

        // A row: mono task id, title (id token stripped), sub-line "spec · tool ·
        // backend", and a session-ref chip carrying objective + session status.
        let row = &view.groups[0].rows[0];
        assert_eq!(row.task_id, "T001");
        assert_eq!(row.title, "Wire middleware");
        assert!(
            row.sub_line.contains(" · claude · fake"),
            "{}",
            row.sub_line
        );
        assert_eq!(row.session_ref.session, a);
        assert_eq!(row.session_ref.objective, "sample objective");
        assert_eq!(row.session_ref.status, SessionStatus::Running);
        assert!(!row.session_ref.attention);
        assert!(row.needs.is_none());

        // Header stats: counts over the whole board.
        assert_eq!(view.counts.in_progress, 1);
        assert_eq!(view.counts.blocked, 1);
        assert_eq!(view.counts.todo, 1);
        assert_eq!(view.counts.done, 1);
        assert_eq!(
            view.stats_line(),
            "1 in progress · 1 blocked · 1 to do · 1 done"
        );

        // Blocked task rows point at their session.
        let blocked = &view.groups[1].rows[0];
        assert_eq!(blocked.session_ref.session, b);
        assert_eq!(blocked.status, TaskStatus::Blocked);

        // Activation opens the session (plain open, no answer focus).
        assert_eq!(row.open().session, a);
        assert_eq!(row.open().focus, None);
    }

    #[tokio::test]
    async fn filters_narrow_by_status_session_tool_backend_text_and_attention() {
        let fx = Fixture::new();
        let (a, _b) = two_sessions(&fx).await;

        // Status chips.
        let view = TasksBoardView::build(
            &fx.app,
            &TaskFilters {
                statuses: vec![TaskStatus::Blocked],
                ..TaskFilters::default()
            },
        );
        assert_eq!(view.groups.len(), 1);
        assert_eq!(view.groups[0].label, "Blocked");

        // Session select.
        let view = TasksBoardView::build(
            &fx.app,
            &TaskFilters {
                session: Some(a),
                ..TaskFilters::default()
            },
        );
        assert_eq!(view.groups.iter().map(|g| g.count).sum::<usize>(), 2);

        // Tool + backend selects.
        let view = TasksBoardView::build(
            &fx.app,
            &TaskFilters {
                tool: Some("goose".into()),
                backend: Some(daedalus_proto::BackendKind::Fake),
                ..TaskFilters::default()
            },
        );
        assert_eq!(view.groups.iter().map(|g| g.count).sum::<usize>(), 2);

        // Text query matches description/id/objective.
        let view = TasksBoardView::build(
            &fx.app,
            &TaskFilters {
                query: "load test".into(),
                ..TaskFilters::default()
            },
        );
        assert_eq!(view.groups.iter().map(|g| g.count).sum::<usize>(), 1);

        // Nothing matches ⇒ the design empty state with a clear-filters affordance.
        let view = TasksBoardView::build(
            &fx.app,
            &TaskFilters {
                query: "zzz-no-such".into(),
                ..TaskFilters::default()
            },
        );
        assert!(view.groups.is_empty());
        let empty = view.empty.expect("empty state");
        assert_eq!(empty.title, "No tasks match");
        assert_eq!(
            empty.body,
            "Try clearing filters or search across the fleet."
        );
        assert!(empty.can_clear, "filters are on — offer Clear filters");

        // The full board has no empty state and no clear affordance.
        let view = TasksBoardView::build(&fx.app, &TaskFilters::default());
        assert!(view.empty.is_none());
        assert!(!TaskFilters::default().is_on());
    }

    #[tokio::test]
    async fn blocked_sessions_flag_their_rows_and_raise_the_needs_strip() {
        let fx = Fixture::new();
        let (a, _b) = two_sessions(&fx).await;

        // Nothing blocked yet: no strip, no needs tags, attention count 0.
        let view = TasksBoardView::build(&fx.app, &TaskFilters::default());
        assert!(view.needs_strip.is_none());
        assert_eq!(view.counts.attention, 0);

        fx.core
            .mark_waiting_for_input(a, "Continue? (y/n)")
            .unwrap();

        let view = TasksBoardView::build(&fx.app, &TaskFilters::default());
        // The strip (Phase 9's NeedsStrip) rides on top of the board.
        let strip = view.needs_strip.as_ref().expect("needs strip");
        assert_eq!(strip.headline, "1 session needs you");

        // a's in-progress row carries the needs tag (prototype: doing/blocked rows of a
        // blocked session); its session-ref chip renders in the attention treatment.
        let doing = &view.groups[0].rows[0];
        assert_eq!(doing.needs, Some(AttentionKind::WaitingForInput));
        assert!(doing.session_ref.attention);
        // The attention chip count ("Needs attention (n)") counts flagged tasks.
        assert_eq!(view.counts.attention, 2);

        // The "Needs attention" chip narrows to flagged rows only.
        let view = TasksBoardView::build(
            &fx.app,
            &TaskFilters {
                attention_only: true,
                ..TaskFilters::default()
            },
        );
        assert!(view
            .groups
            .iter()
            .flat_map(|g| &g.rows)
            .all(|r| r.session_ref.session == a));
    }
}
