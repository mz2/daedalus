//! SQLite persistence + file-backed output capture (`data-model.md`, research R6).
//!
//! Tables mirror the entities; high-volume terminal output is written to per-session
//! capture files and referenced from the `events` table rather than inlined (the
//! excessive-output edge case). All records are retained until the operator deletes them —
//! no auto-expiry (FR-030a).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::Connection;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use thiserror::Error;

use daedalus_proto::{
    AgenticTool, Availability, BackendId, BackendKind, EnvLifecycle, EnvironmentId, EventId,
    EventKind, EventPayload, EventRecord, MetricKind, Objective, ObjectiveId, Origin, Outcome,
    ResourceUsageMetric, SandboxEnvironment, Session, SessionId, SessionStatus, Source, SourceId,
    TaskId, TaskStatus, Timestamp, ToolId, TrackedTask, WorkItemRef, WorktreeRef,
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
    /// The recorded schema version is present but not a valid version number — the store
    /// refuses to open rather than silently skip migrations (P2).
    #[error("unrecognized schema version: {0:?}")]
    CorruptSchemaVersion(String),
    /// A stored row could not be decoded (e.g. an unparseable id or enum token).
    #[error("row decode error: {0}")]
    Decode(String),
}

/// Parse a persisted UUID string, mapping a malformed value onto [`StoreError::Decode`]
/// instead of panicking (P3 resilience).
fn parse_uuid<T>(s: &str) -> Result<T, StoreError>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    s.parse()
        .map_err(|e| StoreError::Decode(format!("invalid uuid {s:?}: {e}")))
}

/// Collect a `query_map` of fallibly-decoded rows, skipping (with a warning) any single row
/// that fails to decode rather than failing — and poisoning the connection for — the whole
/// list query (P3). The `rusqlite` layer is still propagated (a genuine I/O/step failure).
fn collect_lenient<T>(
    rows: impl Iterator<Item = Result<Result<T, StoreError>, rusqlite::Error>>,
    what: &str,
) -> Result<Vec<T>, StoreError> {
    let mut out = Vec::new();
    for row in rows {
        match row? {
            Ok(value) => out.push(value),
            Err(err) => {
                tracing::warn!(target: "daedalus::persist", %err, "skipping undecodable {what} row")
            }
        }
    }
    Ok(out)
}

/// The current schema version (bumped when migrations change).
///
/// v2 (US6, T073): the `sessions` waiting/attention columns (`pending_prompt`,
/// `waiting_since`, `work_item_ref`, `last_known_status`) and `terminal_outcome` stored as
/// a JSON [`Outcome`] instead of plain reason text.
///
/// v3 (T086, FR-021b): the `config` key-value table holding operator settings — currently
/// the per-backend idle rates. New table only, so `CREATE TABLE IF NOT EXISTS` covers the
/// v2 → v3 upgrade with no ALTER steps.
const SCHEMA_VERSION: i64 = 3;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);

CREATE TABLE IF NOT EXISTS config (key TEXT PRIMARY KEY, value TEXT NOT NULL);

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
    accepts_input INTEGER NOT NULL,
    pending_prompt TEXT,
    waiting_since INTEGER,
    work_item_ref TEXT,
    last_known_status TEXT
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
    /// Per-session capture line counters (lazily initialised from the file on first
    /// query, then kept current by [`Store::append_output`]) so the trimmed-output
    /// notice never re-reads whole capture files.
    capture_lines: Mutex<HashMap<SessionId, CaptureCount>>,
}

/// Incrementally-maintained line accounting for one capture file: newline count plus the
/// last byte written, so a trailing unterminated line still counts (FR-016a).
#[derive(Debug, Clone, Copy, Default)]
struct CaptureCount {
    newlines: u64,
    last_byte: Option<u8>,
}

impl CaptureCount {
    fn observe(&mut self, chunk: &[u8]) {
        self.newlines += chunk.iter().filter(|b| **b == b'\n').count() as u64;
        if let Some(last) = chunk.last() {
            self.last_byte = Some(*last);
        }
    }

