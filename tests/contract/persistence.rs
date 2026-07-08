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
fn backend_sandbox_image_is_persisted_operator_configuration() {
    // The OpenShell `--from` image is app-owned configuration (Settings), not a shell
    // environment variable: it round-trips through the store, survives reopen, and a
    // blank value clears back to the backend default.
    use daedalus_proto::BackendKind;

    let dir = TempDir::new().unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(store.backend_image(BackendKind::OpenShell).unwrap(), None);

    store
        .set_backend_image(
            BackendKind::OpenShell,
            Some("daedalus/e2e-openshell:latest"),
        )
        .unwrap();
    drop(store);
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(
        store
            .backend_image(BackendKind::OpenShell)
            .unwrap()
            .as_deref(),
        Some("daedalus/e2e-openshell:latest")
    );

    store
        .set_backend_image(BackendKind::OpenShell, Some("  "))
        .unwrap();
    assert_eq!(store.backend_image(BackendKind::OpenShell).unwrap(), None);
}

#[test]
fn concurrency_limit_and_stall_interval_survive_core_restart() {
    // G6+G15: the concurrency limit and stall interval are persisted operator
    // configuration — like idle rates, they survive a Core rebuild over the same store
    // (reseeded in `Core::new`).
    use std::sync::Arc;

    use daedalus_backend::Backend;
    use daedalus_backend_fake::FakeBackend;
    use daedalus_core::{BackendRegistry, Core, CoreConfig};
    use daedalus_zellij::InMemoryTerminal;

    let dir = TempDir::new().unwrap();
    let build = || {
        let store = Arc::new(Store::open(dir.path()).unwrap());
        let registry = BackendRegistry::new(vec![Arc::new(FakeBackend::new()) as Arc<dyn Backend>]);
        Core::new(
            store,
            registry,
            Arc::new(InMemoryTerminal::new()),
            None,
            CoreConfig::default(),
        )
    };

    {
        let core = build();
        core.set_concurrency_limit(Some(5)).unwrap();
        core.set_stall_interval(45).unwrap();
        let cfg = core.config();
        assert_eq!(cfg.concurrency_limit, Some(5));
        assert_eq!(cfg.stall_interval_secs, 45);
    }

    // Rebuild over the same store: both survive.
    let core = build();
    let cfg = core.config();
    assert_eq!(
        cfg.concurrency_limit,
        Some(5),
        "concurrency limit persisted"
    );
    assert_eq!(cfg.stall_interval_secs, 45, "stall interval persisted");
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

/// P1: an interrupted v1→v2 upgrade leaves some of the new columns already added. Re-running
/// the migration must not brick `Store::open` on a "duplicate column" error — the ALTER
/// steps are idempotent and the run finishes the migration.
#[test]
fn partially_applied_migration_completes_without_bricking() {
    let dir = TempDir::new().unwrap();
    let session_id = SessionId::new();
    {
        // v1-shaped sessions table, but with ONE of the v2 columns already added (as if a
        // previous upgrade was interrupted mid-way), still recorded as schema_version 1.
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
                 accepts_input INTEGER NOT NULL,
                 pending_prompt TEXT
             );
             INSERT INTO meta (key, value) VALUES ('schema_version', '1');",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sessions (id, tool_id, objective_id, environment_id, source_id, status, created_at, accepts_input) VALUES (?1,?2,?3,?4,?5,'running',1,1)",
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

    // Re-running the migration must succeed and finish the schema, not error on the
    // already-present column.
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(store.schema_version().unwrap(), 3);
    let session = store.get_session(session_id).unwrap();
    assert_eq!(session.status, SessionStatus::Running);
    // The remaining v2 columns were added by the completed migration.
    assert_eq!(session.waiting_since, None);
    assert_eq!(session.last_known_status, None);
}

/// P2: a present-but-unparseable `schema_version` must never be silently treated as the
/// current version (which would skip migration and leave the DB missing columns). It is a
/// clear error instead.
#[test]
fn unparseable_schema_version_is_an_error_not_silently_current() {
    let dir = TempDir::new().unwrap();
    {
        // v1-shaped sessions table (missing the US6 columns) but with a garbage version.
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
             INSERT INTO meta (key, value) VALUES ('schema_version', 'garbage');",
        )
        .unwrap();
    }
    // Must not silently succeed while leaving the US6 columns unmigrated.
    assert!(Store::open(dir.path()).is_err());
}

/// P3: a single undecodable row (bad status token) must be skipped with a warning, not
/// panic the whole list query and poison the connection mutex.
#[test]
fn list_sessions_survives_one_corrupt_row() {
    let dir = TempDir::new().unwrap();
    let store = Store::open(dir.path()).unwrap();

    // One good session through the normal path.
    let good = attention_session();
    store.upsert_session(&good).unwrap();

    // One corrupt row (invalid status enum token) written directly.
    let bad_id = SessionId::new();
    {
        let conn = rusqlite::Connection::open(dir.path().join("daedalus.sqlite")).unwrap();
        conn.execute(
            "INSERT INTO sessions (id, tool_id, objective_id, environment_id, source_id, status, created_at, accepts_input) VALUES (?1,?2,?3,?4,?5,'bogus_status',7,1)",
            (
                bad_id.to_string(),
                ToolId::new().to_string(),
                ObjectiveId::new().to_string(),
                EnvironmentId::new().to_string(),
                SourceId::new().to_string(),
            ),
        )
        .unwrap();
    }

    // Does not panic; returns the good row and skips the corrupt one.
    let sessions = store.list_sessions().unwrap();
    assert!(sessions.iter().any(|s| s.id == good.id));
    assert!(sessions.iter().all(|s| s.id != bad_id));

    // Single-row lookups return a decode error (not a panic) for the corrupt row.
    assert!(store.get_session(bad_id).is_err());
}

/// D6: deleting a session unlinks its capture `.log` file and resets the line counter, so
/// no orphaned capture files or stale counters are left behind.
#[test]
fn delete_session_removes_capture_file_and_line_counter() {
    let dir = TempDir::new().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let session = SessionId::new();
    store
        .append_output(session, Timestamp::from_millis(1), b"line one\nline two\n")
        .unwrap();
    // Initialise the line counter.
    assert_eq!(store.capture_line_count(session), 2);
    let capture = dir.path().join("captures").join(format!("{session}.log"));
    assert!(capture.exists());

    store.delete_session(session).unwrap();
    assert!(!capture.exists(), "capture .log unlinked on delete");
    assert_eq!(store.capture_line_count(session), 0, "line counter reset");
}
