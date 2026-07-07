//! Terminal attach abstraction + zellij-backed and in-memory implementations.

use async_trait::async_trait;
use bytes::Bytes;
use thiserror::Error;
use tokio::sync::mpsc;

use daedalus_proto::SessionId;

/// A stream of PTY output bytes flowing from the agent's terminal to the surface.
pub type ByteStream = mpsc::Receiver<Bytes>;
/// A sink for operator keystrokes / send-input flowing into the agent's terminal.
pub type ByteSink = mpsc::Sender<Bytes>;

/// A duplex terminal channel for one attached session (contract `terminal-attach.md`).
pub struct TerminalChannel {
    /// PTY output → rendered in the terminal view (and redacted for capture, R9).
    pub output: ByteStream,
    /// Operator keystrokes / send-input (FR-023), if the tool accepts input.
    pub input: ByteSink,
}

/// Why an attach failed. Never surfaced as a silent blank view (contract C-T2).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AttachError {
    /// zellij is not installed / not reachable on this host.
    #[error("zellij is unavailable: {0}")]
    ZellijUnavailable(String),
    /// The session exists but cannot be attached (e.g. it has exited).
    #[error("session not attachable: {0}")]
    Unattachable(String),
    /// No such session is known to zellij.
    #[error("no such zellij session")]
    NotFound,
}

/// Attach to a session's live terminal via zellij.
#[async_trait]
pub trait TerminalAttach: Send + Sync {
    /// Returns a duplex stream of terminal I/O for the session, or a clear reason.
    async fn attach(&self, session: SessionId) -> Result<TerminalChannel, AttachError>;
}

/// List local zellij sessions by name (used by local discovery — FR-010).
///
/// Shells out to `zellij list-sessions`. If zellij is not present, returns
/// [`AttachError::ZellijUnavailable`] rather than panicking, so callers can mark the source
/// unavailable instead of crashing.
pub async fn list_local_sessions() -> Result<Vec<String>, AttachError> {
    let output = tokio::process::Command::new("zellij")
        .arg("list-sessions")
        .arg("--no-formatting")
        .output()
        .await
        .map_err(|e| AttachError::ZellijUnavailable(e.to_string()))?;

    if !output.status.success() {
        // zellij prints "No active zellij sessions found." to stderr with a non-zero code.
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("No active zellij sessions") {
            return Ok(Vec::new());
        }
        return Err(AttachError::ZellijUnavailable(stderr.trim().to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let names = stdout
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    Ok(names)
}

/// An in-memory terminal used by tests and the fake backend's monitoring path.
///
/// It hands out a channel whose output is fed from a pre-seeded script of chunks, letting
/// the lifecycle/monitoring tests run without a real PTY (Principle III).
#[derive(Default)]
pub struct InMemoryTerminal {
    scripted: std::sync::Mutex<Vec<(SessionId, Vec<Bytes>)>>,
}

impl InMemoryTerminal {
    /// Create an empty in-memory terminal.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed the output chunks a future `attach(session)` will replay.
    pub fn seed(&self, session: SessionId, chunks: Vec<Bytes>) {
        self.scripted
            .lock()
            .expect("poisoned")
            .push((session, chunks));
    }
}

#[async_trait]
impl TerminalAttach for InMemoryTerminal {
    async fn attach(&self, session: SessionId) -> Result<TerminalChannel, AttachError> {
        let chunks = {
            let mut scripted = self.scripted.lock().expect("poisoned");
            let pos = scripted.iter().position(|(s, _)| *s == session);
            match pos {
                Some(i) => scripted.remove(i).1,
                None => return Err(AttachError::NotFound),
            }
        };

        let (out_tx, out_rx) = mpsc::channel(1024);
        let (in_tx, _in_rx) = mpsc::channel(1024);
        tokio::spawn(async move {
            for chunk in chunks {
                if out_tx.send(chunk).await.is_err() {
                    break;
                }
            }
        });
        Ok(TerminalChannel {
            output: out_rx,
            input: in_tx,
        })
    }
}

/// A synthetic *live* terminal for local testing (Principle III): attaching yields a real
/// stream of agent-like output that arrives over a few seconds, rather than a pre-seeded
/// replay. It is the legitimate local-testing path for the surface's capture pipeline (the
/// `fake` backend), demonstrating redaction and waiting-for-input detection end-to-end
/// without a real PTY / zellij.
///
/// The scripted sequence deliberately includes a line carrying a fake secret
/// (`export API_KEY=sk-demo-123456`), so the core redactor is exercised, and a trailing
/// prompt line so a tool declaring a matching prompt convention is surfaced as
/// waiting-for-input (FR-015b).
#[derive(Default)]
pub struct ScriptedLiveTerminal;

impl ScriptedLiveTerminal {
    /// Create a scripted live terminal.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// The scripted agent-like output lines (each emitted with a small delay on attach).
    /// The fake secret and the trailing prompt are load-bearing for the demo.
    fn script() -> Vec<&'static [u8]> {
        vec![
            b"Starting agent...\n",
            b"Cloning workspace...\n",
            b"Installing dependencies...\n",
            b"export API_KEY=sk-demo-123456\n",
            b"Building project...\n",
            b"All tests passed.\n",
            b"Proceed with deployment? [y/N] \n",
        ]
    }
}

#[async_trait]
impl TerminalAttach for ScriptedLiveTerminal {
    async fn attach(&self, _session: SessionId) -> Result<TerminalChannel, AttachError> {
        let (out_tx, out_rx) = mpsc::channel(1024);
        let (in_tx, _in_rx) = mpsc::channel(1024);
        tokio::spawn(async move {
            for line in Self::script() {
                // A short delay so the stream is genuinely live (arrives over ~2.8s).
                tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                if out_tx.send(Bytes::from_static(line)).await.is_err() {
                    break;
                }
            }
        });
        Ok(TerminalChannel {
            output: out_rx,
            input: in_tx,
        })
    }
}