    fn lines(self) -> u64 {
        match self.last_byte {
            None => 0,
            Some(b'\n') => self.newlines,
            Some(_) => self.newlines + 1,
        }
    }
}

/// One schema upgrade step (an entry in [`Store::MIGRATIONS`]).
type Migration = fn(&Connection) -> Result<(), StoreError>;

/// Map SQLite's no-rows result onto [`StoreError::NotFound`] (the `get_*` lookups).
fn not_found(e: rusqlite::Error) -> StoreError {
    match e {
        rusqlite::Error::QueryReturnedNoRows => StoreError::NotFound,
        other => other.into(),
    }
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
            capture_lines: Mutex::new(HashMap::new()),
        };
        store.migrate()?;
        Ok(store)
    }

    /// Ordered migration ladder: entry `i` upgrades schema version `i + 1` to `i + 2`.
    const MIGRATIONS: [Migration; (SCHEMA_VERSION - 1) as usize] =
        [Self::upgrade_v1_to_v2, Self::upgrade_v2_to_v3];

    fn migrate(&self) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        // The whole sequence — the schema batch, every ALTER/UPDATE step, and the final
        // version write — runs in ONE transaction so an interrupted upgrade rolls back
        // atomically and the recorded version can never drift from the actual schema (P1).
        let tx = conn.unchecked_transaction()?;
        // `CREATE ... IF NOT EXISTS` throughout: on an existing database this only fills
        // in missing tables, so the version recorded *before* this run is still readable.
        tx.execute_batch(SCHEMA)?;
        let recorded: Option<String> = tx
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None), // fresh database
                other => Err(other),
            })?;
        // A fresh database starts at the current version — the schema batch above already
        // created every table in its final shape. A present-but-unparseable version is a
        // hard error rather than a silent "already current" that would skip migrations and
        // leave the store missing columns (P2).
        let recorded: Option<i64> = match recorded {
            None => None,
            Some(v) => Some(v.parse().map_err(|_| StoreError::CorruptSchemaVersion(v))?),
        };
        if let Some(recorded) = recorded {
            if (1..SCHEMA_VERSION).contains(&recorded) {
                for v in recorded..SCHEMA_VERSION {
                    Self::MIGRATIONS[(v - 1) as usize](&tx)?;
                }
            }
        }
        tx.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', ?1)",
            [SCHEMA_VERSION.to_string()],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Whether `table` already has a column named `column` (used to make the `ALTER TABLE
    /// ADD COLUMN` migration steps idempotent after a partially-applied upgrade — P1).
    fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool, StoreError> {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            if row.get::<_, String>(1)? == column {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// v1 → v2: add the US6 waiting/attention columns and wrap the plain-text
    /// `terminal_outcome` reasons into the richer JSON [`Outcome`] shape.
    fn upgrade_v1_to_v2(conn: &Connection) -> Result<(), StoreError> {
        for column in [
            "pending_prompt TEXT",
            "waiting_since INTEGER",
            "work_item_ref TEXT",
            "last_known_status TEXT",
        ] {
            // Idempotent: a partially-applied upgrade may already have added this column, so
            // skip it rather than failing on a "duplicate column" error (P1).
            let name = column.split_whitespace().next().expect("column name");
            if !Self::column_exists(conn, "sessions", name)? {
                conn.execute(&format!("ALTER TABLE sessions ADD COLUMN {column}"), [])?;
            }
        }

        let reasons: Vec<(String, String)> = {
            let mut stmt = conn.prepare(
                "SELECT id, terminal_outcome FROM sessions WHERE terminal_outcome IS NOT NULL",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get(1)?)))?;
            rows.collect::<Result<_, _>>()?
        };
        for (id, reason) in reasons {
            let wrapped = serde_json::to_string(&Outcome::reason(reason))?;
            conn.execute(
                "UPDATE sessions SET terminal_outcome = ?2 WHERE id = ?1",
                (id, wrapped),
            )?;
        }
        Ok(())
    }

    /// v2 → v3 (T086, FR-021b): only the new `config` table, which the
    /// `CREATE TABLE IF NOT EXISTS` schema batch already created — nothing to alter.
    fn upgrade_v2_to_v3(_conn: &Connection) -> Result<(), StoreError> {
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

    // ----- Operator configuration (FR-021b) -----

    /// Set (or clear) the persisted per-backend idle rate (per hour) used for waiting-cost
    /// estimates (FR-021b). Configuration survives restart.
    pub fn set_backend_idle_rate(
        &self,
        kind: BackendKind,
        rate: Option<f64>,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        let key = format!("idle_rate.{}", enum_to_text(&kind)?);
        match rate {
            Some(rate) => {
                conn.execute(
                    "INSERT OR REPLACE INTO config (key, value) VALUES (?1, ?2)",
                    (key, rate.to_string()),
                )?;
            }
            None => {
                conn.execute("DELETE FROM config WHERE key = ?1", [key])?;
            }
        }
        Ok(())
    }

    /// All persisted per-backend idle rates (FR-021b).
    pub fn backend_idle_rates(&self) -> Result<Vec<(BackendKind, f64)>, StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        let mut stmt = conn.prepare_cached(
            "SELECT key, value FROM config WHERE key LIKE 'idle_rate.%' ORDER BY key",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (key, value) = row?;
            let kind = key.trim_start_matches("idle_rate.");
            match (text_to_enum::<BackendKind>(kind), value.parse::<f64>()) {
                (Ok(kind), Ok(rate)) => out.push((kind, rate)),
                _ => continue, // an unknown kind / malformed value never breaks reads
            }
        }
        Ok(out)
    }

    /// Set (or clear with `None` = unlimited) the persisted concurrency limit (FR-026,
    /// G6/G15). Configuration survives restart; `None` deletes the key so the reseed keeps
    /// the process default.
    pub fn set_concurrency_limit(&self, limit: Option<usize>) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        match limit {
            Some(limit) => {
                conn.execute(
                    "INSERT OR REPLACE INTO config (key, value) VALUES ('concurrency_limit', ?1)",
                    [limit.to_string()],
                )?;
            }
            None => {
                conn.execute("DELETE FROM config WHERE key = 'concurrency_limit'", [])?;
            }
        }
        Ok(())
    }

    /// The persisted concurrency limit, if one is configured (`None` = not set / unlimited).
    pub fn concurrency_limit(&self) -> Result<Option<usize>, StoreError> {
        self.config_value("concurrency_limit")
    }

    /// Set the persisted stall interval in seconds (FR-020, G6/G15). Survives restart.
    pub fn set_stall_interval(&self, secs: u64) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        conn.execute(
            "INSERT OR REPLACE INTO config (key, value) VALUES ('stall_interval_secs', ?1)",
            [secs.to_string()],
        )?;
        Ok(())
    }

    /// The persisted stall interval in seconds, if configured.
    pub fn stall_interval(&self) -> Result<Option<u64>, StoreError> {
        self.config_value("stall_interval_secs")
    }

    /// Read a single scalar `config` value, parsed from its stored text; `None` when the
    /// key is absent or the stored value fails to parse (a malformed value never breaks the
    /// reseed on startup).
    fn config_value<T: std::str::FromStr>(&self, key: &str) -> Result<Option<T>, StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        let raw: Option<String> = conn
            .query_row("SELECT value FROM config WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        Ok(raw.and_then(|v| v.parse().ok()))
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
            "SELECT name, invocation, capabilities FROM tools WHERE id = ?1",
            [id.to_string()],
            |r| Ok(row_to_tool(id, r)),
        )
        .map_err(not_found)?
    }

    /// List all registered tools.
    pub fn list_tools(&self) -> Result<Vec<AgenticTool>, StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        let mut stmt = conn
            .prepare_cached("SELECT name, invocation, capabilities, id FROM tools ORDER BY name")?;
        let rows = stmt.query_map([], |r| Ok(list_row_to_tool(r)))?;
        collect_lenient(rows, "tool")
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
        .map_err(not_found)
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
            |r| Ok(row_to_environment(id, r)),
        )
        .map_err(not_found)?
    }

    /// List every environment record (§6.7 — the Environments screen).
    pub fn list_environments(&self) -> Result<Vec<SandboxEnvironment>, StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        let mut stmt = conn.prepare_cached(
            "SELECT backend_id, origin, worktree, lifecycle, id FROM environments",
        )?;
        let rows = stmt.query_map([], |r| Ok(list_row_to_environment(r)))?;
        collect_lenient(rows, "environment")
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
             (id, tool_id, objective_id, environment_id, source_id, status, created_at, started_at, ended_at, terminal_outcome, accepts_input, pending_prompt, waiting_since, work_item_ref, last_known_status)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
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
                s.terminal_outcome
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?,
                i64::from(s.accepts_input),
                s.pending_prompt.clone(),
                s.waiting_since.map(|t| t.millis()),
                s.work_item_ref
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?,
                s.last_known_status
                    .map(|st| enum_to_text(&st))
                    .transpose()?,
            ),
        )?;
        Ok(())
    }

    /// Fetch a session by id.
    pub fn get_session(&self, id: SessionId) -> Result<Session, StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        conn.query_row(
            "SELECT tool_id, objective_id, environment_id, source_id, status, created_at, started_at, ended_at, terminal_outcome, accepts_input, pending_prompt, waiting_since, work_item_ref, last_known_status FROM sessions WHERE id = ?1",
            [id.to_string()],
            |r| Ok(row_to_session(id, r)),
        )
        .map_err(not_found)?
    }

    /// List all sessions, newest first.
    pub fn list_sessions(&self) -> Result<Vec<Session>, StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        let mut stmt = conn.prepare_cached(
            "SELECT tool_id, objective_id, environment_id, source_id, status, created_at, started_at, ended_at, terminal_outcome, accepts_input, pending_prompt, waiting_since, work_item_ref, last_known_status, id FROM sessions ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |r| Ok(list_row_to_session(r)))?;
        collect_lenient(rows, "session")
    }

    /// Update a session's status (and optionally a reason/end timestamp) **without** touching
    /// the waiting/attention columns (`pending_prompt`, `waiting_since`). A `reason` is stored
    /// as a reason-only [`Outcome`]; `None` keeps any existing `terminal_outcome`.
    ///
    /// This is deliberately NOT the canonical lifecycle writer — for an ordinary status
    /// transition use [`Store::set_session_state`], which additionally normalizes the waiting
    /// columns and `terminal_outcome` from the target state. This status-only variant exists
    /// for reconciliation's `Unknown`/restore round-trip (FR-020), where a session's waiting
    /// state (its pending prompt + anchor) MUST survive the loss-of-contact transition and be
    /// re-derived on reconnect — normalizing (i.e. clearing) those columns here would lose
    /// them. It leaves `last_known_status` untouched too (that is
    /// [`Store::set_session_last_known`]'s job).
    pub fn set_session_status(
        &self,
        id: SessionId,
        status: SessionStatus,
        ended_at: Option<Timestamp>,
        reason: Option<&str>,
    ) -> Result<(), StoreError> {
        let outcome = reason.map(Outcome::reason);
        let conn = self.conn.lock().expect("poisoned");
        conn.execute(
            "UPDATE sessions SET status = ?2, ended_at = COALESCE(?3, ended_at), terminal_outcome = COALESCE(?4, terminal_outcome) WHERE id = ?1",
            (
                id.to_string(),
                enum_to_text(&status)?,
                ended_at.map(|t| t.millis()),
                outcome.as_ref().map(serde_json::to_string).transpose()?,
            ),
        )?;
        Ok(())
    }

    /// **The canonical status-transition writer.** Transition a session's lifecycle
    /// atomically: write the status (+ optional `ended_at`/`outcome`) AND normalize the
    /// waiting/attention columns and `terminal_outcome` from the TARGET status, all in one
    /// statement so a concurrent reader never observes a torn intermediate state (P4). Every
    /// ordinary lifecycle transition goes through here so the waiting columns and outcome are
    /// derived uniformly from the destination state (the sole exception is reconciliation —
    /// see [`Store::set_session_status`]).
    ///
    /// The waiting columns are derived from `status` — subsuming S2 (any exit out of a
    /// waiting state clears them) and keeping future exits correct:
    /// - `WaitingForInput` ⇒ `pending_prompt = prompt`, `waiting_since = waiting_since`
    /// - `AwaitingConfirmation` ⇒ prompt cleared, `waiting_since = waiting_since` (anchor)
    /// - any other status ⇒ both cleared to `NULL`
    ///
    /// `terminal_outcome` is cleared when transitioning to a live/non-outcome state
    /// (`Running`/`Starting`, so a stale stall reason never lingers — P6) and otherwise
    /// `COALESCE`d with any passed `outcome` (preserving outcomes for terminal/stalled
    /// states; `None` keeps the existing one).
    pub fn set_session_state(
        &self,
        id: SessionId,
        status: SessionStatus,
        ended_at: Option<Timestamp>,
        outcome: Option<&Outcome>,
        prompt: Option<&str>,
        waiting_since: Option<Timestamp>,
    ) -> Result<(), StoreError> {
        let (prompt, waiting_since): (Option<&str>, Option<Timestamp>) = match status {
            SessionStatus::WaitingForInput => (prompt, waiting_since),
            SessionStatus::AwaitingConfirmation => (None, waiting_since),
            _ => (None, None),
        };
        let clears_outcome = matches!(status, SessionStatus::Running | SessionStatus::Starting);
        let conn = self.conn.lock().expect("poisoned");
        if clears_outcome {
            conn.execute(
                "UPDATE sessions SET status = ?2, ended_at = COALESCE(?3, ended_at), terminal_outcome = NULL, pending_prompt = ?4, waiting_since = ?5 WHERE id = ?1",
                (
                    id.to_string(),
                    enum_to_text(&status)?,
                    ended_at.map(|t| t.millis()),
                    prompt,
                    waiting_since.map(|t| t.millis()),
                ),
            )?;
        } else {
            conn.execute(
                "UPDATE sessions SET status = ?2, ended_at = COALESCE(?3, ended_at), terminal_outcome = COALESCE(?4, terminal_outcome), pending_prompt = ?5, waiting_since = ?6 WHERE id = ?1",
                (
                    id.to_string(),
                    enum_to_text(&status)?,
                    ended_at.map(|t| t.millis()),
                    outcome.map(serde_json::to_string).transpose()?,
                    prompt,
                    waiting_since.map(|t| t.millis()),
                ),
            )?;
        }
        Ok(())
    }

    /// Set (or clear) a session's pending prompt + waiting anchor (FR-015b, FR-021a).
    pub fn set_session_waiting(
        &self,
        id: SessionId,
        pending_prompt: Option<&str>,
        waiting_since: Option<Timestamp>,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        conn.execute(
            "UPDATE sessions SET pending_prompt = ?2, waiting_since = ?3 WHERE id = ?1",
            (
                id.to_string(),
                pending_prompt,
                waiting_since.map(|t| t.millis()),
            ),
        )?;
        Ok(())
    }

    /// Set (or clear) the last-known status preserved while a session is `Unknown` (FR-020).
    pub fn set_session_last_known(
        &self,
        id: SessionId,
        last_known: Option<SessionStatus>,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        conn.execute(
            "UPDATE sessions SET last_known_status = ?2 WHERE id = ?1",
            (
                id.to_string(),
                last_known.map(|st| enum_to_text(&st)).transpose()?,
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
        drop(conn);
        // Also drop the on-disk capture file and its line-counter entry, so no orphaned
        // `.log` or stale counter survives the delete (D6).
        match std::fs::remove_file(self.capture_path(id)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        self.capture_lines.lock().expect("poisoned").remove(&id);
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
        let mut stmt = conn.prepare_cached(
            "SELECT task_id, description, status, updated_at FROM tasks WHERE session_id = ?1 ORDER BY task_id",
        )?;
        let rows = stmt.query_map([session.to_string()], |r| {
            Ok((|| -> Result<TrackedTask, StoreError> {
                Ok(TrackedTask {
                    id: TaskId::new(r.get::<_, String>(0)?),
                    session_id: session,
                    description: r.get(1)?,
                    status: text_to_enum::<TaskStatus>(&r.get::<_, String>(2)?)?,
                    updated_at: Timestamp::from_millis(r.get(3)?),
                })
            })())
        })?;
        collect_lenient(rows, "task")
    }

    // ----- Events + capture files -----

    /// The per-session output capture file.
    fn capture_path(&self, session: SessionId) -> PathBuf {
        self.captures_dir.join(format!("{session}.log"))
    }

    /// Append redacted output bytes to the session's capture file and record an
    /// [`EventPayload::Output`] referencing the written span (FR-018).
    pub fn append_output(
        &self,
        session: SessionId,
        timestamp: Timestamp,
        redacted: &[u8],
    ) -> Result<EventRecord, StoreError> {
        use std::io::Write;
        let path = self.capture_path(session);
        let offset = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        file.write_all(redacted)?;

        // Keep the line counter current when it is already initialised; an absent entry
        // stays absent so the first `capture_line_count` reads the whole file once.
        if let Some(count) = self
            .capture_lines
            .lock()
            .expect("poisoned")
            .get_mut(&session)
        {
            count.observe(redacted);
        }

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

    /// Read the tail of a session's redacted capture file (empty if none), for display.
    /// Seeks to the tail instead of reading the whole file.
    #[must_use]
    pub fn output_tail(&self, session: SessionId, max_bytes: usize) -> String {
        use std::io::{Read, Seek, SeekFrom};
        let read_tail = || -> std::io::Result<Vec<u8>> {
            let mut file = std::fs::File::open(self.capture_path(session))?;
            let len = file.metadata()?.len();
            file.seek(SeekFrom::Start(len.saturating_sub(max_bytes as u64)))?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            Ok(bytes)
        };
        match read_tail() {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(_) => String::new(),
        }
    }

    /// Line count of a session's persisted capture file (0 if none) — the "M" of the
    /// trimmed-output notice "showing last N of M lines" (FR-016a). A trailing
    /// unterminated line counts as a line. The file is read in full only on the first
    /// query per session; [`Self::append_output`] keeps the counter current after that.
    #[must_use]
    pub fn capture_line_count(&self, session: SessionId) -> u64 {
        let mut counts = self.capture_lines.lock().expect("poisoned");
        counts
            .entry(session)
            .or_insert_with(|| {
                let mut count = CaptureCount::default();
                if let Ok(bytes) = std::fs::read(self.capture_path(session)) {
                    count.observe(&bytes);
                }
                count
            })
            .lines()
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
        // rowid breaks same-millisecond ties in insertion order, keeping the timeline
        // stable (FR-019a).
        let mut stmt = conn.prepare_cached(
            "SELECT id, timestamp, kind, payload FROM events WHERE session_id = ?1 ORDER BY timestamp, rowid",
        )?;
        let rows = stmt.query_map([session.to_string()], |r| Ok(row_to_event(session, r)))?;
        collect_lenient(rows, "event")
    }

    /// The newest `limit` non-`Output` events of a session, oldest first — the session
    /// timeline (FR-019a). Output chunks are excluded in SQL (the terminal renders them
    /// from the capture file), so the timeline never pages through high-volume output.
    pub fn timeline_events(
        &self,
        session: SessionId,
        limit: usize,
    ) -> Result<Vec<EventRecord>, StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        let mut stmt = conn.prepare_cached(
            "SELECT id, timestamp, kind, payload FROM events WHERE session_id = ?1 AND kind != ?2 ORDER BY timestamp DESC, rowid DESC LIMIT ?3",
        )?;
        let rows = stmt.query_map(
            (
                session.to_string(),
                enum_to_text(&EventKind::Output)?,
                limit as i64,
            ),
            |r| Ok(row_to_event(session, r)),
        )?;
        let mut events = collect_lenient(rows, "event")?;
        events.reverse(); // newest-first query → oldest-first timeline
        Ok(events)
    }

    /// The most recent lifecycle event of a session, if any — when it entered its current
    /// state (FR-021a) without scanning the full event history.
    pub fn last_lifecycle_event(
        &self,
        session: SessionId,
    ) -> Result<Option<EventRecord>, StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        let mut stmt = conn.prepare_cached(
            "SELECT id, timestamp, kind, payload FROM events WHERE session_id = ?1 AND kind = ?2 ORDER BY timestamp DESC, rowid DESC LIMIT 1",
        )?;
        stmt.query_row(
            (session.to_string(), enum_to_text(&EventKind::Lifecycle)?),
            |r| Ok(row_to_event(session, r)),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(StoreError::from(other)),
        })
        .and_then(|opt| opt.transpose())
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
        let mut stmt = conn.prepare_cached(
            "SELECT metric, value, timestamp FROM metrics m WHERE session_id = ?1 AND timestamp = (SELECT MAX(timestamp) FROM metrics WHERE session_id = m.session_id AND metric = m.metric) GROUP BY metric",
        )?;
        let rows = stmt.query_map([session.to_string()], |r| {
            Ok((|| -> Result<ResourceUsageMetric, StoreError> {
                Ok(ResourceUsageMetric {
                    session_id: session,
                    metric: text_to_enum::<MetricKind>(&r.get::<_, String>(0)?)?,
                    value: r.get(1)?,
                    timestamp: Timestamp::from_millis(r.get(2)?),
                })
            })())
        })?;
        collect_lenient(rows, "metric")
    }

    /// The most recent `limit` samples of one metric for a session, oldest → newest —
    /// the short-horizon history behind the telemetry rail's sparklines (FR-019).
    pub fn metric_history(
        &self,
        session: SessionId,
        metric: MetricKind,
        limit: usize,
    ) -> Result<Vec<ResourceUsageMetric>, StoreError> {
        let conn = self.conn.lock().expect("poisoned");
        let mut stmt = conn.prepare_cached(
            "SELECT value, timestamp FROM metrics WHERE session_id = ?1 AND metric = ?2 ORDER BY timestamp DESC, rowid DESC LIMIT ?3",
        )?;
        let rows = stmt.query_map(
            (session.to_string(), enum_to_text(&metric)?, limit as i64),
            |r| {
                Ok(ResourceUsageMetric {
                    session_id: session,
                    metric,
                    value: r.get(0)?,
                    timestamp: Timestamp::from_millis(r.get(1)?),
                })
            },
        )?;
        let mut samples: Vec<ResourceUsageMetric> = rows.collect::<Result<_, _>>()?;
        samples.reverse(); // newest-first query → oldest-first history
        Ok(samples)
    }
}

