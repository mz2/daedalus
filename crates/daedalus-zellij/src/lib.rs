//! zellij multiplexing/attach + the terminal-channel abstraction the surfaces render.
//!
//! Daedalus runs each agent inside a named zellij session and attaches to it. On the
//! desktop the [`TerminalChannel`] feeds a native GPUI terminal view; the same captured
//! stream is redacted and persisted (contract `contracts/terminal-attach.md`).

pub mod attach;

pub use attach::{
    list_local_sessions, AttachError, ByteSink, ByteStream, InMemoryTerminal, TerminalAttach,
    TerminalChannel,
};

/// The name prefix that marks a zellij session as Daedalus-managed (used when naming
/// sessions at launch and when recognising them during local discovery).
pub const SESSION_PREFIX: &str = "daedalus-";

/// The zellij session name for an environment: [`SESSION_PREFIX`] + the environment id.
#[must_use]
pub fn format_session_name(env: &daedalus_proto::EnvironmentId) -> String {
    format!("{SESSION_PREFIX}{env}")
}
