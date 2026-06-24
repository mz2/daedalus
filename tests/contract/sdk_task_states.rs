//! Contract test for `WorkshopSdk::task_states` parsing SpecKit `tasks.md` (C-S1).

use std::time::Instant;

use daedalus_proto::{SessionId, TaskId, TaskStatus, Timestamp};
use daedalus_sdk::parse_task_states;

#[test]
fn c_s1_parses_checkbox_state_quickly() {
    let md = "\
## Phase 1
- [x] T001 Create the workspace
- [ ] T002 Add dependencies
- [~] T003 Configure clippy
- [!] T004 Blocked task
";
    let started = Instant::now();
    let tasks = parse_task_states(md, SessionId::new(), Timestamp::from_millis(0));
    assert!(
        started.elapsed().as_secs() < 5,
        "well within the SC-010 budget"
    );

    assert_eq!(tasks.len(), 4);
    assert_eq!(tasks[0].id, TaskId::new("T001"));
    assert_eq!(tasks[0].status, TaskStatus::Done);
    assert_eq!(tasks[1].status, TaskStatus::Todo);
    assert_eq!(tasks[2].status, TaskStatus::InProgress);
    assert_eq!(tasks[3].status, TaskStatus::Blocked);
}

#[test]
fn reflects_a_checkbox_flip() {
    let before = parse_task_states(
        "- [ ] T001 do it",
        SessionId::new(),
        Timestamp::from_millis(0),
    );
    let after = parse_task_states(
        "- [x] T001 do it",
        SessionId::new(),
        Timestamp::from_millis(0),
    );
    assert_eq!(before[0].status, TaskStatus::Todo);
    assert_eq!(after[0].status, TaskStatus::Done);
}