fn row_to_event(session: SessionId, r: &rusqlite::Row<'_>) -> Result<EventRecord, StoreError> {
    Ok(EventRecord {
        id: EventId::from_uuid(parse_uuid(&r.get::<_, String>(0)?)?),
        session_id: session,
        timestamp: Timestamp::from_millis(r.get(1)?),
        kind: text_to_enum::<EventKind>(&r.get::<_, String>(2)?)?,
        payload: serde_json::from_str(&r.get::<_, String>(3)?)?,
    })
}

/// List-query variant of [`row_to_tool`]: the id is the LAST column (index 3).
fn list_row_to_tool(r: &rusqlite::Row<'_>) -> Result<AgenticTool, StoreError> {
    let id = ToolId::from_uuid(parse_uuid(&r.get::<_, String>(3)?)?);
    row_to_tool(id, r)
}

/// List-query variant of [`row_to_environment`]: the id is the LAST column (index 4).
fn list_row_to_environment(r: &rusqlite::Row<'_>) -> Result<SandboxEnvironment, StoreError> {
    let id = EnvironmentId::from_uuid(parse_uuid(&r.get::<_, String>(4)?)?);
    row_to_environment(id, r)
}

/// List-query variant of [`row_to_session`]: the id is the LAST column (index 14).
fn list_row_to_session(r: &rusqlite::Row<'_>) -> Result<Session, StoreError> {
    let id = SessionId::from_uuid(parse_uuid(&r.get::<_, String>(14)?)?);
    row_to_session(id, r)
}

