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
