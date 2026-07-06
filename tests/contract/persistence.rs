//! Contract tests for the SQLite schema + migrations + file-backed output capture (T010,
//! T069: the US6 waiting/attention session columns).

use daedalus_core::Store;
use daedalus_proto::{
    AgenticTool, Capabilities, EnvironmentId, EventPayload, InvocationSpec, ObjectiveId, Outcome,
    Session, SessionId, SessionStatus, SourceId, Timestamp, ToolId, WorkItemRef,
};
use tempfile::TempDir;

#[test]
fn schema_migrates_and_reports_version() {
    let dir = TempDir::new().unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(store.schema_version().unwrap(), 3);
    assert!(dir.path().join("daedalus.sqlite").exists());
    assert!(dir.path().join("captures").is_dir());
}

#[test]
fn backend_idle_rates_roundtrip_and_survive_reopen() {
    // FR-021b (T086): operator-configured idle rates are persisted configuration — they
    // survive a restart and are clearable.
    use daedalus_proto::BackendKind;

    let dir = TempDir::new().unwrap();
    {
        let store = Store::open(dir.path()).unwrap();
        store
            .set_backend_idle_rate(BackendKind::Fake, Some(0.60))
            .unwrap();
        store
            .set_backend_idle_rate(BackendKind::Workshop, Some(2.40))
            .unwrap();
        store
            .set_backend_idle_rate(BackendKind::Workshop, None)
            .unwrap(); // cleared
    }
    let store = Store::open(dir.path()).unwrap(); // "restart"
    let rates = store.backend_idle_rates().unwrap();
    assert_eq!(rates, vec![(BackendKind::Fake, 0.60)]);
}

#[test]
fn tools_roundtrip_and_enforce_unique_names() {
    let dir = TempDir::new().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let tool = AgenticTool {
        id: ToolId::new(),
        name: "claude".into(),
        invocation: InvocationSpec {
            program: "claude".into(),
            args: vec![],
            env: vec![],
        },
        capabilities: Capabilities {
            accepts_interactive_input: true,
            prompt_convention: None,
        },
    };
    store.upsert_tool(&tool).unwrap();
    assert!(store.tool_name_exists("claude").unwrap());
    let back = store.get_tool(tool.id).unwrap();
    assert_eq!(back.name, "claude");
}

#[test]
fn output_is_stored_as_capture_file_reference_not_inline() {
    let dir = TempDir::new().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let session = SessionId::new();
    let event = store
        .append_output(session, Timestamp::from_millis(1), b"hello world\n")
        .unwrap();
    match event.payload {
        EventPayload::Output {
            capture_file,
            offset,
            len,
        } => {
            assert_eq!(offset, 0);
            assert_eq!(len, 12);
            // The bytes live in the capture file, not in the DB row.
            let contents = std::fs::read(&capture_file).unwrap();
            assert_eq!(contents, b"hello world\n");
        }
        other => panic!("expected Output payload, got {other:?}"),
    }
    // A second append advances the offset (append-only stream).
    let event2 = store
        .append_output(session, Timestamp::from_millis(2), b"more\n")
        .unwrap();
    if let EventPayload::Output { offset, .. } = event2.payload {
        assert_eq!(offset, 12);
    }
    assert_eq!(store.list_events(session).unwrap().len(), 2);
}

fn attention_session() -> Session {
    Session {
        id: SessionId::new(),
        tool_id: ToolId::new(),
        objective_id: ObjectiveId::new(),
        environment_id: EnvironmentId::new(),
        source_id: SourceId::new(),
        status: SessionStatus::WaitingForInput,
        created_at: Timestamp::from_millis(10),
        started_at: Some(Timestamp::from_millis(11)),
        ended_at: None,
        terminal_outcome: Some(Outcome {
            reason: None,
            exit_code: Some(0),
            exit_summary: Some("stopped for review".into()),
        }),
        accepts_input: true,
        pending_prompt: Some("Continue? (y/n)".into()),
        waiting_since: Some(Timestamp::from_millis(12)),
        work_item_ref: Some(WorkItemRef {
            tracker: "github".into(),
            reference: "daedalus#42".into(),
        }),
        last_known_status: Some(SessionStatus::Running),
    }
}

#[test]
fn session_waiting_and_attention_columns_roundtrip_across_reopen() {
    let dir = TempDir::new().unwrap();
    let session = attention_session();
    {
        let store = Store::open(dir.path()).unwrap();
        store.upsert_session(&session).unwrap();
        assert_eq!(store.get_session(session.id).unwrap(), session);
    }
    // Reopen: migration is idempotent and the new columns reload intact (T069/T073).
    let store = Store::open(dir.path()).unwrap();
    let back = store.get_session(session.id).unwrap();
    assert_eq!(back, session);
    assert!(store.list_sessions().unwrap().contains(&session));
}

#[test]
fn v1_schema_upgrades_in_place_preserving_outcome_reasons() {
    let dir = TempDir::new().unwrap();
    let session_id = SessionId::new();
    {
        // Lay down a v1 database by hand: sessions without the US6 columns, plain-text
        // terminal_outcome, schema_version 1.
        let conn = rusqlite::Connection::open(dir.path().join("daedalus.sqlite")).unwrap();
        conn.execute_batch(
            "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE sessions (
                 id TEXT PRIMARY KEY,
                 tool_id TEXT NOT NULL,
                 objective_id TEXT NOT NULL,
                 environment_id TEXT NOT NULL,
                 source_id TEXT NOT NULL,
                 status TEXT NOT NULL,
                 created_at INTEGER NOT NULL,
                 started_at INTEGER,
                 ended_at INTEGER,
                 terminal_outcome TEXT,
                 accepts_input INTEGER NOT NULL
             );
             INSERT INTO meta (key, value) VALUES ('schema_version', '1');",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sessions VALUES (?1,?2,?3,?4,?5,'stopped',1,2,3,'stopped by operator',1)",
            (
                session_id.to_string(),
                ToolId::new().to_string(),
                ObjectiveId::new().to_string(),
                EnvironmentId::new().to_string(),
                SourceId::new().to_string(),
            ),
        )
        .unwrap();
    }

    let store = Store::open(dir.path()).unwrap();
    assert_eq!(store.schema_version().unwrap(), 3);
    let session = store.get_session(session_id).unwrap();
    assert_eq!(session.status, SessionStatus::Stopped);
    // The v1 reason text is preserved inside the richer Outcome.
    assert_eq!(
        session.terminal_outcome,
        Some(Outcome::reason("stopped by operator"))
    );
    assert_eq!(session.pending_prompt, None);
    assert_eq!(session.waiting_since, None);
    assert_eq!(session.work_item_ref, None);
    assert_eq!(session.last_known_status, None);
}
