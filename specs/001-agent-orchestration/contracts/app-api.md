# Contract: App API (surface ↔ core)

**Crate**: `daedalus-app` over `daedalus-core`, types in `daedalus-proto`.
**Consumed by**: `apps/desktop` (GPUI). Surface-agnostic so a future web surface reuses it unchanged.
**Satisfies**: FR-007a (shared core, thin surfaces), FR-015/016, FR-025, plus all operator actions.

The surface holds **no orchestration logic**; it issues commands, runs queries, and subscribes to an event
stream. Because the GPUI surface is in-process Rust, this is a direct API (no FFI, no serialization
boundary required for v1); `daedalus-proto` types are shared verbatim.

```rust
// Commands (operator intent) — each returns a Result with a stated reason on failure.
pub enum Command {
    RegisterTool(ToolDef),                       // FR-001a
    StartSession(StartSessionRequest),           // FR-001/002/006
    StopSession(SessionId),                      // FR-022
    SendInput { session: SessionId, data: Bytes },// FR-023 (only if tool accepts input)
    ConfirmCompletion(SessionId),                // FR-015a (AwaitingConfirmation → Completed)
    CleanUp(SessionId),                          // FR-024
    DeleteRecord(SessionId),                     // FR-030a (operator-initiated)
    ConnectDiscovered(DiscoveredSessionId),      // FR-011 (attach via zellij)
}

// Queries (read current state)
pub trait AppQuery {
    fn fleet(&self) -> Vec<SessionSummary>;          // FR-025 unified view
    fn session(&self, id: SessionId) -> Option<SessionDetail>;
    fn task_board(&self, id: SessionId) -> Vec<TrackedTask>;   // FR-009/017
    fn discovered(&self) -> Vec<DiscoveredSession>;            // FR-010, de-duplicated (FR-013)
    fn backends(&self) -> Vec<BackendStatus>;                  // FR-028
    fn resource_usage(&self, id: SessionId) -> Vec<ResourceUsageMetric>; // FR-019
}

// Event stream (push) — drives live UI updates without polling.
pub enum AppEvent {
    SessionStatusChanged { id: SessionId, status: SessionStatus }, // FR-015/020/021
    Output { id: SessionId, chunk: RedactedBytes },                // FR-016 (secret-redacted, R9)
    TaskStatusChanged { id: SessionId, task: TaskId, status: TaskStatus }, // FR-017 (≤5s, SC-010)
    DiscoveryChanged,                                              // sources/sessions appeared/left
    BackendAvailabilityChanged { id: BackendId, availability: Availability },
    Notification(TerminalOrAbnormal),                             // FR-021
}
```

### Contract rules (test-first)

- **C-A1**: `StartSession` with no provisionable/reachable environment returns a stated reason and creates
  no session record (FR-005). *(test)*
- **C-A2**: `SendInput` is rejected with a clear reason when the tool does not accept input (FR-023). *(test)*
- **C-A3**: `Output` events are redacted; no secret-shaped content reaches the stream or persistence
  (FR-031). *(test)*
- **C-A4**: `fleet()`/`discovered()` never show a session known-unreachable as healthy (FR-014, FR-028).
  *(test)*
- **C-A5**: every operator capability is reachable through `Command`/`AppQuery` (capability completeness for
  the single surface; parity contract preserved for future web). *(review gate)*
