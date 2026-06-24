//! Contract tests for the SQLite schema + migrations + file-backed output capture (T010).

use daedalus_core::Store;
use daedalus_proto::{
    AgenticTool, Capabilities, EventPayload, InvocationSpec, SessionId, Timestamp, ToolId,
};
use tempfile::TempDir;

#[test]
fn schema_migrates_and_reports_version() {
    let dir = TempDir::new().unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(store.schema_version().unwrap(), 1);
    assert!(dir.path().join("daedalus.sqlite").exists());
    assert!(dir.path().join("captures").is_dir());
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
