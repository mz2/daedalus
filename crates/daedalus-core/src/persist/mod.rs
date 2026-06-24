//! SQLite persistence + file-backed output capture (`data-model.md`, research R6).
//!
//! Tables mirror the entities; high-volume terminal output is written to per-session
//! capture files and referenced from the `events` table rather than inlined (the
//! excessive-output edge case). All records are retained until the operator deletes them —
//! no auto-expiry (FR-030a).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::Connection;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use thiserror::Error;

use daedalus_proto::{
    AgenticTool, Availability, BackendId, BackendKind, EnvLifecycle, EnvironmentId, EventId,
    EventKind, EventPayload, EventRecord, MetricKind, Objective, ObjectiveId, Origin,
    ResourceUsageMetric, SandboxEnvironment, Session, SessionId, SessionStatus, Source, SourceId,
    TaskId, TaskStatus, Timestamp, ToolId, TrackedTask, WorktreeRef,
};

/// Persistence errors.
#[derive(Debug, Error)]
pub enum StoreError {
    /// Underlying SQLite failure.
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// JSON (de)serialisation of a stored value failed.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    /// Capture-file I/O failure.
    #[error("capture i/o error: {0}")]
    Io(#[from] std::io::Error),
    /// A referenced row was not found.
    #[error("not found")]
    NotFound,
}

/// The current schema version (bumped when migrations change).
const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);

