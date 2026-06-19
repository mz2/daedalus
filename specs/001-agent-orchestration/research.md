# Phase 0 Research: Agent Orchestration

**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Date**: 2026-06-19

This document resolves the open technical decisions for the plan. The headline decision — the **UI
toolkit** — is recorded first; the operator selected **GPUI** and chose to **defer the web surface**, which
also dissolves the Rust↔Flutter FFI concern that drove the earlier evaluation.

---

## R1. UI toolkit (DECIDED — GPUI, web deferred)

**Decision**: **GPUI** ([gpui.rs](https://www.gpui.rs/), Zed's GPU-accelerated Rust UI framework) for a
**native desktop GUI**, using the **[longbridge/gpui-component](https://github.com/longbridge/gpui-component)**
component library. The **web surface is deferred** for v1 of this implementation — Daedalus ships a single
native desktop surface for now, and the core is kept surface-agnostic so a web UI can be added later without
reworking orchestration.

**Operator decision (2026-06-19)**: build with GPUI + gpui-component; forget the web frontend for now.

**Visual design is locked** to the prototype design system in [`design/`](../../design/) (canonical export
`design/Daedalus-Prototype-standalone.html`; design→GPUI mapping in `design/README.md`): two platform-native
skins (Yaru/Ubuntu, Cupertino), dark+light themes, density steps, and a color-blind-safe status palette
(incl. an awaiting-confirmation hue matching FR-015a). The GPUI surface ports these tokens/components; see
plan.md and spec.md FR-009a.

### Why GPUI fits

- **No FFI boundary** — GPUI is Rust, running in-process with `daedalus-core`; the UI shares
  `daedalus-proto` types verbatim with a single `cargo` build and single-language stack traces. This was
  the operator's primary concern with Flutter, and it disappears entirely.
- **Genuinely native, no webview** — GPUI renders on the GPU (Metal on macOS; Blade/Vulkan on Linux),
  satisfying FR-007b. It is the engine behind the Zed editor, so it is proven on dense, high-performance,
  text-heavy UIs — a close match for an operator console.
- **Polished components** — `gpui-component` (by Longbridge) supplies a broad, good-looking widget set
  (buttons, inputs, tables, lists, dropdowns, docking/panels, theming) that maps onto the design brief's
  needs: status badges, the tracked-task board, telemetry widgets, dense layouts, and dark/light theming.
- **Embedded terminal is natural** — GPUI's ecosystem already drives `alacritty_terminal` (Zed's terminal),
  so the desktop embedded terminal is a real native GPUI view attaching to zellij, not a canvas/DOM
  workaround.

### Consequences and the web deferral

- **Spec deviation (recorded, not silent)**: FR-007a and SC-011 require a web UI with capability parity
  across surfaces. Deferring web means v1 of this implementation does **not** meet that surface requirement
  yet. This is an explicit operator scope decision, tracked in `plan.md` Complexity Tracking and to be
  reflected when `spec.md`/tasks are updated. The shared-core principle is preserved: all orchestration
  stays in `daedalus-core`/`daedalus-app` behind `daedalus-proto`, so adding a web surface later is additive.
- **Platform scope**: native desktop on macOS and Linux (GPUI's supported desktop targets). Windows is not
  a v1 target.

### Alternatives considered and rejected

- **Flutter (Dart + Rust core)** — richest widgets and web maturity, but the only option with the
  Rust↔Flutter **FFI workflow** the operator flagged (codegen step, dual toolchain, cross-boundary
  debugging, two-WASM web). Rejected on that friction.
- **Slint / egui (Rust, native + WASM web)** — both eliminate FFI and were the prior front-runners while a
  shared native+web codebase was required. With web deferred, the shared-web advantage no longer decides the
  choice, and GPUI's proven performance on dense text UIs plus the ready gpui-component library and
  terminal story win. Kept on record as the fallback if GPUI's smaller community or build setup proves
  problematic.
- **Dioxus** — desktop renders through the system **webview** (wry); excluded by FR-007b.

### Risk — GPUI maturity & packaging

GPUI is developed primarily inside the Zed repository and is younger as a standalone, externally-consumed
dependency than Flutter/Slint; expect to pin a specific `gpui`/`gpui-component` revision and validate the
build on both macOS and Linux early. Accessibility coverage (WCAG AA, AccessKit) must be verified against
the design brief. Tracked as R-UI / R-A11Y below.

---

## R2. Session multiplexing & attachment — zellij

**Decision**: Treat zellij as the single multiplexer. Daedalus starts each agent inside a named zellij
session; the desktop surface attaches via a native terminal view, the web surface via zellij's web service
(or an `xterm.js` client against it). Daedalus drives zellij through its CLI/control interface and discovers
local zellij sessions by listing them.

**Rationale**: FR-008/FR-011 mandate zellij; reusing it (Principle VII) avoids reimplementing a PTY
multiplexer and gives attach/detach, scrollback, and a ready web terminal. Keeps scope small.

**Alternatives considered**: a bespoke PTY layer (tmux, raw `portable-pty`) — rejected: duplicates zellij,
contradicts the spec and the reuse principle. Note as risk R-Z: zellij's web service maturity/stability must
be validated early since it underpins the web terminal.

## R3. Local-network discovery — mDNS

**Decision**: Advertise/browse over mDNS-SD using a pure-Rust library (`mdns-sd`), a Daedalus service type
(e.g. `_daedalus._tcp`) carrying host + session-source metadata in TXT records. Discovery results feed a
de-duplication layer keyed by a stable session identity (R7).

**Rationale**: FR-010 requires mDNS host discovery on the LAN; a pure-Rust crate avoids a system-daemon
dependency and keeps local testability (Principle III).

**Alternatives**: binding to the OS responder (Avahi/Bonjour) — rejected: heavier platform coupling and
worse local testability. Validate multi-source dedup against the same session advertised via local + mDNS.

## R4. Remote reach — authenticated tunnels

**Decision**: Daedalus never opens a network listener by default (FR-032, SC-012). Remote hosts and remote
Workshops are reached only over **operator-established authenticated tunnels** (e.g. SSH or WireGuard-style),
modelled as a tunnel registry the operator configures; discovery and attachment ride existing tunnels rather
than Daedalus exposing a service.

**Rationale**: Principle III (local-first) and FR-033. Keeps attack surface minimal; the operator owns the
trust boundary.

**Alternatives**: a built-in listening control service with auth — rejected: violates the no-open-listener
posture and SC-012. A managed/relay service — out of v1 scope (single-operator).

## R5. Backend abstraction + the two v1 backends

**Decision**: One `Backend` trait (`provision`, `attach`, `start_agent`, `resource_usage`, `stop`,
`teardown`, `availability`) with environment/session value types in `daedalus-backend`. v1 implements:
- **Workshop (Linux)** — drives Canonical Workshop's control interface to provision/attach environments and
  launch the agent inside, under zellij.
- **macOS local sandbox** — confines an agent on macOS using Seatbelt (`sandbox-exec`) / App Sandbox
  container primitives, under zellij.
- **Fake (in-memory)** — required by Principle III for local testing without Workshop/remote access.

**Rationale**: FR-027 requires ≥2 backends behind a common abstraction without changing the operator
workflow; the fake backend makes the whole core locally exercisable.

**Alternatives**: hard-coding Workshop only — rejected (fails FR-027, SC-013, and local testability).
Risk R-WS: the exact Workshop control surface must be confirmed against Workshop docs during contract work;
Risk R-MAC: Seatbelt is deprecated-but-functional on macOS — validate it meets SC-002 isolation, with App
Sandbox containers as the fallback primitive.

## R6. Persistence & reconciliation

**Decision**: SQLite (single local file) holds session metadata, status, tracked-task history, and a
lifecycle/event log; large/streaming terminal output goes to per-session capture files referenced from the
DB. On startup, a reconciliation pass re-derives each persisted session's current status from its backend +
zellij liveness (FR-029, FR-030). Records are retained until the operator deletes them (FR-030a).

**Rationale**: SQLite is embedded, transactional, local-first, and trivially testable; file-backed output
avoids bloating the DB with high-volume terminal streams (edge case: excessive output).

**Alternatives**: a server DB (Postgres) — rejected (not local-first, heavier). Everything-in-SQLite incl.
output blobs — rejected (poor fit for large append-only streams).

## R7. In-Workshop SDK & session identity / de-duplication

**Decision**: A `daedalus-sdk` runs inside a Workshop environment and advertises a connectable service plus
the workspace location of the session's SDD artifacts. Every session carries a **stable identity** (e.g.
host-id + zellij-session-id, or an SDK-issued UUID) so the same session seen via multiple sources collapses
to one entry (FR-013).

**Rationale**: FR-012 requires an in-Workshop advertisement; a stable identity is the precondition for
dedup (FR-013) and for the task board to bind to the right artifacts.

**Alternatives**: identifying sessions by display name — rejected (collisions, unstable across sources).

## R8. Task-status derivation from SDD artifacts

**Decision**: The task-status board derives each tracked task's state by reading the session's SDD artifacts
(e.g. SpecKit `tasks.md` checkbox state) from the workspace, surfaced via the in-Workshop SDK, and refreshes
on change within the SC-010 budget (≤5s). Daedalus parses checkbox/markers rather than inferring from output.

**Rationale**: FR-017 and Principle IV mandate a single authoritative source for task state.

**Alternatives**: inferring progress from terminal output/heuristics — rejected (unreliable, violates the
single-source rule).

## R9. Secret-safe output capture

**Decision**: Captured terminal output and persisted logs pass through a redaction step; Daedalus never
prompts for, stores, or displays secrets (FR-031). The operator pre-provisions secrets in the
environment/backend.

**Rationale**: Principle III (local-first) secret boundary and FR-031. Redaction-on-capture keeps secrets
out of the persisted record by construction.

**Alternatives**: capturing raw output verbatim — rejected (risks persisting secrets). Designing secret-entry
UI — explicitly out of scope (design brief §12).

---

## Resolved unknowns summary

| # | Unknown | Decision |
|---|---------|----------|
| R1 | UI toolkit | **GPUI + gpui-component** (Rust, native desktop, no FFI); **web surface deferred** |
| R2 | Multiplexer | zellij (native GPUI terminal view attaching to zellij; web service deferred with web UI) |
| R3 | LAN discovery | mDNS-SD via `mdns-sd`, `_daedalus._tcp` |
| R4 | Remote reach | operator-established authenticated tunnels; no open listener |
| R5 | Backends | `Backend` trait + Workshop + macOS sandbox + fake |
| R6 | Persistence | SQLite + file-backed output; startup reconciliation |
| R7 | Identity/SDK | in-Workshop SDK; stable session identity for dedup |
| R8 | Task status | parse SDD artifacts (SpecKit `tasks.md`) via SDK |
| R9 | Secrets | redaction-on-capture; never managed/displayed |

## Open risks to validate in implementation

- **R-UI** — GPUI/gpui-component are younger as external dependencies; pin a revision and validate the build
  on macOS + Linux early.
- **R-WEB** — web surface deferred (FR-007a/SC-011 deviation); keep the core surface-agnostic so web is
  additive later.
- **R-Z** — zellij integration underpins attach/multiplexing; validate the native terminal-view attach path
  (alacritty_terminal + zellij) early.
- **R-WS** — exact Canonical Workshop control surface to be confirmed during contract work.
- **R-MAC** — macOS Seatbelt isolation must be proven to meet SC-002 (App Sandbox containers as fallback).
- **R-A11Y** — validate WCAG-AA + AccessKit coverage in GPUI against the design brief.
