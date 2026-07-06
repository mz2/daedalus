# Feature Specification: Orchestrate and Monitor Agentic Tools in Sandbox Environments

**Feature Branch**: `001-agent-orchestration`  
**Created**: 2026-06-06  
**Updated**: 2026-07-06 — incorporates the completed design prototype (see `design/README.md` and
`design/storyboards.md`): waiting-for-input status, Needs-you attention queue, aggregate tasks-board
home, session event timeline, visible output trimming, system theme, degraded availability.  
**Status**: Draft  
**Input**: User description: "Daedalus is a tool to orchestrate and monitor agentic tools running in Canonical Workshop (https://ubuntu.com/workshop, see also project documentation) environments, and potentially in other sandbox environments (for example some suitable for macos)."

## Clarifications

### Session 2026-06-06

- Q: What is the primary surface through which an operator interacts with Daedalus? → A: A web application with an embedded terminal and a per-session task-status board. Operators discover agent sessions from the local host, from other hosts advertising over mDNS, and from connected/tunneled Workshop environments on those hosts; sessions are reached and attached through the zellij terminal multiplexer; agent sessions can be started in Workshop environments directly from the UI; an SDK advertises a connectable service from inside a Workshop so Daedalus can discover and connect to it.
- Q: How should agent credentials/secrets be handled in v1? → A: The operator pre-provisions any secrets in the environment/backend; Daedalus does not manage or inject agent secrets, and must avoid capturing them in persisted logs/output.
- Q: What is the v1 scope for suspending a running agent (pause/resume)? → A: Stop and send-input only; pause/resume is out of scope for v1.
- Q: How does an operator specify the task/objective given to an agent? → A: Follow a spec-driven-development (SDD) tool convention (e.g., SpecKit) — a session's objective and its tracked tasks are derived from SDD artifacts rather than ad-hoc free-form prompts.
- Q: What delivery surfaces must the application support? → A: The application's frontend MUST be implemented with technology that allows it to run both as a native (desktop) application and as a web application from a shared codebase; both surfaces offer the same capabilities (embedded terminal, task-status board, discovery, orchestration, and control).
- Q: How is access to Daedalus and the sessions it exposes controlled in v1? → A: Local-first, single operator. The application binds to the operator's local/loopback context by default with no open network listener; remote reach to other hosts and Workshop environments is over authenticated tunnels the operator establishes. No multi-user accounts or authorization in v1.
- Q: Where does the task-status board get each tracked task's status? → A: Read task states from the session's spec-driven-development artifacts (e.g., SpecKit `tasks.md` checkbox states) in the workspace, surfaced via the in-Workshop SDK.
- Q: How does an agentic tool become available to select and launch? → A: The operator registers agentic tools via declarative definitions (name, how to invoke the tool inside a sandbox, capabilities such as whether it accepts interactive input); registered tools become selectable when starting a session.
- Q: When an operator starts a new session, what hosts the agent? → A: The operator may target either a fresh environment provisioned by Daedalus or a pre-existing environment they select. When a pre-existing environment is targeted, the objective's tracked tasks must direct the agent to work in isolated git worktrees so pre-existing work is not disturbed. All sessions are managed inside zellij, and zellij's own web service may serve as the embedded terminal UI to keep implementation scope small.
- Q: How should "genuinely native desktop GUI (no Electron)" reconcile with the native+web shared-codebase requirement? → A: A shared application core (sessions, backends, discovery, state) with separate thin presentation layers — a genuinely native desktop GUI per platform (macOS and Linux) and a separate web UI. "Shared codebase" means the shared core, not shared UI rendering. Electron is excluded, and the desktop GUI must use genuinely native UI technologies (not a bundled or system-webview shell).
- Q: For the macOS desktop app in v1, what hosts an agent session (given Workshop is Linux-only)? → A: Both — v1 supports orchestrating remote Linux Workshop environments over authenticated tunnels AND a macOS-specific local sandbox backend. Both Canonical Workshop and a macOS-specific sandbox backend are therefore required for v1.
- Q: How is a session's "completed" terminal state determined? → A: A session is "completed" when all its tracked tasks are done, or when the operator explicitly confirms completion. The agent's process exiting is not by itself "completed": if the agent's run ends without all tracked tasks done (and without failure), the session enters an "awaiting confirmation" state that the operator resolves (confirm → completed, or stop/clean up). Agent crash or non-zero exit is recorded as failed.
- Q: How long are ended sessions and their captured output retained in v1? → A: Retained until the operator deletes them — no automatic expiry; cleanup is operator-initiated.

### Session 2026-07-06 (decisions realized in the completed design prototype)

- Q: How is an agent that blocks mid-run on a question to the operator represented? → A: As a
  first-class session status, **"waiting for input"** — distinct from "awaiting confirmation". The
  agent's pending question is captured and shown, the session shows how long it has been waiting, and
  answering happens in the embedded terminal with the pending question brought into view and the input
  focused ("answer mode").
