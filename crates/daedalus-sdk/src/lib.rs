//! The in-Workshop SDK: it runs inside a Workshop environment, advertises a connectable
//! service, and exposes the session's SDD artifact location so the task board can read
//! status (FR-012, FR-017; contract `contracts/discovery-and-sdk.md`).
//!
//! The task-state parser is a pure function so it is shared verbatim by the core's task
//! board (T031) and exercised test-first (C-S1).

use serde::{Deserialize, Serialize};

use daedalus_proto::{
    ArtifactRef, SessionId, SessionIdentity, TaskId, TaskStatus, Timestamp, TrackedTask,
};

/// How Daedalus attaches to an advertised session (via zellij over the tunnel).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectInfo {
    /// The named zellij session to attach to.
    pub zellij_session: String,
    /// A human label for the host/environment.
    pub host_label: String,
}

/// What a Workshop environment advertises about its session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Advertisement {
    /// Stable identity for cross-source de-duplication (FR-013).
    pub session_identity: SessionIdentity,
    /// How Daedalus attaches.
    pub connect: ConnectInfo,
    /// Workspace location of the SDD artifacts (e.g. `tasks.md`).
    pub artifacts: ArtifactRef,
}

/// The SDK service contract.
pub trait WorkshopSdk {
    /// Advertise this session's identity, connect info, and artifact location.
    fn advertise(&self) -> Advertisement;

    /// Task states parsed from SDD artifacts (e.g. SpecKit `tasks.md` checkbox state) —
    /// never inferred from output (FR-017, Principle IV).
    fn task_states(&self) -> Vec<TrackedTask>;
}

/// Parse SpecKit-style `tasks.md` checkbox state into tracked tasks (FR-017, C-S1).
///
/// Recognised markers (case-insensitive for `x`):
/// - `- [ ]` → [`TaskStatus::Todo`]
/// - `- [x]` / `- [X]` → [`TaskStatus::Done`]
/// - `- [~]` / `- [-]` → [`TaskStatus::InProgress`]
/// - `- [!]` → [`TaskStatus::Blocked`]
///
/// A leading `Txxx` token in the description is used as the task id; otherwise the task is
/// numbered by position. The `*` bullet is accepted as well as `-`.
#[must_use]
pub fn parse_task_states(
    content: &str,
    session_id: SessionId,
    observed_at: Timestamp,
) -> Vec<TrackedTask> {
    let mut tasks = Vec::new();
    let mut fallback_index = 0u32;

    for raw in content.lines() {
        let line = raw.trim_start();
        let Some(rest) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) else {
            continue;
        };
        let rest = rest.trim_start();
        if !rest.starts_with('[') {
            continue;
        }
        // Expect `[m]` where m is a single marker char.
        let bytes = rest.as_bytes();
        if bytes.len() < 3 || bytes[2] != b']' {
            continue;
        }
        let marker = bytes[1] as char;
        let status = match marker {
            ' ' => TaskStatus::Todo,
            'x' | 'X' => TaskStatus::Done,
            '~' | '-' => TaskStatus::InProgress,
            '!' => TaskStatus::Blocked,
            _ => continue,
        };

        let description = rest[3..].trim().to_string();
        let id = extract_task_id(&description).unwrap_or_else(|| {
            fallback_index += 1;
            format!("task-{fallback_index}")
        });

        tasks.push(TrackedTask {
            id: TaskId::new(id),
            session_id,
            description,
            status,
            updated_at: observed_at,
        });
    }

    tasks
}

/// Pull a leading SpecKit task id like `T001` or `T017a` from a description.
fn extract_task_id(description: &str) -> Option<String> {
    let token = description.split_whitespace().next()?;
    let mut chars = token.chars();
    if chars.next()? != 'T' {
        return None;
    }
    let tail: String = chars.collect();
    // Must start with at least one digit (e.g. T001, T017a); reject "The", "Test", etc.
    if tail.chars().next().is_some_and(|c| c.is_ascii_digit())
        && tail.chars().all(|c| c.is_ascii_alphanumeric())
    {
        Some(token.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_marker_kinds_and_ids() {
        let md = "\
# Tasks
- [x] T001 Create the workspace
- [ ] T002 Add dependencies
- [~] T003 Configure clippy
- [!] T004 Blocked on upstream
Some prose that is not a task.
* [X] T005 Bullet-star done
";
        let session = SessionId::new();
        let tasks = parse_task_states(md, session, Timestamp::from_millis(0));
        assert_eq!(tasks.len(), 5);
        assert_eq!(tasks[0].id, TaskId::new("T001"));
        assert_eq!(tasks[0].status, TaskStatus::Done);
        assert_eq!(tasks[1].status, TaskStatus::Todo);
        assert_eq!(tasks[2].status, TaskStatus::InProgress);
        assert_eq!(tasks[3].status, TaskStatus::Blocked);
        assert_eq!(tasks[4].status, TaskStatus::Done);
        assert!(tasks.iter().all(|t| t.session_id == session));
    }

    #[test]
    fn falls_back_to_positional_ids() {
        let md = "- [ ] do a thing\n- [x] do another";
        let tasks = parse_task_states(md, SessionId::new(), Timestamp::from_millis(0));
        assert_eq!(tasks[0].id, TaskId::new("task-1"));
        assert_eq!(tasks[1].id, TaskId::new("task-2"));
    }
}
