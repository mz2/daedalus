# Tasks: Orchestrate and Monitor Agentic Tools in Sandbox Environments

**Input**: Design documents from `/specs/001-agent-orchestration/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Tests**: REQUIRED. Constitution Principle I (Red/Green TDD, NON-NEGOTIABLE) means every behavioral task is
preceded by a test written FIRST and demonstrated to FAIL before implementation. Pure refactors are exempt
but MUST keep the suite green.

**Scope note**: Per `plan.md` / `research.md` §R1, the **web surface is deferred**; the single surface is the
native GPUI desktop app (`apps/desktop`). The core stays surface-agnostic so a web UI is additive later.

**Design note**: All GPUI screen/component tasks implement the **locked design system** in
[`design/`](../../design/) — tokens, skins (Yaru/mac), themes, status palette, and per-screen layouts are
specified in `design/README.md`, mapped from the **canonical runnable prototype `design/prototype/`**
(completed 2026-07-03; the single-file standalone export is superseded). Port it; the foundational
theme/component task is T017. Screen→task mapping is in `design/README.md`. Per **constitution v1.1.0
Principle IV**, every UI task includes a validate-against-prototype step: compare the running GPUI screen
with the prototype rendering of the same screen/state (reproduction paths in `design/storyboards.md`) and
record any deviation in `design/README.md` — never silent drift.

**Organization**: grouped by user story (US1–US6) for independent implementation and testing.

**Implementation status (2026-06-19)**: the full headless stack — `daedalus-proto`, the backend
trait + fake/Workshop/macOS backends, persistence, the state machine, redaction, discovery
(+SDK), the orchestration core, and the `daedalus-app` API — is implemented test-first and is
**green** (`cargo test --workspace`: 86 passing; `clippy --workspace -D warnings` clean). The GPUI
desktop surface ships its **design-system port + screen view-models as compiled, unit-tested Rust**
(`apps/desktop`), with the actual **GPUI rendering behind the non-default `gpui` feature** (research
R-UI: GPUI needs a GPU/display + per-platform validation); the default headless build runs and is
CI-checked. **T065** (a11y) is done at the design-system/view-model layer: a tested WCAG-AA contrast
matrix across both themes × both skins, legible on-pill foreground selection, status never color-only
(glyph+label+accessible label), and AccessKit/keyboard data (`accessible_label()` on badges/buttons/nav,
`keyboard_shortcuts()`) — the live AccessKit *wiring* and focus handling land with the `gpui` renderer.
**T067**: the quickstart ran clean end-to-end on **Linux** (`build`/`run`/`test`/`lint` all green, 86
tests); the **macOS** leg and the live GPUI run still need that hardware. **T066** (perf validation
against the SC numbers) remains unchecked — it needs the real backends on real hardware.

**Update (2026-07-06)**: the spec incorporated the completed design prototype (US6 "Needs you",
FR-015b/016a/019a/021a/021b/025a, system theme, degraded availability) and the constitution added
Principle IV (design fidelity). **Phases 9–10 below are the new, unstarted work**; T068's consistency
verification and the paragraph above describe the pre-update state and are superseded by T087.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no incomplete dependencies)
- **[Story]**: US1–US5 for story-phase tasks; Setup/Foundational/Polish have no story label
- Paths follow the Cargo workspace in `plan.md` (`crates/…`, `apps/desktop/…`, `tests/…`)

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: workspace, toolchain, and the local runtime path that all later work depends on.

- [X] T001 Create the Cargo workspace and crate skeletons (`Cargo.toml` + `crates/{daedalus-core,daedalus-proto,daedalus-backend,daedalus-backend-workshop,daedalus-backend-macos,daedalus-backend-fake,daedalus-discovery,daedalus-zellij,daedalus-sdk,daedalus-app}`, `apps/desktop`, `tests/{contract,integration}`) per plan.md
- [X] T002 Add and pin workspace dependencies in `Cargo.toml` (`tokio`, `serde`/`serde_json`, `gpui` + `gpui-component` at pinned git revs, `alacritty_terminal`, `rusqlite`/`sqlx`, `mdns-sd`, `async-trait`); document the pins
- [X] T003 [P] Configure `rustfmt.toml` + `clippy` with warnings-as-errors (`cargo clippy --workspace --all-targets -- -D warnings`) — Constitution Principle II
- [X] T004 [P] Add a `justfile`/`Makefile` with single-command `run` (fake backend), `test`, and `lint` entry points and verify the quickstart.md path works — Constitution Principle III
- [X] T005 [P] Add CI workflow `.github/workflows/ci.yml` running `fmt --check`, `clippy -D warnings`, and `cargo test --workspace` on macOS + Linux

**Checkpoint**: `cargo build --workspace` and the lint/test commands run clean on an empty skeleton.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: shared types, persistence, the backend boundary, the fake backend, the state machine, the
app-API, and the GPUI shell. **No user story can begin until this phase is complete.**

- [X] T006 [P] Define shared id/enum types in `crates/daedalus-proto/src/ids.rs` and `status.rs` (`SessionId`, `ToolId`, …, `SessionStatus`, `TaskStatus`, `Availability`, `Origin`, `SourceKind`, `BackendKind`)
- [X] T007 [P] Define entity structs in `crates/daedalus-proto/src/entities.rs` (Session, AgenticTool, Objective, TrackedTask, SandboxEnvironment, EnvironmentBackend, Source, EventRecord, ResourceUsageMetric) per data-model.md
- [X] T008 [P] Contract test for the `Backend` trait shape (C-B1…C-B5) in `tests/contract/backend.rs` — write FIRST, must fail
- [X] T009 Define the `Backend` async trait + value types in `crates/daedalus-backend/src/lib.rs` per contracts/backend.md (makes T008 compile/fail meaningfully)
- [X] T010 [P] Test the SQLite schema + migrations (tables for sessions, tasks, events, metrics; output stored as capture-file refs) in `tests/contract/persistence.rs` — write FIRST, must fail
- [X] T011 Implement persistence layer + migrations in `crates/daedalus-core/src/persist/` (SQLite via rusqlite/sqlx; file-backed output capture) per data-model.md
- [X] T012 [P] Test the session lifecycle state machine transitions (Starting→Running→{Completed|Failed|Stalled|Stopped|AwaitingConfirmation}; completion only on all-tasks-done or confirm — FR-015a) in `tests/unit/state_machine.rs` — write FIRST, must fail
- [X] T013 Implement the session state machine in `crates/daedalus-core/src/session/state.rs`
- [X] T014 Implement the `fake` in-memory backend in `crates/daedalus-backend-fake/src/lib.rs` (drives all contract/integration tests locally — Principle III)
- [X] T015 [P] Implement the secret-redaction-on-capture utility in `crates/daedalus-core/src/redact.rs` with a failing test first in `tests/unit/redact.rs` (FR-031)
- [X] T016 Define the app-API surface (`Command`, `AppQuery`, `AppEvent`) in `crates/daedalus-app/src/api.rs` per contracts/app-api.md (no orchestration logic in surfaces)
- [X] T017 [P] Port the design system to GPUI in `apps/desktop/src/theme.rs` + `apps/desktop/src/components/` — tokens for Yaru/mac skins × dark/light themes × density, status palette (incl. awaiting-confirmation), typography (Ubuntu/JetBrains Mono), and core components (status badge, buttons, chips, cards, grouped list, meters, nav, titlebar) per `design/README.md` and `design/Daedalus-Prototype-standalone.html`
- [X] T017a Implement the GPUI app shell in `apps/desktop/src/app.rs` (skin-correct titlebar + window controls, sidebar/topbar nav with Hosts list, global status counts, command palette ⌘K, notifications, settings, dark/light theme) per `design/README.md` + design-brief §6.1, wired to `daedalus-app` (depends on T017)

**Checkpoint**: foundation builds; fake backend + persistence + state machine are exercised by failing→passing tests; the desktop shell opens.

---

## Phase 3: User Story 1 - Start an agent session in a sandbox from the UI (Priority: P1) 🎯 MVP

**Goal**: select a registered tool + an SDD objective, choose fresh/pre-existing environment, and start an
isolated session from the GPUI app.

**Independent Test**: start a session against the `fake` backend (and a real backend where available),
confirm it reports "running" with a unique id while the host filesystem/processes stay untouched.

### Tests for User Story 1 (write FIRST, must fail) ⚠️

- [X] T018 [P] [US1] Contract test for `Command::RegisterTool` + `Command::StartSession` (C-A1: failure leaves no record) in `tests/contract/app_start.rs`
- [X] T019 [P] [US1] Integration test: start fresh-env session → running + unique id (AS1); pre-existing → isolated worktree (AS2, C-B1); unprovisionable → clean failure, no orphan (AS4, C-B2) in `tests/integration/start_session.rs`
- [X] T020 [P] [US1] Adversarial isolation integration test: a hostile agent cannot read/modify the host (SC-002, FR-004) in `tests/integration/isolation.rs`

### Implementation for User Story 1

- [X] T021 [P] [US1] Implement the agentic-tool registry (declarative tool defs, `RegisterTool`) in `crates/daedalus-core/src/tools.rs` (FR-001a)
- [X] T022 [US1] Implement the start-session flow in `crates/daedalus-core/src/session/start.rs` (bind tool/objective/env/source, assign unique id, fresh-vs-pre-existing, worktree for pre-existing — FR-001/002/002a/003/005)
- [X] T023 [US1] Wire `StartSession`/`RegisterTool` through `daedalus-app` to core (FR-006)
- [X] T024 [P] [US1] Implement the Workshop backend `acquire`/`start_agent` in `crates/daedalus-backend-workshop/src/lib.rs` (Linux; confirm control surface — R-WS)
- [X] T025 [P] [US1] Implement the macOS sandbox backend `acquire`/`start_agent` in `crates/daedalus-backend-macos/src/lib.rs` (Seatbelt/App Sandbox; must meet SC-002 — R-MAC)
- [X] T026 [US1] Build the GPUI Start-session flow screen in `apps/desktop/src/screens/start.rs` (tool pick → SDD objective ref → env choice w/ worktree note → backend choice → launch) per design-brief §6.3, incl. validation/provisioning-failure states

**Checkpoint**: US1 is independently demoable on the fake backend — the MVP.

---

## Phase 4: User Story 2 - Monitor a running agent in real time (Priority: P2)

**Goal**: live embedded terminal + task-status board + resource usage + terminal-state detection.

**Independent Test**: start a session and watch the terminal stream and the task board update, then see a
recorded terminal state (completed/awaiting-confirmation/failed/stalled) when the run ends.

### Tests for User Story 2 (write FIRST, must fail) ⚠️

- [X] T027 [P] [US2] Contract test for terminal attach (C-T1 latency, C-T2 unattachable reason, C-T3 excessive output) in `tests/contract/terminal_attach.rs`
- [X] T028 [P] [US2] Contract test for `WorkshopSdk::task_states` parsing SpecKit `tasks.md` checkbox state (C-S1, ≤5s) in `tests/contract/sdk_task_states.rs`
- [X] T029 [P] [US2] Integration test: output streams to the terminal (FR-016), board reflects task changes (FR-017), terminal-state determination incl. awaiting-confirmation (FR-015a), and the session record (output + task history + outcome) is persisted (FR-018, covers T033) in `tests/integration/monitor.rs`
- [X] T029a [P] [US2] Integration test: resource usage (CPU/memory/disk/time) surfaces to `AppQuery` for a running session (FR-019, covers T034) in `tests/integration/resource_usage.rs`
- [X] T029b [P] [US2] Integration test: the operator is notified when a session reaches a terminal/abnormal state (FR-021, covers T036) in `tests/integration/notifications.rs`

### Implementation for User Story 2

- [X] T030 [US2] Implement zellij attach + output streaming in `crates/daedalus-zellij/src/attach.rs` exposing `TerminalAttach` per contracts/terminal-attach.md (redacted capture, FR-016)
- [X] T031 [US2] Implement the SDD task-status reader (parse `tasks.md` checkbox states) in `crates/daedalus-core/src/tasks/board.rs` (FR-017)
- [X] T032 [US2] Implement terminal-state + stall/failure detection and the `AwaitingConfirmation` path in `crates/daedalus-core/src/session/outcome.rs` (FR-015/015a/020)
- [X] T033 [US2] Persist output, task history, and outcome to the record and emit `AppEvent`s (FR-018, FR-021)
- [X] T034 [P] [US2] Implement resource-usage surfacing via `Backend::resource_usage` plumbed to `AppQuery` (FR-019)
- [X] T035 [US2] Build the GPUI Session-detail screen in `apps/desktop/src/screens/session.rs` — embedded terminal (alacritty_terminal view), task-status board, telemetry rail, status badge, per design-brief §6.4
- [X] T036 [P] [US2] Implement the notifications affordance for terminal/abnormal events in `apps/desktop/src/screens/notifications.rs` (FR-021)

**Checkpoint**: US1 + US2 work independently — start and fully observe a session.

---

## Phase 5: User Story 3 - Discover and connect to sessions across hosts and tunneled workshops (Priority: P3)

**Goal**: discover sessions (local / mDNS / tunneled Workshop), de-duplicated, and attach to any.

**Independent Test**: with sessions on local host, an mDNS host, and a tunneled Workshop, confirm each is
discovered once with its source/status and is connectable into the embedded terminal.

### Tests for User Story 3 (write FIRST, must fail) ⚠️

- [X] T037 [P] [US3] Contract test for discovery de-duplication (C-D1) and source-unavailable marking (C-D2, C-D3) in `tests/contract/discovery.rs`
- [X] T038 [P] [US3] Integration test: discover across all three source kinds, single entry per session, connect attaches via zellij (AS US3) in `tests/integration/discover.rs`

### Implementation for User Story 3

- [X] T039 [P] [US3] Implement the local discovery source (enumerate local zellij sessions) in `crates/daedalus-discovery/src/local.rs` (FR-010)
- [X] T040 [P] [US3] Implement the mDNS discovery source (`_daedalus._tcp`, TXT identity) in `crates/daedalus-discovery/src/mdns.rs` (FR-010)
- [X] T041 [P] [US3] Implement the tunneled-Workshop discovery source over operator tunnels in `crates/daedalus-discovery/src/tunnel.rs` (FR-010, FR-033)
- [X] T042 [US3] Implement stable session identity + the de-duplication merge in `crates/daedalus-discovery/src/dedupe.rs` (FR-013)
- [X] T043 [US3] Implement unreachable-source handling (host stops advertising / tunnel drops) in `crates/daedalus-discovery/src/lib.rs` (FR-014)
- [X] T044 [P] [US3] Implement the in-Workshop SDK advertisement service in `crates/daedalus-sdk/src/lib.rs` (`advertise` + artifact ref) per contracts/discovery-and-sdk.md (FR-012)
- [X] T045 [US3] Implement the connect/attach flow (`ConnectDiscovered`) in `daedalus-app` + core (FR-011)
- [X] T046 [US3] Build the GPUI Discover screen in `apps/desktop/src/screens/discover.rs` (grouped by source, connect action, empty/scanning/unreachable states) per design-brief §6.5

**Checkpoint**: US1–US3 independently functional.

---

## Phase 6: User Story 4 - Intervene in and control a session's lifecycle (Priority: P4)

**Goal**: stop a running agent, send it input, and clean up its environment.

**Independent Test**: start a session, issue stop and send-input, and clean up — confirm the agent and its
environment respond and other sessions are unaffected.

### Tests for User Story 4 (write FIRST, must fail) ⚠️

- [X] T047 [P] [US4] Contract test for `stop`/`teardown` idempotency + isolation (C-B5) and send-input rejection when unsupported (C-A2) in `tests/contract/control.rs`
- [X] T048 [P] [US4] Integration test: stop halts agent + releases env + records stopped-by-operator; send-input delivered + reflected; cleanup frees resources without affecting others (AS US4) in `tests/integration/control.rs`

### Implementation for User Story 4

- [X] T049 [US4] Implement `Command::StopSession` → `Backend::stop` + teardown/release + status in `crates/daedalus-core/src/session/control.rs` (FR-022)
- [X] T050 [P] [US4] Implement `Command::SendInput` with capability gating to the zellij input sink in `daedalus-zellij`/core (FR-023)
- [X] T051 [P] [US4] Implement `Command::CleanUp` → release environment, free resources, isolation from other sessions (FR-024)
- [X] T052 [US4] Add Stop / Send-input / Confirm-completion / Clean-up controls to the GPUI session-detail header (input disabled w/ reason when unsupported; no pause/resume) per design-brief §6.4

**Checkpoint**: US1–US4 independently functional.

---

## Phase 7: User Story 5 - Manage a fleet across one or more sandbox backends (Priority: P5)

**Goal**: a unified view of many sessions across backends, with concurrency limits and backend availability.

**Independent Test**: start multiple sessions across backends and confirm one unified view shows each
session's identity/backend/status, with unavailable backends clearly marked.

### Tests for User Story 5 (write FIRST, must fail) ⚠️

- [X] T053 [P] [US5] Contract test for backend availability never erroring + unavailable marking (C-B4) in `tests/contract/availability.rs`
- [X] T054 [P] [US5] Integration test: fleet view lists every session w/ tool/objective/backend/env/status; concurrency limit rejection (FR-026); backend-unavailable mid-session marked, others keep working (AS US5) in `tests/integration/fleet.rs`
- [X] T054a [P] [US5] Backend-portability test: run the same session-lifecycle suite (start → monitor → stop/clean-up) **unchanged** against `fake` and one real backend and assert identical operator workflow/outcomes (SC-008) in `tests/integration/portability.rs`

### Implementation for User Story 5

- [X] T055 [US5] Implement the unified fleet query (`AppQuery::fleet`) over all sources/backends in `crates/daedalus-core/src/fleet.rs` (FR-025)
- [X] T056 [P] [US5] Implement the configurable concurrency limit + clear rejection in `crates/daedalus-core/src/session/start.rs` (FR-026)
- [X] T057 [P] [US5] Implement backend availability detection + a backend registry supporting multi-backend selection at start in `crates/daedalus-core/src/backends.rs` (FR-027, FR-028)
- [X] T058 [US5] Build the GPUI Fleet/home screen (session cards, filters, status badges, density for 10+) per design-brief §6.2 in `apps/desktop/src/screens/fleet.rs`
- [X] T059 [P] [US5] Build the GPUI Environments/Backends screen (availability, limits) per design-brief §6.7 in `apps/desktop/src/screens/backends.rs`
- [X] T060 [P] [US5] Build the GPUI Tools registry screen per design-brief §6.6 in `apps/desktop/src/screens/tools.rs`

**Checkpoint**: all five user stories independently functional.

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: cross-cutting guarantees, scope-boundary behaviors, and validation.

- [X] T061 [P] Integration test + implementation for restart reconciliation: re-derive each non-terminal session's status after a Daedalus restart with no history lost (FR-029/030, SC-007) in `tests/integration/reconcile.rs` + `crates/daedalus-core/src/reconcile.rs`
- [X] T062 [P] Implement retain-until-deleted records + `Command::DeleteRecord` (no auto-expiry) with a test in `tests/integration/retention.rs` (FR-030a)
- [X] T063 [P] Test + verify the local-first posture: no open network listener by default; remote reach only over operator tunnels (SC-012, FR-032/033) in `tests/integration/network_posture.rs`
- [X] T064 [P] Build the GPUI Settings screen (tunnels, concurrency limit, notifications, theme, visible local-first posture) per design-brief §6.8 in `apps/desktop/src/screens/settings.rs`
- [X] T065 [P] Accessibility pass: WCAG-AA contrast in both themes, status not color-only, keyboard nav + focus, AccessKit labels (design-brief §11, R-A11Y)
- [ ] T066 Performance validation against success criteria (SC-001 <2min start, SC-003 <5s output, SC-005 <30s stop, SC-006 ≥10 concurrent, SC-009 <10s discover/connect, SC-010 <5s task status)
- [X] T066a [P] Assert genuinely-native packaging (FR-007b): a CI check fails the build if any Electron/CEF/system-webview (e.g. `wry`, `tao`, `webkit2gtk`) dependency appears in the `apps/desktop` dependency tree
- [X] T067 Run the quickstart.md end-to-end on macOS and Linux and fix any drift; confirm `just run`/`test`/`lint` all green
- [X] T068 [P] Verify spec/plan/tasks scope consistency (phased surfaces — desktop first, web subsequent; already reflected in spec FR-007a/SC-011 and plan) and refresh `CLAUDE.md` Recent Changes

---

## Phase 9: User Story 6 - Clear the queue of agents blocked on you (Priority: P6)

**Goal**: the "Needs you" attention queue — waiting-for-input detection, waiting-cost indication, and
one-action jumps into the right affordance (spec US6, FR-015b, FR-021/021a/021b, SC-014/015).

**Independent Test**: with sessions seeded (fake backend) in waiting-for-input, awaiting-confirmation,
stalled, failed, and disconnected states, each appears in the queue ≤5s with cue/duration/cost, and
activating an entry opens the session with the matching affordance focused.

**Prototype counterparts** (validate per Principle IV): `design/prototype/needs.jsx` (view/tray/strip),
`data.js needsYou()` (s-909/s-910 awaiting, s-911 confirm), answer mode in `session.jsx`
(`design/storyboards.md` Flow 2).

### Tests for User Story 6 (write FIRST, must fail) ⚠️

- [ ] T069 [P] [US6] Unit tests for the proto/state-machine extensions per data-model.md — `SessionStatus::{WaitingForInput, Unknown}` (Running⇄WaitingForInput; Unknown derived from liveness; waiting-input is NOT stalled), session `pending_prompt`/`waiting_since`/`work_item_ref`, outcome exit summary, tool `PromptConvention`, backend `idle_rate`/`availability_reason` — in `tests/unit/state_machine.rs` + `tests/unit/proto_attention.rs`
- [ ] T070 [P] [US6] Integration test: waiting-for-input detection per the tool's declared prompt convention — SDK waiting signal (with question text) and prompt-pattern fallback; non-interactive tools never enter; answering returns to Running and clears the prompt (FR-015b) in `tests/integration/waiting_input.rs`
- [ ] T071 [P] [US6] Integration test: `AppQuery::NeedsYou` — kinds + ordering (waiting-input → confirmation → stalled → disconnected → failed), waiting duration, idle-cost/env-held/no-rate cases, ≤5s appearance, empty ⇒ "nothing needs you" (FR-021a/b, SC-014/015) in `tests/integration/needs_you.rs`
- [ ] T072 [P] [US6] Extend `tests/integration/notifications.rs`: blocked-on-operator notifications (waiting-input, awaiting-confirmation) with jump-to-session (FR-021)

### Implementation for User Story 6

- [ ] T073 [US6] Implement the proto + state-machine extensions in `crates/daedalus-proto` and `crates/daedalus-core/src/session/state.rs` (makes T069 green; migration for the new session columns per data-model.md)
- [ ] T074 [US6] Implement waiting-for-input detection in `crates/daedalus-core/src/session/outcome.rs` + SDK waiting-state surfacing in `crates/daedalus-sdk` (capture pending question; clear on `SendInput`) (FR-015b)
- [ ] T075 [US6] Implement the Needs-you query + waiting-cost derivation (idle rate × waiting; env-held for ended-holding-env) in `crates/daedalus-core/src/attention.rs`, exposed as `AppQuery::NeedsYou` + `AppEvent`s via `daedalus-app` (FR-021a/b)
- [ ] T076 [US6] Extend notifications to blocked-on-operator kinds with jump-to-session in core/app + `apps/desktop/src/screens/notifications.rs` (FR-021)
- [ ] T077 [US6] Build the GPUI Needs-you surfaces in `apps/desktop/src/screens/needs.rs` — dedicated view (default), header tray, pinned strip on the tasks board, shell badge counts (purple awaiting+confirm segment) — validate all three placements + the "all caught up" empty state against the prototype
- [ ] T078 [US6] Implement answer-mode deep links: queue/notification activation opens session detail with the pending question scrolled into view + terminal input focused; "Review & confirm" focuses the confirm-completion controls — validate against prototype answer mode (s-909, s-911)

**Checkpoint**: US6 independently demoable on the fake backend; SC-014/015 measurable.

---

## Phase 10: Design-Alignment & New Cross-Cutting Scope (2026-07-06 spec update)

**Purpose**: the remaining realized-design requirements and the Principle-IV re-validation of screens
built before the design was completed.

- [ ] T079 [P] Failing test: aggregate tasks query across all sessions with status/session/tool/backend filters + blocked-on-operator flags (FR-025a) in `tests/integration/tasks_board.rs`
- [ ] T080 Implement the aggregate tasks query in `crates/daedalus-core/src/tasks/aggregate.rs` + build the GPUI aggregate Tasks board as the **landing view** in `apps/desktop/src/screens/tasks.rs` (Fleet becomes one click away) — validate against prototype `tasks.jsx` (board/list/table layouts)
- [ ] T081 [P] Trimmed-output notice in the terminal pane ("showing last N of M lines · full log persisted", FR-016a; responsiveness already covered by C-T3/T027) in `apps/desktop` terminal view — validate against prototype `term-trim` (s-906)
- [ ] T082 Refresh the session telemetry rail against the realized design (FR-019/019a): resource history sparklines, lifecycle/event timeline (incl. `EventKind::OperatorAction`), outcome block for ended sessions in `apps/desktop/src/screens/session.rs` — validate against prototype rail states (s-911 confirm, s-908 unknown/metrics-unavailable)
- [ ] T083 [P] Degraded availability with stated reason end-to-end (FR-028): `availability_reason` through backends/discovery/core, shell hosts indicator (topbar pill + popover), availability-aware source/backend chips — validate against prototype (Backends `host-down` state, hosts indicator)
- [ ] T084 [P] System theme option (follow OS appearance; default) in `apps/desktop/src/theme.rs` + Settings segment (FR-009a) — validate against prototype Settings
- [ ] T085 Re-validate the pre-update screens (T026 start, T035 session, T046 discover, T058 fleet, T059 backends, T060 tools, T064 settings) against the completed prototype — incl. the start-flow failure states, Discover source-dropped/unreachable, Tools empty+validation, Backends no-envs — record deviations in `design/README.md` (constitution Principle IV; plan.md re-check 2026-07-06)
- [ ] T086 [P] Settings: optional per-backend idle-rate configuration persisted and fed to the Needs-you cost derivation (FR-021b) in `apps/desktop/src/screens/settings.rs` + core config
- [ ] T087 [P] Resource-limit policy test: a sandbox exceeding its CPU/memory/disk/time limits is surfaced and the session stopped per the configured policy (spec edge case) in `tests/integration/limits.rs`
- [ ] T088 Verify spec/plan/tasks/data-model consistency after Phases 9–10 and refresh `CLAUDE.md` Recent Changes (supersedes T068's pre-update verification)

---

## Dependencies & Execution Order

### Phase dependencies

- **Setup (Ph 1)**: no deps — start immediately.
- **Foundational (Ph 2)**: depends on Setup — **blocks all user stories**.
- **User Stories (Ph 3–7)**: each depends only on Foundational; independently testable. Recommended priority
  order P1→P5, but stories can be parallelized across developers once Ph 2 is done.
- **Polish (Ph 8)**: depends on the targeted stories being complete.
- **US6 (Ph 9)**: depends on Foundational plus the US2 outcome/monitoring path (T032/T033) and the US4
  send-input path (T050); testable on the fake backend alone.
- **Design-alignment (Ph 10)**: T079/T080 after Foundational; T081/T082 after T035; T083 after T057/T059;
  T084/T086 after T064; T085 after the completed prototype (done) — anytime; T088 last.

### Story dependencies / independence

- **US1 (P1)**: foundation only — the MVP.
- **US2 (P2)**: foundation only; observes sessions US1 can create but is testable via the fake backend alone.
- **US3 (P3)**: foundation + the SDK (T044); independent of US2.
- **US4 (P4)**: foundation; acts on a running session (use fake backend in tests) — independent.
- **US5 (P5)**: foundation; multi-backend/fleet view — independent.

### Within each story

Tests (write first, must fail) → models/types → core services → backend/zellij/discovery → GPUI screen.

**Cross-story UI dependency**: every GPUI screen task (T026, T035, T046, T058, T059, T060, T064, T077,
T080, T081, T082) depends on the design-system port **T017** (theme tokens + core components); screens MUST
NOT be built before T017 lands. T017 itself needs a small extension for the realized design (confirm-status
glyph/token, needs-tag, failure-notice pattern) — fold into T077's first step.

### Parallel opportunities

- Setup: T003, T004, T005 in parallel.
- Foundational: T006, T007, T008, T010, T012, T015 are `[P]` (different files) before their implementations.
- Per story, all `[P]` test tasks run together, then `[P]` implementation tasks in different files (e.g. the
  two real backends T024/T025; the discovery sources T039/T040/T041).
- Whole stories can run in parallel across developers after Phase 2.

---

## Parallel Example: User Story 1

```bash
# Write the failing tests together first:
Task: "Contract test RegisterTool/StartSession in tests/contract/app_start.rs"   # T018
Task: "Integration test start fresh/pre-existing/failure in tests/integration/start_session.rs"  # T019
Task: "Adversarial isolation test in tests/integration/isolation.rs"             # T020

# Then the parallelizable implementations (different crates/files):
Task: "Workshop backend acquire/start_agent in crates/daedalus-backend-workshop/src/lib.rs"  # T024
Task: "macOS sandbox backend acquire/start_agent in crates/daedalus-backend-macos/src/lib.rs" # T025
```

---

## Implementation Strategy

### MVP first (US1 only)

1. Phase 1 Setup → 2. Phase 2 Foundational (critical, blocks everything) → 3. Phase 3 US1 →
**STOP & validate** US1 on the fake backend → demo.

### Incremental delivery

Setup + Foundational → US1 (MVP) → US2 → US3 → US4 → US5 → Polish. Each story is an independently testable,
demoable increment that doesn't break earlier ones.

---

## Notes

- `[P]` = different files, no incomplete dependency. `[Story]` maps a task to its user story for traceability.
- Verify every test fails for the expected reason before implementing (Principle I).
- Lint clean (`-D warnings`) and locally runnable (fake backend) at every step (Principles II, III).
- Commit after each task or logical group.
- Out of scope (do not build): pause/resume, multi-user/auth, auto-expiry of history, the web surface (deferred).