- Q: How does the operator find everything currently blocked on them? → A: A unified **"Needs you"
  attention queue** aggregating every session blocked on the operator — waiting for input, awaiting
  confirmation, stalled, failed, or disconnected — each with a reason cue, the agent's question or exit
  summary where applicable, waiting duration, and the cost of waiting. Each entry jumps in one action to
  the session with the matching affordance focused.
- Q: Should the cost of leaving a blocked session waiting be visible? → A: Yes. Live environments show
  an estimated idle cost derived from a configurable per-backend idle rate; ended sessions that still
  hold an environment are marked "environment held" instead. Informational estimates, not billing.
- Q: What is the application's landing view? → A: An **aggregate tasks board** across all sessions
  (every tracked task, its session and status, with attention flags) — the operator's primary question
  is "what are the agents working on / what needs me". The session-level fleet view remains one
  navigation action away. (Supersedes the design brief's §5 Fleet-first ordering.)
- Q: What theme options exist? → A: Dark, light, and **system** (follows the operating system's
  appearance); system is the default.
- Q: How granular is host/backend availability? → A: **available / degraded / unavailable**, with a
  stated reason for degradation (e.g., memory pressure), surfaced in the app shell's hosts indicator,
  on source/backend chips, and on the Environments screen.
- Q: How is excessive terminal output handled? → A: The embedded terminal stays responsive by limiting
  displayed scrollback and showing a visible notice ("showing last N of M lines · full log persisted");
  the persisted record still captures the full output.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Start an agent session in a sandbox from the UI (Priority: P1)

An operator opens the Daedalus application (running natively or in a browser), selects a registered
agentic tool and an objective (defined following a spec-driven-development convention such as SpecKit),
chooses whether to run in a fresh environment or a pre-existing one, and starts a session inside an
isolated Workshop sandbox — directly from the UI, without the agent touching the operator's host
machine.

**Why this priority**: This is the core value of the product. Starting an autonomous agent safely
inside a sandbox, from a single tool, is the minimum that delivers value. Every other capability
builds on top of a launched, isolated session.

**Independent Test**: Can be fully tested by selecting one agentic tool and an SDD-defined objective
in the application, starting a session, and confirming the agent runs inside a Workshop sandbox while the
host filesystem and processes remain untouched. Delivers a safely-running agent even with no other
feature present.

**Acceptance Scenarios**:

1. **Given** an available sandbox backend and a registered agentic tool, **When** the operator starts
   a session with an objective from the UI and chooses a fresh environment, **Then** Daedalus provisions
   an isolated environment, starts the agent inside it within zellij, and reports the session as
   "running" with a unique identifier.
2. **Given** a pre-existing environment the operator can select, **When** the operator starts a session
   targeting it, **Then** Daedalus starts the agent inside that environment within zellij and the
   objective's tracked tasks direct the agent to work in an isolated git worktree so pre-existing work
   is not disturbed.
3. **Given** a session has started, **When** the operator inspects it, **Then** Daedalus shows which
   agentic tool, objective, and environment the session is bound to.
4. **Given** no sandbox environment can be provisioned (or the selected pre-existing one is
   unreachable), **When** the operator attempts to start a session, **Then** Daedalus reports the
   failure with a clear reason and does not leave a partial or orphaned session.

---

### User Story 2 - Monitor a running agent in real time (Priority: P2)

While an agent runs, the operator watches its progress in the application: an embedded terminal showing
live output, a per-session task-status board reflecting the state of each tracked task, resource
usage, and whether the session has completed, failed, or stalled.

**Why this priority**: Autonomous agents are opaque and long-running; visibility is what makes them
trustworthy to operate. Monitoring is the second half of the product's stated purpose
("orchestrate **and monitor**") and is essential immediately after a session can be launched.

**Independent Test**: Can be fully tested by starting a session and observing the embedded terminal
and the task-status board update as the agent works, plus a terminal state
(completed/failed/stalled) when it finishes. Delivers operator confidence without requiring control,
discovery, or multi-backend features.

**Acceptance Scenarios**:

1. **Given** a running agent, **When** the operator opens its session view, **Then** Daedalus shows
   the embedded terminal streaming live output and a task-status board that updates as tracked task
   states change.
2. **Given** a running agent, **When** all of the session's tracked tasks become done, **Then** Daedalus
   records the session as completed; **and when** the agent's run instead ends without all tracked tasks
   done (and without failure), Daedalus marks the session "awaiting confirmation" for the operator to
   resolve; **and when** the agent crashes or exits abnormally, or stops producing output for a
   configured interval, Daedalus records it as failed or stalled respectively.
3. **Given** a session has ended, **When** the operator reviews it later, **Then** Daedalus shows the
   captured output, tracked-task history, and final status from a persisted record.

---

### User Story 3 - Discover and connect to sessions across hosts and tunneled workshops (Priority: P3)

The operator sees agent sessions that exist beyond the ones they just started — running on the local
host, on other hosts advertising over mDNS, and inside connected/tunneled Workshop environments — and
connects to any of them from the application to observe or interact via its embedded terminal.

**Why this priority**: The product's purpose is to monitor agentic tools running in Workshop
environments, which are often started elsewhere or on other machines. Discovery makes those sessions
reachable from one place. It depends on the session/monitoring foundation (US1, US2) already existing.

**Independent Test**: Can be fully tested by having sessions running on the local host, on an
mDNS-advertised host, and inside a tunneled Workshop, then confirming each is discovered, listed, and
connectable from the application's embedded terminal. Delivers cross-host visibility.

**Acceptance Scenarios**:

1. **Given** sessions running on the local host, on an mDNS-advertised host, and inside a tunneled
   Workshop, **When** the operator opens the discovery view, **Then** Daedalus lists each discovered
   session with its source and status.
2. **Given** a discovered session, **When** the operator connects to it, **Then** Daedalus attaches to
   the session through the zellij terminal multiplexer and renders it in the embedded terminal.
3. **Given** the same session is advertised through more than one source, **When** the operator views
   discovery, **Then** Daedalus shows it once rather than as duplicates.

---

### User Story 4 - Intervene in and control a session's lifecycle (Priority: P4)

The operator stops a running agent, sends it guidance/input, and tears down its environment when
finished.

**Why this priority**: Orchestration implies control, not just observation. Stopping a misbehaving
agent and reclaiming its environment is important for safe operation but depends on the session being
launched, monitored, and reachable.

**Independent Test**: Can be fully tested by starting a session, issuing stop and send-input commands,
and confirming the agent and its environment respond accordingly. Delivers operator control on top of
an already-monitored session.

**Acceptance Scenarios**:

1. **Given** a running agent, **When** the operator stops the session, **Then** Daedalus halts the
   agent, tears down or releases the environment, and records the session as stopped by operator.
2. **Given** a running agent that accepts input, **When** the operator sends guidance or input,
   **Then** Daedalus delivers it to the agent and reflects the agent's response in the embedded
   terminal.
3. **Given** a stopped or finished session, **When** the operator cleans it up, **Then** Daedalus
   releases the associated environment and frees its resources without affecting other sessions.

---

### User Story 5 - Manage a fleet across one or more sandbox backends (Priority: P5)

The operator runs several agents at once and sees them all in one place, across more than one kind of
sandbox backend (Canonical Workshop, and other backends such as one suitable for macOS).

**Why this priority**: Scaling to many concurrent agents and to additional environment backends is the
growth path beyond the core loop, but the product is already valuable with one backend and a few
sessions, so this is the lowest priority of the set.

**Independent Test**: Can be fully tested by starting multiple sessions (on the same and, where
available, a second backend) and confirming a single unified view shows each session's identity,
backend, and status independently. Delivers fleet-level visibility and backend portability.

**Acceptance Scenarios**:

1. **Given** multiple active sessions, **When** the operator views the fleet, **Then** Daedalus lists
   every session with its agentic tool, objective, backend, environment, and current status.
2. **Given** more than one configured sandbox backend, **When** the operator starts a session, **Then**
   the operator can choose which backend hosts it, and the same orchestrate/monitor/control workflow
   applies regardless of backend.
3. **Given** a configured backend is unavailable, **When** the operator views or starts sessions,
   **Then** Daedalus clearly marks that backend as unavailable and continues to operate sessions on
   other backends.

---

### User Story 6 - Clear the queue of agents blocked on you (Priority: P6)

An operator glances at a single "Needs you" queue to see every session that is blocked on them — an
agent that asked a question, a run that ended awaiting confirmation, a stalled or failed run, a
disconnected environment — with how long each has been waiting and what the wait is costing, and jumps
straight into the right affordance (the terminal with the agent's question focused, or the
confirm-completion controls).

**Why this priority**: This is the operator-attention layer over monitoring (US2) and control (US4):
it makes running many concurrent agents sustainable by turning "what needs me right now?" into one
glance and one action. It depends on session statuses, notifications, and send-input/confirmation
already existing, so it lands after the stories it builds on.

**Independent Test**: With sessions seeded in waiting-for-input, awaiting-confirmation, stalled,
failed, and disconnected states, confirm each appears in the queue with its cue, waiting duration, and
waiting-cost indication, and that activating an entry lands in that session with the matching
affordance focused. Delivers a complete blocked-on-operator triage loop.

**Acceptance Scenarios**:

1. **Given** a running agent that accepts interactive input asks the operator a question, **When** the
   operator opens the Needs-you queue, **Then** the session is listed as waiting for input with the
   agent's question, how long it has been waiting, and an estimated idle cost for its live environment.
2. **Given** an entry in the queue, **When** the operator activates it, **Then** the application opens
   that session with the relevant affordance focused: the embedded terminal's input with the pending
   question in view (waiting for input), or the confirm-completion controls (awaiting confirmation).
3. **Given** a session that ended awaiting confirmation while still holding its environment, **When**
   the operator views the queue, **Then** the entry is marked as holding an environment rather than
   accruing live idle cost.
4. **Given** no sessions are blocked on the operator, **When** the operator opens the queue, **Then**
   it states that nothing needs them rather than showing a bare empty region.

---

### Edge Cases

- **Environment provisioning failure**: The backend cannot create or attach a sandbox — the session
  must fail cleanly with a reason and leave no orphaned environment.
- **Agent crash or non-zero exit**: The session is marked failed and the final output/exit reason is
  captured.
- **Agent exits with tracked tasks incomplete**: A clean agent exit that leaves tracked tasks unfinished
  does not auto-complete; the session is surfaced as "awaiting confirmation" for the operator to confirm
  completion or clean up, rather than silently shown as completed.
- **Agent stall/hang**: An agent that stops making progress or producing output for a configured
  interval is surfaced as stalled so the operator can intervene.
- **Agent waits on input indefinitely**: A session waiting for operator input is distinct from a stall —
  its environment stays alive and it is surfaced with its pending question, waiting duration, and idle
  cost rather than timing out silently or being misreported as stalled.
- **Ended session holds an environment**: A session that ended awaiting confirmation (or was not yet
  cleaned up) and still holds its environment is visibly marked as holding it until the operator
  confirms or cleans up.
- **Resource exhaustion**: A sandbox that exceeds its allotted CPU/memory/disk/time is surfaced and the
  session is stopped according to the configured limit policy.
- **Connection loss to the environment**: Loss of contact with a running sandbox is detected and
  reported; the session's last-known state is preserved rather than silently lost.
- **mDNS host disappears**: A host that stops advertising is removed from discovery, and its
  previously-discovered sessions are marked unreachable rather than appearing healthy.
- **Tunnel to a Workshop drops**: Loss of a tunnel to a remote Workshop marks its sessions as
  disconnected; reconnection re-establishes discovery and attachment.
- **zellij unavailable / session not multiplexed**: A discovered session that cannot be attached
  through zellij is shown as non-attachable with a clear reason, rather than failing silently.
- **SDK service not advertised**: A Workshop whose in-environment SDK service is not advertised is not
  discoverable; the operator is told why instead of seeing an empty list with no explanation.
- **Duplicate advertisement**: A session reachable through more than one source is de-duplicated to a
  single entry.
- **Excessive output**: An agent producing very large or rapid output does not overwhelm the embedded
  terminal or cause loss of the persisted record.
- **Concurrency limit reached**: Starting more sessions than allowed is rejected with a clear reason
  rather than degrading existing sessions.
- **Backend unavailable mid-session**: A backend that goes away while sessions are active leaves those
  sessions in a clearly-marked unknown/disconnected state rather than appearing healthy.
- **Isolation breach attempt**: An agent that attempts to reach beyond its sandbox (host filesystem,
  network, processes) is contained by the environment's isolation guarantees.
- **Pre-existing environment, no trampling**: A session on a pre-existing environment works in an
  isolated git worktree so concurrent or pre-existing work is not disturbed; if the worktree cannot be
  created, the session fails with a clear reason rather than operating in place.
- **Daedalus restart**: After Daedalus itself restarts, previously-started sessions are reconciled —
  still reflected with their correct status rather than lost.

## Requirements *(mandatory)*

### Functional Requirements

**Orchestration**

- **FR-001**: System MUST allow an operator to start a session by selecting a registered agentic tool
  and an objective defined following a spec-driven-development tool convention (e.g., SpecKit).
- **FR-001a**: Operators MUST be able to register agentic tools via declarative definitions (name, how
  to invoke the tool inside a sandbox, and capabilities such as whether it accepts interactive input);
  registered tools become selectable when starting a session.
- **FR-002**: System MUST let the operator choose, when starting a session, between provisioning a fresh
  isolated sandbox environment and targeting a pre-existing environment the operator selects, then start
  the selected agentic tool inside that environment.
- **FR-002a**: When a session targets a pre-existing environment, the objective's tracked tasks MUST
  direct the agent to work in an isolated git worktree so that pre-existing work in that environment is
  not disturbed; if the worktree cannot be created, the session MUST fail with a clear reason rather
  than proceeding in place.
- **FR-003**: System MUST assign each session a unique identifier and bind it to its agentic tool,
  objective, and environment.
- **FR-004**: System MUST ensure an agent's execution is confined to its sandbox and does not act on
  the operator's host environment.
- **FR-005**: System MUST fail a session cleanly — with a stated reason and no orphaned environment —
  when an environment cannot be provisioned or the agent cannot be started.
- **FR-006**: Operators MUST be able to start agent sessions in a Workshop environment directly from
  the application.

**Interface (Application, Embedded Terminal, Task Board)**

- **FR-007**: System MUST provide a Daedalus application as the primary operator interface for
  orchestrating and monitoring sessions.
- **FR-007a**: The application MUST be structured as a shared application core (sessions, backends,
  discovery, state) with separate thin presentation layers. Surfaces are delivered in phases: the **native
  desktop GUI is the first surface** and a **web UI is a planned subsequent surface**. The core MUST remain
  surface-agnostic so the web UI is additive (no orchestration logic in any surface). The same operator
  capabilities MUST be available on each **delivered** surface (capability parity across delivered
  surfaces). ("Shared codebase" refers to the shared core, not shared UI rendering.)
- **FR-007b**: The desktop GUI MUST be implemented with genuinely native UI technologies and MUST NOT use
  Electron or a bundled/system-webview shell for its UI; it MUST run as a native desktop application on
  both macOS and Linux.
- **FR-008**: System MUST embed a terminal through which the operator interacts with a session's live
  terminal on every surface: a native terminal view attaching to zellij on the desktop GUI, and zellij's
  own web service (or equivalent) on the web UI.
- **FR-009**: System MUST present a per-session task-status board showing the status of each tracked
  task for that session.
- **FR-009a**: The operator UI MUST follow the project's locked design system (see the UI design brief and
  the `design/` design-system reference). Concretely it MUST: present a platform-native appearance on each
  desktop platform (macOS and Linux); support dark and light themes plus a "system"
  option that follows the operating system's appearance (the default); render every session and task
  status using a color-blind-safe status palette **paired with an icon and label** (status MUST NOT be
  conveyed by color alone); support adjustable information density; and provide keyboard-first operation
  including a command palette. (The specific UI toolkit is a planning decision; this requirement is about
  the operator-visible design language and behavior.)

