# Phase 1 Data Model: Agent Orchestration

**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Date**: 2026-06-19
**Updated**: 2026-07-06 — spec update from the completed design prototype: `WaitingForInput` + `Unknown`
states, waiting/confirmation session fields, Attention Item (derived), idle rates, availability reasons.

Entities are derived from the spec's "Key Entities" plus the clarified lifecycle semantics. Types live in
`daedalus-proto` (serde) and are persisted by `daedalus-core` in SQLite (output streamed to capture files).

## Entities

### Session
A single invocation of an agentic tool against an objective within an environment.

| Field | Type | Notes / Rules |
|-------|------|---------------|
| `id` | SessionId (UUID) | Unique (FR-003); stable identity used for cross-source dedup (FR-013) |
| `tool_id` | ToolId | → AgenticTool (FR-003) |
| `objective_id` | ObjectiveId | → Objective |
| `environment_id` | EnvironmentId | → SandboxEnvironment |
| `source_id` | SourceId | → Source/Host (local / mDNS / tunneled) |
| `status` | SessionStatus | See state machine; persisted (FR-015) |
| `created_at` / `started_at` / `ended_at` | Timestamp | `ended_at` set on terminal state |
| `terminal_outcome` | Option\<Outcome\> | reason text for failed/stalled/stopped; exit code + agent exit summary for AwaitingConfirmation (FR-015a) |
| `accepts_input` | bool | mirrors the tool capability (FR-023) |
| `pending_prompt` | Option\<String\> | the agent's question while `WaitingForInput` (FR-015b); cleared on answer |
| `waiting_since` | Option\<Timestamp\> | set on entering `WaitingForInput` / `AwaitingConfirmation`; drives waiting duration + idle cost (FR-021a/b) |
| `work_item_ref` | Option\<WorkItemRef\> | optional external work item (e.g. GitHub issue / Jira key) for display chips |

**Validation**: a Session MUST reference an existing tool, objective, environment, and source. A Session on
a pre-existing environment MUST carry a worktree reference (see SandboxEnvironment); absence ⇒ start fails
(FR-002a).

### SessionStatus (state machine)
States: `Starting → Running ⇄ WaitingForInput; Running → {Completed | Failed | Stalled | Stopped | AwaitingConfirmation}`;
plus `Unknown` (connection lost — derived presentation state, FR-020).

```
                 ┌─────────────► Failed            (crash / non-zero exit / provision fail)
                 │
Starting ──► Running ──► Completed                 (all tracked tasks done — FR-015a)
   │             │  └──► AwaitingConfirmation ──► Completed   (operator confirms)
   │             │                          └──► Stopped      (operator stops/cleans up)
   │             ├──⇄ WaitingForInput              (agent blocked on operator input — FR-015b;
   │             │                                  back to Running when answered)
   │             ├──► Stalled                      (no output/progress for configured interval)
   │             └──► Stopped                      (operator stop — FR-022)
   └──► Failed                                     (cannot start / no environment — FR-005)

any live state ──► Unknown (contact lost; last-known state preserved) ──► re-derived on reconnect/reconcile
```

Rules:
- `Completed` requires **all** tracked tasks done OR explicit operator confirmation — never agent exit
  alone (FR-015a, SC-004).
- A clean agent exit with tracked tasks unfinished ⇒ `AwaitingConfirmation` (not `Completed`), carrying
  the agent's exit summary; operator resolves via confirm-completion or clean-up.
- `WaitingForInput` (FR-015b) is entered only for tools that accept interactive input, detected per the
  tool's declared prompt convention (SDK waiting signal, else declared prompt pattern); it captures
  `pending_prompt` + `waiting_since` and returns to `Running` when input is delivered. Distinct from
  `Stalled` — a session waiting on input MUST NOT be reported stalled.
- Agent crash / abnormal exit ⇒ `Failed`; no output/progress for the configured stall interval ⇒ `Stalled`
  (operator-configurable; **default 120s**).
- Loss of contact with the environment ⇒ `Unknown`: last-known state preserved and displayed as
  connection-lost, never as healthy (FR-020); reconciliation on restart/reconnect re-derives status
  (FR-030). `Unknown` is derived from liveness, not an operator-settable state.
- Terminal states: `Completed`, `Failed`, `Stopped`. `Stalled`, `WaitingForInput`, `AwaitingConfirmation`,
  and `Unknown` are non-terminal attention states the operator resolves — all four feed the Needs-you
  queue (FR-021a) together with `Failed`.

**Terminology mapping** (one concept, three vocabularies — keep aligned):

| spec.md | `daedalus-proto` | design prototype key | UI label |
|---------|------------------|----------------------|----------|
| waiting for input | `WaitingForInput` | `awaiting` | "Waiting for input" |
| awaiting confirmation | `AwaitingConfirmation` | `confirm` | "Awaiting confirmation" |
| disconnected/unknown | `Unknown` | `unknown` | "Connection lost" |

### AgenticTool
Operator-registered, declaratively defined (FR-001a).

| Field | Type | Notes |
|-------|------|-------|
| `id` | ToolId | |
| `name` | String | unique display/identifier |
| `invocation` | InvocationSpec | how to launch inside a sandbox |
| `capabilities` | Capabilities | `accepts_interactive_input: bool`; when true, `prompt_convention: PromptConvention` (`SdkSignal` \| `PromptPattern(regex)`) — drives `WaitingForInput` detection (FR-001a/015b) |