fn row_to_tool(id: ToolId, r: &rusqlite::Row<'_>) -> Result<AgenticTool, StoreError> {
    Ok(AgenticTool {
        id,
        name: r.get(0)?,
        invocation: serde_json::from_str(&r.get::<_, String>(1)?)?,
        capabilities: serde_json::from_str(&r.get::<_, String>(2)?)?,
    })
}

fn row_to_environment(
    id: EnvironmentId,
    r: &rusqlite::Row<'_>,
) -> Result<SandboxEnvironment, StoreError> {
    Ok(SandboxEnvironment {
        id,
        backend_id: BackendId::from_uuid(parse_uuid(&r.get::<_, String>(0)?)?),
        origin: text_to_enum::<Origin>(&r.get::<_, String>(1)?)?,
        worktree_ref: r
            .get::<_, Option<String>>(2)?
            .map(|w| serde_json::from_str::<WorktreeRef>(&w))
            .transpose()?,
        lifecycle: text_to_enum::<EnvLifecycle>(&r.get::<_, String>(3)?)?,
    })
}

fn row_to_session(id: SessionId, r: &rusqlite::Row<'_>) -> Result<Session, StoreError> {
    Ok(Session {
        id,
        tool_id: ToolId::from_uuid(parse_uuid(&r.get::<_, String>(0)?)?),
        objective_id: ObjectiveId::from_uuid(parse_uuid(&r.get::<_, String>(1)?)?),
        environment_id: EnvironmentId::from_uuid(parse_uuid(&r.get::<_, String>(2)?)?),
        source_id: SourceId::from_uuid(parse_uuid(&r.get::<_, String>(3)?)?),
        status: text_to_enum::<SessionStatus>(&r.get::<_, String>(4)?)?,
        created_at: Timestamp::from_millis(r.get(5)?),
        started_at: r.get::<_, Option<i64>>(6)?.map(Timestamp::from_millis),
        ended_at: r.get::<_, Option<i64>>(7)?.map(Timestamp::from_millis),
        terminal_outcome: r
            .get::<_, Option<String>>(8)?
            .map(|o| serde_json::from_str::<Outcome>(&o))
            .transpose()?,
        accepts_input: r.get::<_, i64>(9)? != 0,
        pending_prompt: r.get(10)?,
        waiting_since: r.get::<_, Option<i64>>(11)?.map(Timestamp::from_millis),
        work_item_ref: r
            .get::<_, Option<String>>(12)?
            .map(|w| serde_json::from_str::<WorkItemRef>(&w))
            .transpose()?,
        last_known_status: r
            .get::<_, Option<String>>(13)?
            .map(|s| text_to_enum::<SessionStatus>(&s))
            .transpose()?,
    })
}