**Discovery & Connectivity**

- **FR-010**: System MUST discover available agent sessions from (a) the local host, (b) other hosts
  advertising over mDNS, and (c) connected/tunneled Workshop environments on those hosts.
- **FR-011**: System MUST manage all agent sessions within the zellij terminal multiplexer, and MUST
  reach and attach to discovered sessions through zellij and render them in the embedded terminal.
- **FR-012**: System MUST provide an SDK that advertises, from inside a Workshop environment, a
  connectable service that Daedalus can discover and connect to.
- **FR-013**: System MUST de-duplicate a session advertised through more than one source so it appears
  as a single entry.
- **FR-014**: System MUST indicate when a discovered session becomes unreachable (host stops
  advertising, tunnel drops, or attachment fails) rather than showing it as healthy.

**Monitoring**

- **FR-015**: System MUST report each session's current status (e.g., starting, running, waiting for
  input, awaiting confirmation, completed, failed, stalled, stopped, or disconnected/unknown when
  contact with the environment is lost) and update it as the session progresses.
- **FR-015a**: System MUST treat a session as "completed" only when all of its tracked tasks are done or
  the operator explicitly confirms completion — not on agent process exit alone. When the agent's run
  ends without all tracked tasks done and without failure, the session MUST enter an "awaiting
  confirmation" state that the operator resolves (confirm → completed, or stop/clean up); agent crash or
  abnormal exit MUST be recorded as failed. While awaiting confirmation, the system MUST surface the
  agent's exit summary and remaining tracked tasks, and offer confirm-completion and clean-up as the
  primary controls.