**Validation**: `name` non-empty and unique; `invocation` well-formed ⇒ otherwise registration fails with a
reason (design brief §6.6).

### Objective
Overall goal for a session, expressed via an SDD convention (e.g. SpecKit) and decomposed into tracked
tasks (FR-001, Principle IV).

| Field | Type | Notes |
|-------|------|-------|
| `id` | ObjectiveId | |
| `artifact_ref` | ArtifactRef | location of SDD artifacts (spec/plan/tasks) in the workspace |
| `description` | String | short summary for display |

### TrackedTask
One unit of work shown on the board; status is **read from** SDD artifacts, not inferred (FR-017).

| Field | Type | Notes |
|-------|------|-------|
| `id` | TaskId | identifier from the SDD artifact (e.g. `T001`) |
| `session_id` | SessionId | |
| `description` | String | |
| `status` | TaskStatus | `Todo | InProgress | Done | Blocked` — sourced from `tasks.md` checkbox state |
| `updated_at` | Timestamp | board reflects changes ≤5s (SC-010) |

### SandboxEnvironment
The isolated environment hosting one session (FR-004), fresh or pre-existing.

| Field | Type | Notes |
|-------|------|-------|
| `id` | EnvironmentId | |
| `backend_id` | BackendId | owning backend |
| `origin` | Origin | `Fresh | PreExisting` |
| `worktree_ref` | Option\<WorktreeRef\> | REQUIRED when `origin = PreExisting` (FR-002a) |
| `lifecycle` | EnvLifecycle | `Provisioning | Ready | Releasing | Released | Unavailable` |
| `limits` | ResourceLimits | CPU/memory/disk/time policy |

### EnvironmentBackend (Provider)
A source of sandbox environments behind the common abstraction (FR-027).

| Field | Type | Notes |
|-------|------|-------|
| `id` | BackendId | |
| `kind` | BackendKind | `Workshop | MacosSandbox | Fake | …` |
| `availability` | Availability | `Available | Degraded | Unavailable` (FR-028) |
| `availability_reason` | Option\<String\> | stated reason when degraded/unavailable (FR-028), e.g. "high memory pressure" |
| `capabilities` | BackendCapabilities | limits, supports-fresh, supports-preexisting |
| `idle_rate` | Option\<MoneyPerHour\> | optional operator-configured rate (Settings) for waiting-cost estimates (FR-021b); `None` ⇒ no cost shown |

### Source / Host
Where a session is discovered (FR-010).

| Field | Type | Notes |
|-------|------|-------|
| `id` | SourceId | |
| `kind` | SourceKind | `Local | Mdns | TunneledWorkshop` |
| `availability` | Availability | unreachable when host stops advertising / tunnel drops (FR-014) |
| `availability_reason` | Option\<String\> | stated reason when degraded/unavailable, surfaced in the shell hosts indicator (FR-028) |

### EventRecord
Captured output, task-status changes, and lifecycle events for a session (FR-018).

| Field | Type | Notes |
|-------|------|-------|
| `id` | EventId | monotonic per session |
| `session_id` | SessionId | |
| `timestamp` | Timestamp | |
| `kind` | EventKind | `Output | TaskStatusChange | Lifecycle | OperatorAction` (start/stop/input-sent/confirm/clean-up — feeds the session timeline, FR-019a) |
| `payload` | EventPayload | output goes to capture file (ref), not inline blob, for volume |

**Rule**: output passes through redaction before persistence — secrets are never written (FR-031, R9).

### ResourceUsageMetric
Observed consumption for a session's environment (FR-019).

| Field | Type | Notes |
|-------|------|-------|
| `session_id` | SessionId | |
| `metric` | MetricKind | `Cpu | Memory | Disk | Time` |
| `value` | f64 | |
| `timestamp` | Timestamp | |

Recent samples are retained per session so the session view can show short-horizon history/trends
(FR-019); persistence granularity is an implementation choice.

### AttentionItem ("Needs you" entry) — derived, not persisted
Computed from live session state for the Needs-you queue (FR-021a/b, US6); never stored.

| Field | Type | Notes |
|-------|------|-------|
| `session_id` | SessionId | activation deep-links to the session with the matching affordance focused |
| `kind` | AttentionKind | `WaitingForInput | AwaitingConfirmation | Stalled | Failed | Disconnected` |
| `cue` | String | reason line (pending question for input; exit summary for confirmation; stall/fail/drop reason otherwise) |
| `waiting` | Duration | now − `waiting_since` (or time in state) |
| `cost` | CostIndication | `Idle(estimate)` for a live env with a configured backend `idle_rate`; `EnvHeld` for an ended session holding its env; `None` when no rate configured |

Ordering: `WaitingForInput` → `AwaitingConfirmation` → `Stalled` → `Disconnected` → `Failed` (most
answerable first), newest-waiting last within a kind (matches the design prototype's `needsYou()`).

## Relationships

```
AgenticTool 1───* Session *───1 Objective 1───* TrackedTask
                    │   │
        SandboxEnvironment   Source/Host
                    │
        EnvironmentBackend
Session 1───* EventRecord
Session 1───* ResourceUsageMetric
```

## Persistence notes (SQLite)

- Tables mirror the entities above; `EventRecord.payload` for `Output` stores a reference to a per-session
  capture file rather than the bytes (excessive-output edge case).
- All session-bearing records are retained until the operator deletes them — no auto-expiry (FR-030a).
- On startup, reconciliation re-derives each non-terminal session's status from backend + zellij liveness
  and writes the corrected status (FR-029, FR-030, SC-007).
