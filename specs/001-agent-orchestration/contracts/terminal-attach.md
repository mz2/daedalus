# Contract: Embedded terminal attach

**Crates**: `daedalus-zellij` + `apps/desktop`. **Satisfies**: FR-008, FR-011, FR-016, SC-003.

The embedded terminal is a first-class surface. On the **desktop** (GPUI), it is a native GPUI terminal
view backed by `alacritty_terminal`, attached to a zellij session. (A web terminal — zellij's web service
or an `xterm.js` client — is deferred with the web surface; the attach boundary below is defined so it can
be added without changing the core.)

```rust
pub trait TerminalAttach {
    /// Attach to a session's live terminal via zellij; returns a duplex stream of terminal I/O.
    async fn attach(&self, session: SessionId) -> Result<TerminalChannel, AttachError>;
}

pub struct TerminalChannel {
    pub output: ByteStream,   // PTY output → rendered in the GPUI terminal view (redacted for capture, R9)
    pub input: ByteSink,      // operator keystrokes / send-input (FR-023), if the tool accepts input
}
```

### Rules (test-first)
- **C-T1**: output appears in the terminal view within 5s of being produced (SC-003). *(test/bench)*
- **C-T2**: a session not attachable via zellij yields `AttachError` with a clear reason (edge case:
  zellij unavailable), never a silent blank view. *(test)*
- **C-T3**: excessive/rapid output keeps the view responsive and does not lose the persisted record
  (edge case: excessive output). *(test)*
- **C-T4**: the same captured stream that feeds persistence is secret-redacted (FR-031). *(test)*