- **FR-015b**: When a running agent blocks on a question or approval from the operator (possible only
  for tools that accept interactive input), the system MUST surface the session as "waiting for input" —
  distinct from stalled and from awaiting confirmation — capturing and displaying the agent's pending
  question and how long the session has been waiting. Answering MUST happen through the embedded
  terminal (FR-023), and opening the session from a waiting-for-input cue MUST bring the pending
  question into view with the terminal input focused.
- **FR-016**: System MUST stream a running agent's output to the embedded terminal as it is produced.
- **FR-016a**: Under very large or rapid output the embedded terminal MUST remain responsive by limiting
  displayed scrollback, MUST show a visible notice that the display is trimmed (e.g., "showing last N of
  M lines · full log persisted"), and the persisted record (FR-018) MUST still capture the full output.
- **FR-017**: System MUST derive each tracked task's status from the session's spec-driven-development
  artifacts (e.g., SpecKit `tasks.md` checkbox states) in the workspace, surfaced via the in-Workshop
  SDK, and reflect changes on the task-status board as they occur.
- **FR-018**: System MUST record a session's output, tracked-task history, and terminal outcome to a
  persisted record that remains available after the session ends.
- **FR-019**: System MUST surface resource usage for a session's environment to the operator, including
  recent history (e.g., CPU/memory trends) alongside current values in the session view.
- **FR-019a**: System MUST record and surface a per-session lifecycle/event timeline — status changes,
  key operator actions (start, stop, input sent, confirm, clean-up), and notable agent events — in the
  session view; for an ended session it MUST also present an outcome summary (final state, exit
  reason/code, and where the persisted capture lives).
- **FR-020**: System MUST detect and surface terminal and abnormal conditions — completion, failure,
  stall, and loss of contact with the environment.
- **FR-021**: System MUST notify the operator when a session reaches a terminal or abnormal state, or
  becomes blocked on the operator (waiting for input, awaiting confirmation); notifications MUST allow
  jumping directly to the session concerned.
- **FR-021a**: System MUST provide a unified attention queue ("Needs you") of every session blocked on
  the operator — waiting for input, awaiting confirmation, stalled, failed, or disconnected — each entry
  showing its reason cue (including the agent's pending question or exit summary where applicable), how
  long it has been waiting, and its waiting-cost indication (FR-021b). Activating an entry MUST open the
  session in one action with the matching affordance focused (terminal answer mode for waiting-for-input;
  confirm-completion controls for awaiting confirmation). The queue MUST be reachable from persistent
  navigation with its count surfaced in the application shell, and MUST state clearly when nothing needs
  the operator.
- **FR-021b**: System MUST indicate the cost of leaving a blocked session waiting: for a live
  environment, an estimated idle cost derived from a configurable per-backend idle rate; for an ended
  session that still holds its environment, an explicit environment-held indication. These are
  informational estimates, not billing records.

**Control / Intervention**

- **FR-022**: Operators MUST be able to stop a running agent, after which the system halts the agent
  and releases or tears down its environment. (Pause/resume is out of scope for v1.)
- **FR-023**: Operators MUST be able to send input/guidance to a running agent that accepts it, and the
  system MUST deliver it and reflect the agent's response.
- **FR-024**: Operators MUST be able to clean up a finished or stopped session, releasing its
  environment and freeing resources without affecting other sessions.

**Fleet & Backends**

- **FR-025**: System MUST present a unified view of all sessions (active and recent), each showing its
  agentic tool, objective, source/backend, environment, and status.
- **FR-025a**: System MUST provide an aggregate task board across all sessions — every tracked task with
  its session, tool, backend, and status, filterable (by status, session, tool, backend) and flagging
  tasks whose session is blocked on the operator. This aggregate board is the application's landing
  view; the session-level fleet view (FR-025) is one navigation action away.
- **FR-026**: System MUST support running multiple agents concurrently, up to a configurable limit,
  without sessions interfering with one another.
- **FR-027**: System MUST support more than one kind of sandbox backend through a common backend
  abstraction. For v1 this MUST include both Canonical Workshop (Linux) and a macOS-specific local
  sandbox backend, plus the ability to add further backends, all without changing the operator's
  orchestrate/monitor/control workflow. On macOS, operators MUST also be able to orchestrate remote Linux
  Workshop environments over authenticated tunnels.
- **FR-028**: System MUST detect and clearly indicate when a configured backend or host is unavailable
  or degraded (with a stated reason, e.g., resource pressure), while continuing to operate sessions on
  other available backends and making that continuity explicit to the operator. Availability MUST be
  visible from the application shell as well as the backends/environments view.

**Persistence & Reconciliation**

- **FR-029**: System MUST persist session metadata, status, tracked-task history, and captured output
  so that history survives a restart of Daedalus.
- **FR-030**: System MUST reconcile previously-started sessions after a restart, reflecting their
  correct current status rather than losing them.
- **FR-030a**: System MUST retain persisted session records (metadata, status, tracked-task history, and
  captured output) until the operator deletes them; v1 MUST NOT auto-expire session history, and cleanup
  is operator-initiated.

**Secrets (scope boundary)**

- **FR-031**: System MUST NOT require operators to hand it agent credentials; secrets are pre-provisioned
  by the operator in the environment/backend, and the system MUST avoid capturing such secrets in
  persisted logs or output.

**Access & Security (control plane)**

- **FR-032**: System MUST default to a local-first, single-operator access model — bound to the
  operator's local/loopback context with no open network listener by default.
- **FR-033**: System MUST reach other hosts and Workshop environments only over authenticated tunnels
  established by the operator, rather than exposing an open network service.
- **FR-034**: System scope for v1 MUST NOT include multi-user accounts or authorization; access is the
  single operator's own. (Recorded as an explicit scope boundary.)

### Key Entities *(include if feature involves data)*

- **Session (Agent Session / Run)**: A single invocation of an agentic tool against an objective within
  an environment. Key attributes: unique identifier, status (starting, running, waiting for input,
  awaiting confirmation, completed, failed, stalled, stopped, disconnected/unknown), start/end time,
  terminal outcome, references to its agentic tool, objective, environment, and source; when waiting for
  input: the agent's pending question and waiting-since time; when awaiting confirmation: the agent's
  exit summary and remaining tracked tasks; optionally a reference to an external work item (e.g., a
  GitHub issue or Jira key). "Completed" requires all tracked tasks done or operator confirmation;
  records are retained until the operator deletes them.
- **Agentic Tool**: An operator-registered autonomous agent that Daedalus can launch, defined
  declaratively. Key attributes: name/identifier, how to invoke the tool inside a sandbox, capabilities
  (e.g., whether it accepts interactive input).
- **Objective**: The overall goal given to an agent for a session, expressed following a
  spec-driven-development convention (e.g., SpecKit), and decomposed into tracked tasks. Key attributes:
  description/source artifacts, association to a session.
- **Tracked Task**: An individual unit of work for a session (e.g., a task from a spec-driven-development
  workflow such as SpecKit), shown on the session's task-status board. Key attributes: identifier,
  description, status (derived from the session's SDD artifacts such as a SpecKit `tasks.md`),
  association to a session.
- **Sandbox Environment**: An isolated environment hosting one session — either freshly provisioned per
  session by Daedalus or a pre-existing environment selected by the operator; all sessions are managed
  within zellij. Key attributes: identifier, lifecycle state, fresh-vs-pre-existing origin,
  isolation/resource limits, owning backend.
- **Environment Backend (Provider)**: A source of sandbox environments (Canonical Workshop or another).
  Key attributes: name, availability status (available/degraded/unavailable, with reason),
  capabilities/limits, optional configured idle rate used for waiting-cost estimates (FR-021b).
- **Source / Host**: A place from which sessions are discovered — the local host, an mDNS-advertised
  host, or a connected/tunneled Workshop. Key attributes: identifier, kind (local/mDNS/tunneled),
  availability status (available/degraded/unavailable, with reason).
- **Event / Output Record**: Captured output, actions, and lifecycle events for a session. Key
  attributes: timestamp, session reference, content, type (output, task status change, lifecycle event).
- **Resource Usage Metric**: Observed consumption for a session's environment. Key attributes: session
  reference, metric type (CPU/memory/disk/time), value, timestamp.
- **Attention Item ("Needs you" entry)**: A derived entry representing one session blocked on the
  operator. Key attributes: session reference, kind (waiting for input / awaiting confirmation /
  stalled / failed / disconnected), reason cue (including the agent's pending question or exit summary
  where applicable), waiting duration, waiting-cost indication (estimated idle cost for a live
  environment, or environment-held for an ended one). Derived from session state, not stored
  independently.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: An operator can take an agentic tool from "selected" in the UI to "running in an isolated
  sandbox" in under 2 minutes for a supported backend.
- **SC-002**: 100% of sessions are confined to their sandbox — across normal and adversarial test
  agents, no test agent is able to read or modify the operator's host environment.
- **SC-003**: A running agent's output appears in the embedded terminal within 5 seconds of being
  produced.
- **SC-004**: 100% of sessions reach a recorded terminal state (completed, failed, stalled, or stopped),
  or a clearly-surfaced "awaiting confirmation" state pending operator resolution — no session is left in
  an indeterminate or unknown state once its agent run has ended.
- **SC-005**: An operator can stop a running agent and reclaim its environment within 30 seconds of
  issuing the stop command.
- **SC-006**: An operator can monitor at least 10 concurrent sessions in a single unified view without
  losing status or output for any session.
- **SC-007**: After Daedalus restarts, 100% of previously-started sessions are reflected with their
  correct status, and no persisted session history is lost.
- **SC-008**: Adding a new sandbox backend requires no change to how an operator starts, monitors, or
  controls a session — the same workflow works unchanged across backends.
- **SC-009**: An operator can discover and connect to a running session (on the local host, an
  mDNS-advertised host, or a tunneled Workshop) within 10 seconds of it becoming reachable.
- **SC-010**: A tracked task's status change appears on its session's task-status board within 5 seconds.
- **SC-011**: The application is built over a shared application core (no Electron) with thin presentation
  surfaces. The **first delivered surface** is a genuinely native desktop GUI on both macOS and Linux, on
  which 100% of operator capabilities (start, discover, connect, monitor via embedded terminal and task
  board, control) are available. A **web UI is a planned subsequent surface**; when delivered it MUST offer
  the same capabilities over the same shared core (capability parity across delivered surfaces). See the
  implementation plan for the phased surface scope.
- **SC-012**: The application exposes no open network listener by default — verified by confirming that,
  with no operator-established tunnel, the control plane is reachable only from the operator's local
  context.
- **SC-013**: A session can be started and monitored on Linux using a Canonical Workshop environment and
  on macOS using the macOS-specific local sandbox backend, with the same operator workflow on both.
- **SC-014**: 100% of sessions in a blocked-on-operator state (waiting for input, awaiting confirmation,
  stalled, failed, disconnected) appear in the Needs-you queue within 5 seconds of entering that state,
  and activating any entry lands the operator in that session with the matching affordance focused in a
  single action.
- **SC-015**: When an agent asks a question, the operator can read the question, the waiting duration,
  and the waiting-cost indication from the queue (or its notification) without opening the session.

## Assumptions

- **Backend scope for the first release**: v1 supports two sandbox backends through a common abstraction —
  Canonical Workshop (Linux) and a macOS-specific local sandbox. Workshop does not run on macOS today, so
  macOS operators use the local macOS sandbox backend and/or orchestrate remote Linux Workshops over
  authenticated tunnels. Further backends remain a planned extension.
- **Interface is a shared-core, multi-surface application**: The operator surface is a single Daedalus
  application with an embedded terminal and a per-session task-status board, structured as a shared
  application core with separate presentation layers — a genuinely native desktop GUI (macOS and Linux)
  and a web UI. Electron and webview-shell approaches are excluded for the desktop GUI. The specific
  native UI technology is a planning decision; the requirement is genuinely native desktop UI plus a web
  UI over a shared core, with capability parity — not a particular framework.
- **Operator UI follows a locked design system**: The visual and interaction design is fixed by the
  project design system — the UI design brief plus the completed `design/` prototype (2026-07-03; see
  `design/README.md` for the design→implementation mapping and `design/storyboards.md` for flow
  storyboards). It defines platform-native skins (Ubuntu/Yaru on Linux, Cupertino on macOS), dark/light/
  system themes, adjustable density, a color-blind-safe status palette paired with icon+label (including
  the shared attention-purple for the two blocked-on-operator states, distinguished by glyph and label),
  keyboard-first operation with a command palette, the Needs-you queue, the aggregate tasks-board home,
  and the session telemetry rail (FR-009a and the 2026-07-06 clarifications). Implementation ports this
  design rather than inventing a new visual language. (The current build targets the native desktop
  surface first; see the implementation plan for the phased surface scope.)
- **Sessions managed within zellij**: All agent sessions are managed inside the zellij terminal
  multiplexer and reached/attached through it; zellij's own web service may provide the embedded terminal
  UI on both surfaces, which keeps implementation scope small.
- **Agentic tools are operator-registered**: Tools are made available through operator-supplied
  declarative definitions (name, in-sandbox invocation, capabilities); Daedalus does not auto-discover or
  hard-code a fixed tool set.
- **Worktree isolation for pre-existing environments**: When a session runs in a pre-existing
  environment, its SDD task definitions instruct the agent to use isolated git worktrees so existing work
  is preserved.
- **Discovery mechanisms**: Sessions are discovered on the local host, from other hosts advertising over
  mDNS on the local network, and from connected/tunneled Workshop environments; an SDK runs inside a
  Workshop to advertise a connectable service.
- **Environment lifecycle**: The sandbox backend is responsible for actually creating and isolating
  environments; Daedalus drives their lifecycle (provision/attach, use, release) through the backend
  rather than implementing isolation itself.
- **Agentic tools are externally defined**: The autonomous agents Daedalus launches are existing tools
  configured for use; Daedalus orchestrates and observes them but does not implement the agents' own
  reasoning.
- **Objectives follow an SDD convention**: A session's objective and its tracked tasks are expressed
  following a spec-driven-development tool convention (e.g., SpecKit), rather than ad-hoc free-form
  prompts.
- **Secrets are operator-provisioned**: The operator pre-provisions any credentials/secrets the agent
  needs in the environment/backend; Daedalus does not manage or inject them and avoids capturing them in
  persisted logs/output.
- **Completion is task- or operator-driven**: A session completes when all tracked tasks are done or the
  operator confirms; a clean agent exit with unfinished tasks awaits operator confirmation rather than
  auto-completing.
- **History retained until deleted**: Ended-session records are kept until the operator deletes them;
  there is no automatic retention/expiry policy in v1 (the operator manages disk usage via cleanup).
- **Single-operator, local-first by default**: The initial scope targets a single operator orchestrating
  their own sessions. The application binds to the operator's local/loopback context with no open network
  listener by default; reach to other hosts and Workshops is over authenticated tunnels the operator
  establishes. Multi-user accounts, authorization, and team collaboration are out of scope for v1 unless
  later specified.
- **Connectivity**: The machine running Daedalus can reach each configured backend's control interface
  and the discovery transports (mDNS, tunnels); transient loss of connectivity is treated as an abnormal
  condition to surface, not a normal mode.
- **Interactive input is capability-dependent**: Not every agentic tool accepts mid-session input;
  control features that require it apply only to agents that support it, and the system degrades
  gracefully for those that do not. The "waiting for input" status (FR-015b) can therefore only arise
  for tools registered as accepting interactive input.
- **Waiting-cost rates are informational estimates**: Per-backend idle rates used for the Needs-you
  waiting-cost indication (FR-021b) are operator-configured approximations intended to prompt timely
  triage; Daedalus does not meter or bill actual spend.
- **Prototype-only backend illustrations**: The design prototype seeds two backend types beyond the v1
  scope — a GPU backend ("NVIDIA OpenShell") and an unsandboxed "on host" run mode — as design
  exploration of the FR-027 extension point. GPU backends remain a planned extension; an unsandboxed
  on-host mode conflicts with FR-004/SC-002 (sandbox isolation is absolute) and is explicitly **not**
  part of this specification.
