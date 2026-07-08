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
/// A sink for surface-driven terminal geometry changes (the view was resized).
pub type ResizeSink = mpsc::Sender<TerminalSize>;

/// Terminal geometry, as the surface's terminal view measures it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSize {
    /// Character columns.
    pub cols: u16,
    /// Character rows.
    pub rows: u16,
}

/// A duplex terminal channel for one attached session (contract `terminal-attach.md`).
#[derive(Debug)]
pub struct TerminalChannel {
    /// PTY output → rendered in the terminal view (and redacted for capture, R9).
    pub output: ByteStream,
    /// Operator keystrokes / send-input (FR-023), if the tool accepts input.
    pub input: ByteSink,
    /// Surface-driven geometry updates → applied to the attach's PTY (SIGWINCH to the
    /// bridge). Terminals without a real PTY (in-memory/scripted) accept-and-ignore;
    /// senders should not treat a send error as fatal.
    pub resize: ResizeSink,
}

/// A resize sink whose updates go nowhere — for terminals with no PTY to resize
/// (in-memory/scripted). The receiver is dropped, so sends fail; callers ignore that.
fn ignored_resize_sink() -> ResizeSink {
    mpsc::channel(1).0
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

/// Resolves a session to the full command line (program + args) of its attach bridge, or
/// a clear reason why it cannot be attached (C-T2).
pub type AttachCommandResolver =
    Box<dyn Fn(SessionId) -> Result<Vec<String>, AttachError> + Send + Sync>;

/// A [`TerminalAttach`] that spawns a bridge subprocess **on a local PTY** whose master
/// side is the terminal — the real-backend attach path (issue #15). For OpenShell the
/// bridge is `openshell sandbox exec --tty -n <sandbox> -- zellij attach <session>`; any
/// future backend with a process-shaped attach surface plugs in via its own resolver.
///
/// The PTY is load-bearing, not cosmetic: TTY-mode bridge CLIs take their terminal from
/// the controlling terminal, and a plain-piped child without one produces **no output at
/// all** (verified against openshell 0.0.77 — bytes flow from a shell, zero from
/// `setsid`). The child gets the PTY slave as stdio + controlling terminal; output
/// (stdout and stderr, interleaved by the line discipline) is read from the master, and
/// operator input is written to it (FR-023).
///
/// Lifecycle: the bridge is killed when the surface drops the channel, detaching from —
/// never terminating — the agent's session; the output channel closes when the bridge
/// exits (session ended).
pub struct ProcessTerminal {
    resolver: AttachCommandResolver,
}

/// Fixed bridge PTY geometry until surface-driven resize is plumbed through
/// [`TerminalChannel`] (issue #15 follow-up).
const PTY_COLS: u16 = 120;
const PTY_ROWS: u16 = 40;

impl ProcessTerminal {
    /// Build over a session→command resolver.
    #[must_use]
    pub fn new(resolver: AttachCommandResolver) -> Self {
        Self { resolver }
    }
}

#[async_trait]
impl TerminalAttach for ProcessTerminal {
    async fn attach(&self, session: SessionId) -> Result<TerminalChannel, AttachError> {
        let command = (self.resolver)(session)?;
        let (program, args) = command
            .split_first()
            .ok_or_else(|| AttachError::Unattachable("empty attach command".into()))?;

        let pty = portable_pty::native_pty_system();
        let pair = pty
            .openpty(portable_pty::PtySize {
                rows: PTY_ROWS,
                cols: PTY_COLS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| AttachError::Unattachable(format!("could not open a PTY: {e}")))?;

        let mut cmd = portable_pty::CommandBuilder::new(program);
        cmd.args(args);
        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| AttachError::ZellijUnavailable(format!("{program}: {e}")))?;
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| AttachError::Unattachable(format!("PTY reader: {e}")))?;
        let mut writer = pair
            .master
            .take_writer()
            .map_err(|e| AttachError::Unattachable(format!("PTY writer: {e}")))?;

        let (out_tx, out_rx) = mpsc::channel::<Bytes>(1024);
        let (in_tx, mut in_rx) = mpsc::channel::<Bytes>(1024);
        let (resize_tx, mut resize_rx) = mpsc::channel::<TerminalSize>(16);
        let child = std::sync::Arc::new(std::sync::Mutex::new(child));

        // Surface detached (both channel ends dropped) ⇒ kill the bridge so the blocking
        // reader unblocks. Killing the bridge only detaches; the agent session lives on.
        let watchdog_child = child.clone();
        let watchdog_out = out_tx.clone();
        tokio::spawn(async move {
            watchdog_out.closed().await;
            let _ = watchdog_child.lock().expect("poisoned").kill();
        });

        // PTY master → surface (blocking reader on a dedicated thread).
        let reader_child = child.clone();
        std::thread::spawn(move || {
            use std::io::Read as _;
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break, // bridge exited: channel closes
                    Ok(n) => {
                        if out_tx
                            .blocking_send(Bytes::copy_from_slice(&buf[..n]))
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
            let _ = reader_child.lock().expect("poisoned").kill();
        });

        // Surface geometry → PTY winsize (SIGWINCH to the bridge). This thread owns the
        // master end, keeping it alive for the bridge's lifetime; it exits when the
        // channel closes (TerminalChannel dropped).
        let master = pair.master;
        std::thread::spawn(move || {
            while let Some(size) = resize_rx.blocking_recv() {
                // Degenerate sizes (a view mid-layout reports 0x0/1x1) would make the
                // TUI client on the far end exit ("Bye from Zellij") — never forward them.
                if size.cols < 10 || size.rows < 3 {
                    continue;
                }
                let _ = master.resize(portable_pty::PtySize {
                    rows: size.rows,
                    cols: size.cols,
                    pixel_width: 0,
                    pixel_height: 0,
                });
            }
            drop(master);
        });

        // Surface keystrokes / SendInput → PTY master (FR-023).
        std::thread::spawn(move || {
            use std::io::Write as _;
            while let Some(bytes) = in_rx.blocking_recv() {
                if writer.write_all(&bytes).is_err() || writer.flush().is_err() {
                    break;
                }
            }
        });

        Ok(TerminalChannel {
            output: out_rx,
            input: in_tx,
            resize: resize_tx,
        })
    }
}

/// An in-memory terminal used by tests and the fake backend's monitoring path.
///
/// It hands out a channel whose output is fed from a pre-seeded script of chunks, letting
/// the lifecycle/monitoring tests run without a real PTY (Principle III).
#[derive(Default)]
pub struct InMemoryTerminal {
    scripted: std::sync::Mutex<Vec<(SessionId, Vec<Bytes>)>>,
    /// Input the surface/core wrote into an attached channel, capturable by tests
    /// (FR-023: send-input genuinely reaches the attached terminal).
    received: std::sync::Arc<std::sync::Mutex<Vec<(SessionId, Bytes)>>>,
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

    /// Input delivered into this session's attached channel so far (FR-023).
    #[must_use]
    pub fn input_received(&self, session: SessionId) -> Vec<Bytes> {
        self.received
            .lock()
            .expect("poisoned")
            .iter()
            .filter(|(s, _)| *s == session)
            .map(|(_, b)| b.clone())
            .collect()
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
        let (in_tx, mut in_rx) = mpsc::channel::<Bytes>(1024);
        let received = self.received.clone();
        tokio::spawn(async move {
            while let Some(bytes) = in_rx.recv().await {
                received.lock().expect("poisoned").push((session, bytes));
            }
        });
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
            resize: ignored_resize_sink(),
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
            resize: ignored_resize_sink(),
        })
    }
}