CREATE TABLE IF NOT EXISTS tools (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    invocation TEXT NOT NULL,
    capabilities TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS objectives (
    id TEXT PRIMARY KEY,
    artifact_root TEXT NOT NULL,
    tasks_file TEXT NOT NULL,
    description TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS backends (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    availability TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sources (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    availability TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS environments (
    id TEXT PRIMARY KEY,
    backend_id TEXT NOT NULL,
    origin TEXT NOT NULL,
    worktree TEXT,
    lifecycle TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sessions (
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

CREATE TABLE IF NOT EXISTS tasks (
    session_id TEXT NOT NULL,
    task_id TEXT NOT NULL,
    description TEXT NOT NULL,
    status TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (session_id, task_id)
);

CREATE TABLE IF NOT EXISTS events (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    timestamp INTEGER NOT NULL,
    kind TEXT NOT NULL,
    payload TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_events_session ON events (session_id, timestamp);

CREATE TABLE IF NOT EXISTS metrics (
    session_id TEXT NOT NULL,
    metric TEXT NOT NULL,
    value REAL NOT NULL,
    timestamp INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_metrics_session ON metrics (session_id, timestamp);
"#;

/// The persistent store: a SQLite connection plus a captures directory.
pub struct Store {
    conn: Mutex<Connection>,
    captures_dir: PathBuf,
}

fn enum_to_text<T: Serialize>(v: &T) -> Result<String, StoreError> {
    match serde_json::to_value(v)? {
        Value::String(s) => Ok(s),
        other => Ok(other.to_string()),
    }
}

fn text_to_enum<T: DeserializeOwned>(s: &str) -> Result<T, StoreError> {
    Ok(serde_json::from_value(Value::String(s.to_string()))?)
}

impl Store {
    /// Open (creating if needed) a store rooted at `dir`: `dir/daedalus.sqlite` plus a
    /// `dir/captures/` directory for output capture files.
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, StoreError> {
        let dir = dir.as_ref();
        std::fs::create_dir_all(dir)?;
        let captures_dir = dir.join("captures");
        std::fs::create_dir_all(&captures_dir)?;
        let conn = Connection::open(dir.join("daedalus.sqlite"))?;
        let store = Self {
            conn: Mutex::new(conn),
            captures_dir,
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        conn.execute_batch(SCHEMA)?;
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', ?1)",
            [SCHEMA_VERSION.to_string()],
        )?;
        Ok(())
    }

    /// The recorded schema version (used by reconciliation / future migrations).
    pub fn schema_version(&self) -> Result<i64, StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        let v: String = conn.query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )?;
        Ok(v.parse().unwrap_or(0))
    }

    // ----- Tools -----

    /// Insert (or replace) a registered tool.
    pub fn upsert_tool(&self, tool: &AgenticTool) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        conn.execute(
            "INSERT OR REPLACE INTO tools (id, name, invocation, capabilities) VALUES (?1,?2,?3,?4)",
            (
                tool.id.to_string(),
                &tool.name,
                serde_json::to_string(&tool.invocation)?,
                serde_json::to_string(&tool.capabilities)?,
            ),
        )?;
        Ok(())
    }

    /// Whether a tool with this name already exists.
    pub fn tool_name_exists(&self, name: &str) -> Result<bool, StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM tools WHERE name = ?1", [name], |r| {
                r.get(0)
            })?;
        Ok(count > 0)
    }

    /// Fetch a tool by id.
    pub fn get_tool(&self, id: ToolId) -> Result<AgenticTool, StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        conn.query_row(
            "SELECT id, name, invocation, capabilities FROM tools WHERE id = ?1",
            [id.to_string()],
            |r| {
                Ok(AgenticTool {
                    id,
                    name: r.get(1)?,
                    invocation: serde_json::from_str(&r.get::<_, String>(2)?)
                        .expect("valid invocation json"),
                    capabilities: serde_json::from_str(&r.get::<_, String>(3)?)
                        .expect("valid capabilities json"),
                })
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => StoreError::NotFound,
            other => other.into(),
        })
    }

    /// List all registered tools.
    pub fn list_tools(&self) -> Result<Vec<AgenticTool>, StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        let mut stmt =
            conn.prepare("SELECT id, name, invocation, capabilities FROM tools ORDER BY name")?;
        let rows = stmt.query_map([], |r| {
            let id: String = r.get(0)?;
            Ok(AgenticTool {
                id: ToolId::from_uuid(id.parse().expect("uuid")),
                name: r.get(1)?,
                invocation: serde_json::from_str(&r.get::<_, String>(2)?)
                    .expect("valid invocation json"),
                capabilities: serde_json::from_str(&r.get::<_, String>(3)?)
                    .expect("valid capabilities json"),
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    // ----- Objectives / backends / sources / environments -----

    /// Insert (or replace) an objective.
    pub fn upsert_objective(&self, obj: &Objective) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        conn.execute(
            "INSERT OR REPLACE INTO objectives (id, artifact_root, tasks_file, description) VALUES (?1,?2,?3,?4)",
            (
                obj.id.to_string(),
                &obj.artifact_ref.root,
                &obj.artifact_ref.tasks_file,
                &obj.description,
            ),
        )?;
        Ok(())
    }

    /// Fetch an objective by id.
    pub fn get_objective(&self, id: ObjectiveId) -> Result<Objective, StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        conn.query_row(
            "SELECT artifact_root, tasks_file, description FROM objectives WHERE id = ?1",
            [id.to_string()],
            |r| {
                Ok(Objective {
                    id,
                    artifact_ref: daedalus_proto::ArtifactRef {
                        root: r.get(0)?,
                        tasks_file: r.get(1)?,
                    },
                    description: r.get(2)?,
                })
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => StoreError::NotFound,
            other => other.into(),
        })
    }

    /// Insert (or replace) a backend record.
    pub fn upsert_backend(
        &self,
        id: BackendId,
        kind: BackendKind,
        availability: Availability,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        conn.execute(
            "INSERT OR REPLACE INTO backends (id, kind, availability) VALUES (?1,?2,?3)",
            (
                id.to_string(),
                enum_to_text(&kind)?,
                enum_to_text(&availability)?,
            ),
        )?;
        Ok(())
    }

    /// Insert (or replace) a source record.
    pub fn upsert_source(&self, source: &Source) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        conn.execute(
            "INSERT OR REPLACE INTO sources (id, kind, availability) VALUES (?1,?2,?3)",
            (
                source.id.to_string(),
                enum_to_text(&source.kind)?,
                enum_to_text(&source.availability)?,
            ),
        )?;
        Ok(())
    }

    /// Insert (or replace) an environment.
    pub fn upsert_environment(&self, env: &SandboxEnvironment) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        conn.execute(
            "INSERT OR REPLACE INTO environments (id, backend_id, origin, worktree, lifecycle) VALUES (?1,?2,?3,?4,?5)",
            (
                env.id.to_string(),
                env.backend_id.to_string(),
                enum_to_text(&env.origin)?,
                env.worktree_ref
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?,
                enum_to_text(&env.lifecycle)?,
            ),
        )?;
        Ok(())
    }

    /// Fetch an environment by id.
    pub fn get_environment(&self, id: EnvironmentId) -> Result<SandboxEnvironment, StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        conn.query_row(
            "SELECT backend_id, origin, worktree, lifecycle FROM environments WHERE id = ?1",
            [id.to_string()],
            |r| {
                let worktree: Option<String> = r.get(2)?;
                Ok(SandboxEnvironment {
                    id,
                    backend_id: BackendId::from_uuid(r.get::<_, String>(0)?.parse().expect("uuid")),
                    origin: text_to_enum::<Origin>(&r.get::<_, String>(1)?).expect("origin"),
                    worktree_ref: worktree
                        .map(|w| serde_json::from_str::<WorktreeRef>(&w).expect("worktree json")),
                    lifecycle: text_to_enum::<EnvLifecycle>(&r.get::<_, String>(3)?)
                        .expect("lifecycle"),
                })
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => StoreError::NotFound,
            other => other.into(),
        })
    }

    /// Update an environment's lifecycle.
    pub fn set_environment_lifecycle(
        &self,
        id: EnvironmentId,
        lifecycle: EnvLifecycle,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        conn.execute(
            "UPDATE environments SET lifecycle = ?2 WHERE id = ?1",
            (id.to_string(), enum_to_text(&lifecycle)?),
        )?;
        Ok(())
    }

    // ----- Sessions -----

    /// Insert (or replace) a session record.
    pub fn upsert_session(&self, s: &Session) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        conn.execute(
            "INSERT OR REPLACE INTO sessions
             (id, tool_id, objective_id, environment_id, source_id, status, created_at, started_at, ended_at, terminal_outcome, accepts_input)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            (
                s.id.to_string(),
                s.tool_id.to_string(),
                s.objective_id.to_string(),
                s.environment_id.to_string(),
                s.source_id.to_string(),
                enum_to_text(&s.status)?,
                s.created_at.millis(),
                s.started_at.map(|t| t.millis()),
                s.ended_at.map(|t| t.millis()),
                s.terminal_outcome.clone(),
                i64::from(s.accepts_input),
            ),
        )?;
        Ok(())
    }

    /// Fetch a session by id.
    pub fn get_session(&self, id: SessionId) -> Result<Session, StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        conn.query_row(
            "SELECT tool_id, objective_id, environment_id, source_id, status, created_at, started_at, ended_at, terminal_outcome, accepts_input FROM sessions WHERE id = ?1",
            [id.to_string()],
            |r| Ok(row_to_session(id, r)),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => StoreError::NotFound,
            other => other.into(),
        })
    }

    /// List all sessions, newest first.
    pub fn list_sessions(&self) -> Result<Vec<Session>, StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, tool_id, objective_id, environment_id, source_id, status, created_at, started_at, ended_at, terminal_outcome, accepts_input FROM sessions ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            let id = SessionId::from_uuid(r.get::<_, String>(0)?.parse().expect("uuid"));
            // Shift column indices by one because id is column 0 here.
            Ok(Session {
                id,
                tool_id: ToolId::from_uuid(r.get::<_, String>(1)?.parse().expect("uuid")),
                objective_id: ObjectiveId::from_uuid(r.get::<_, String>(2)?.parse().expect("uuid")),
                environment_id: EnvironmentId::from_uuid(
                    r.get::<_, String>(3)?.parse().expect("uuid"),
                ),
                source_id: SourceId::from_uuid(r.get::<_, String>(4)?.parse().expect("uuid")),
                status: text_to_enum::<SessionStatus>(&r.get::<_, String>(5)?).expect("status"),
                created_at: Timestamp::from_millis(r.get(6)?),
                started_at: r.get::<_, Option<i64>>(7)?.map(Timestamp::from_millis),
                ended_at: r.get::<_, Option<i64>>(8)?.map(Timestamp::from_millis),
                terminal_outcome: r.get(9)?,
                accepts_input: r.get::<_, i64>(10)? != 0,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Update a session's status (and optionally an outcome/end timestamp).
    pub fn set_session_status(
        &self,
        id: SessionId,
        status: SessionStatus,
        ended_at: Option<Timestamp>,
        outcome: Option<&str>,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        conn.execute(
            "UPDATE sessions SET status = ?2, ended_at = COALESCE(?3, ended_at), terminal_outcome = COALESCE(?4, terminal_outcome) WHERE id = ?1",
            (
                id.to_string(),
                enum_to_text(&status)?,
                ended_at.map(|t| t.millis()),
                outcome,
            ),
        )?;
        Ok(())
    }

    /// Record that a session started running at `at`.
    pub fn set_session_started(&self, id: SessionId, at: Timestamp) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        conn.execute(
            "UPDATE sessions SET started_at = ?2 WHERE id = ?1",
            (id.to_string(), at.millis()),
        )?;
        Ok(())
    }

    /// Delete a session and all its child records (FR-030a, operator-initiated).
    pub fn delete_session(&self, id: SessionId) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        let sid = id.to_string();
        conn.execute("DELETE FROM tasks WHERE session_id = ?1", [&sid])?;
        conn.execute("DELETE FROM events WHERE session_id = ?1", [&sid])?;
        conn.execute("DELETE FROM metrics WHERE session_id = ?1", [&sid])?;
        conn.execute("DELETE FROM sessions WHERE id = ?1", [&sid])?;
        Ok(())
    }

    // ----- Tasks -----

    /// Insert or update a tracked task's status.
    pub fn upsert_task(&self, task: &TrackedTask) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        conn.execute(
            "INSERT OR REPLACE INTO tasks (session_id, task_id, description, status, updated_at) VALUES (?1,?2,?3,?4,?5)",
            (
                task.session_id.to_string(),
                &task.id.0,
                &task.description,
                enum_to_text(&task.status)?,
                task.updated_at.millis(),
            ),
        )?;
        Ok(())
    }

    /// List a session's tracked tasks in id order.
    pub fn list_tasks(&self, session: SessionId) -> Result<Vec<TrackedTask>, StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        let mut stmt = conn.prepare(
            "SELECT task_id, description, status, updated_at FROM tasks WHERE session_id = ?1 ORDER BY task_id",
        )?;
        let rows = stmt.query_map([session.to_string()], |r| {
            Ok(TrackedTask {
                id: TaskId::new(r.get::<_, String>(0)?),
                session_id: session,
                description: r.get(1)?,
                status: text_to_enum::<TaskStatus>(&r.get::<_, String>(2)?).expect("status"),
                updated_at: Timestamp::from_millis(r.get(3)?),
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    // ----- Events + capture files -----

    /// Append redacted output bytes to the session's capture file and record an
    /// [`EventPayload::Output`] referencing the written span (FR-018).
    pub fn append_output(
        &self,
        session: SessionId,
        timestamp: Timestamp,
        redacted: &[u8],
    ) -> Result<EventRecord, StoreError> {
        use std::io::Write;
        let path = self.captures_dir.join(format!("{session}.log"));
        let offset = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        file.write_all(redacted)?;

        let event = EventRecord {
            id: EventId::new(),
            session_id: session,
            timestamp,
            kind: EventKind::Output,
            payload: EventPayload::Output {
                capture_file: path.to_string_lossy().into_owned(),
                offset,
                len: redacted.len() as u64,
            },
        };
        self.insert_event(&event)?;
        Ok(event)
    }

    /// Insert a pre-built event record.
    pub fn insert_event(&self, event: &EventRecord) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        conn.execute(
            "INSERT OR REPLACE INTO events (id, session_id, timestamp, kind, payload) VALUES (?1,?2,?3,?4,?5)",
            (
                event.id.to_string(),
                event.session_id.to_string(),
                event.timestamp.millis(),
                enum_to_text(&event.kind)?,
                serde_json::to_string(&event.payload)?,
            ),
        )?;
        Ok(())
    }

    /// List a session's events in time order.
    pub fn list_events(&self, session: SessionId) -> Result<Vec<EventRecord>, StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, kind, payload FROM events WHERE session_id = ?1 ORDER BY timestamp, id",
        )?;
        let rows = stmt.query_map([session.to_string()], |r| {
            Ok(EventRecord {
                id: EventId::from_uuid(r.get::<_, String>(0)?.parse().expect("uuid")),
                session_id: session,
                timestamp: Timestamp::from_millis(r.get(1)?),
                kind: text_to_enum::<EventKind>(&r.get::<_, String>(2)?).expect("kind"),
                payload: serde_json::from_str(&r.get::<_, String>(3)?).expect("payload json"),
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    // ----- Metrics -----

    /// Insert a resource-usage sample.
    pub fn insert_metric(&self, m: &ResourceUsageMetric) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        conn.execute(
            "INSERT INTO metrics (session_id, metric, value, timestamp) VALUES (?1,?2,?3,?4)",
            (
                m.session_id.to_string(),
                enum_to_text(&m.metric)?,
                m.value,
                m.timestamp.millis(),
            ),
        )?;
        Ok(())
    }

    /// Latest sample per metric kind for a session.
    pub fn latest_metrics(
        &self,
        session: SessionId,
    ) -> Result<Vec<ResourceUsageMetric>, StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        let mut stmt = conn.prepare(
            "SELECT metric, value, timestamp FROM metrics m WHERE session_id = ?1 AND timestamp = (SELECT MAX(timestamp) FROM metrics WHERE session_id = m.session_id AND metric = m.metric) GROUP BY metric",
        )?;
        let rows = stmt.query_map([session.to_string()], |r| {
            Ok(ResourceUsageMetric {
                session_id: session,
                metric: text_to_enum::<MetricKind>(&r.get::<_, String>(0)?).expect("metric"),
                value: r.get(1)?,
                timestamp: Timestamp::from_millis(r.get(2)?),
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

fn row_to_session(id: SessionId, r: &rusqlite::Row<'_>) -> Session {
    Session {
        id,
        tool_id: ToolId::from_uuid(r.get::<_, String>(0).expect("col").parse().expect("uuid")),
        objective_id: ObjectiveId::from_uuid(
            r.get::<_, String>(1).expect("col").parse().expect("uuid"),
        ),
        environment_id: EnvironmentId::from_uuid(
            r.get::<_, String>(2).expect("col").parse().expect("uuid"),
        ),
        source_id: SourceId::from_uuid(r.get::<_, String>(3).expect("col").parse().expect("uuid")),
        status: text_to_enum::<SessionStatus>(&r.get::<_, String>(4).expect("col"))
            .expect("status"),
        created_at: Timestamp::from_millis(r.get(5).expect("col")),
        started_at: r
            .get::<_, Option<i64>>(6)
            .expect("col")
            .map(Timestamp::from_millis),
        ended_at: r
            .get::<_, Option<i64>>(7)
            .expect("col")
            .map(Timestamp::from_millis),
        terminal_outcome: r.get(8).expect("col"),
        accepts_input: r.get::<_, i64>(9).expect("col") != 0,
    }
}
