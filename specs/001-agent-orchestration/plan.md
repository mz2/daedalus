# Implementation Plan: Orchestrate and Monitor Agentic Tools in Sandbox Environments

**Branch**: `001-agent-orchestration` | **Date**: 2026-06-19 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/001-agent-orchestration/spec.md`

## Summary

Daedalus is a single-operator, local-first application for orchestrating and monitoring autonomous
agentic tools that run inside isolated sandbox environments. The operator starts **sessions** (an agentic
tool working an SDD-defined objective inside an environment), watches them through an embedded terminal
and a per-session task-status board, discovers sessions across local/mDNS/tunneled-Workshop sources,
attaches via zellij, and controls lifecycle (stop / send-input / clean-up). v1 ships two backends —
Canonical Workshop (Linux) and a macOS-specific local sandbox — behind one common abstraction.

**Technical approach**: a single shared **Rust core** owns all orchestration (sessions + state machine,
backend abstraction, discovery, persistence, reconciliation). The presentation surface is a genuinely
native desktop GUI built with **GPUI** ([gpui.rs](https://www.gpui.rs/)) and the
**[longbridge/gpui-component](https://github.com/longbridge/gpui-component)** widget library (macOS +
Linux, GPU-rendered, no Electron/webview) that renders core state and issues core commands in-process —
no FFI boundary. zellij provides session multiplexing/attachment; the desktop embedded terminal is a native
GPUI view (via `alacritty_terminal`) attached to zellij. The **web surface is deferred** for this
implementation (operator decision, 2026-06-19); the core stays surface-agnostic so a web UI can be added
later. See [research.md](./research.md) §R1 for the decision and the recorded spec deviation.

The **visual design is locked** to the prototype design system in [`design/`](../../design/) (canonical
export `design/Daedalus-Prototype-standalone.html`, mapped to GPUI in `design/README.md`): two platform-native
skins (Yaru/Ubuntu → Linux, Cupertino → macOS), dark+light themes, adjustable density, and a color-blind-safe
status palette (paired with icon+label) that maps 1:1 to `SessionStatus`/`TaskStatus`. The GPUI surface ports
these tokens and components rather than inventing new ones.

## Technical Context

**Language/Version**: Rust 1.83+ (2021 edition) end-to-end — core, backends, discovery, SDK, shared
contracts, **and the UI** (GPUI is Rust; no second language, no FFI).  
**Design system**: ported from [`design/`](../../design/) — tokens (color/status palette, typography
Ubuntu+JetBrains Mono, radii, spacing, motion, shadows), per-skin (Yaru/mac) × per-theme (dark/light)
values, and the component/screen inventory in `design/README.md`. The GPUI theme module implements these.  
**Primary Dependencies**: `gpui` + `gpui-component` (native desktop GUI); `alacritty_terminal` (embedded
terminal engine for the GPUI terminal view); `tokio` (async runtime); zellij (session multiplexing/attach);
`mdns-sd` or equivalent (mDNS discovery); an authenticated-tunnel mechanism (operator-established, e.g.
SSH/WireGuard-style); `serde` (+ `serde_json`) for the shared contract types; `sqlx`/`rusqlite` (SQLite
persistence); a macOS sandbox primitive (Seatbelt `sandbox-exec` / App Sandbox container); the Canonical
Workshop control interface for the Workshop backend.  
**Storage**: SQLite (single local file) for session metadata, status, tracked-task history, and a
lifecycle/event log; captured terminal output persisted to per-session capture files referenced from the
DB. Retain-until-deleted (no auto-expiry in v1).  
**Testing**: `cargo test` / `cargo nextest`; `clippy -D warnings` + `rustfmt`; contract tests for the
backend trait, the in-Workshop SDK protocol, discovery, and persistence; integration tests for isolation
(incl. adversarial agents), lifecycle state transitions, discovery de-duplication, and restart
reconciliation. UI-toolkit-specific test runner per the research decision.  
**Target Platform**: macOS (Apple silicon + Intel) and Linux desktop for the native GPUI GUI (Windows not a
v1 target; web surface deferred); Linux for Workshop-hosted agents; macOS for the local-sandbox backend.  
**Project Type**: Native desktop application over a shared Rust core (Cargo workspace); surface-agnostic
core leaves a web surface as a future addition.  
**Performance Goals**: agent output visible in the embedded terminal within 5s of production (SC-003);
selected→running in under 2 min (SC-001); stop+reclaim within 30s (SC-005); ≥10 concurrent sessions in one
view without status/output loss (SC-006); tracked-task status on the board within 5s (SC-010);
discover→connect within 10s (SC-009); UI targets 60fps on desktop-class hardware.  
**Constraints**: no open network listener by default — loopback-only control plane (SC-012, FR-032);
remote reach only via operator-established authenticated tunnels (FR-033); genuinely native desktop GUI,
no Electron/webview shell (FR-007b); 100% sandbox isolation incl. adversarial agents (SC-002); secrets
never managed, injected, or persisted (FR-031); WCAG AA + color-blind-safe status in both themes.  
**Scale/Scope**: single operator; ≥10 concurrent sessions; 2 backends in v1 (extensible); ~8 primary
screens (per the design brief), desktop-only for now; open set of operator-registered agentic tools.

**UI decision (resolved in research.md §R1)**: native desktop GUI with **GPUI + gpui-component** (Rust,
in-process with the core, no FFI). The **web surface is deferred** by operator decision — a recorded
deviation from FR-007a/SC-011, tracked in Complexity Tracking below.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Constitution v1.0.0 — three principles plus Quality Gates.

| Principle | How this plan satisfies it | Status |
|-----------|----------------------------|--------|
| **I. Red/Green TDD (NON-NEGOTIABLE)** | Every contract (backend trait, SDK advertisement protocol, discovery, persistence) and every behavior (session state machine, dedup, reconciliation, isolation) gets a failing test before implementation. tasks.md will order test tasks ahead of their implementation tasks. | ✅ PASS |
| **II. Strict Linting — Warnings Are Errors** | Rust: `cargo clippy -D warnings` + `rustfmt --check` in CI; no blanket `#[allow]`. UI: linter chosen with the toolkit (`dart analyze`/`flutter analyze` for Flutter, or `clippy -D warnings` for Slint/egui). Web assets (if any) linted likewise. | ✅ PASS |
| **III. Locally Testable Runtime Environment** | A documented single-command build/run path (quickstart.md). A `daedalus-backend-fake` in-memory backend lets sessions, discovery, persistence, and lifecycle be exercised locally **without** Workshop or remote access; the macOS sandbox backend is locally testable on macOS. Workshop integration is verified via the fake backend + contract tests where Workshop is unavailable. | ✅ PASS |

**Quality Gates** (tests green, lint clean, locally exercised, no silent scope-narrowing) are encoded into
the task plan and review checklist. **No violations** — Complexity Tracking is empty.

Re-check after Phase 1 design: see end of [research.md](./research.md) / data-model — no new violations
introduced (the workspace decomposition and the fake backend serve Principles III and pluggability, not
gold-plating).

## Project Structure

### Documentation (this feature)

```text
specs/001-agent-orchestration/
├── plan.md              # This file
├── research.md          # Phase 0 output — frontend evaluation + all unknowns resolved
├── data-model.md        # Phase 1 output — entities, state machine, persistence schema
├── quickstart.md        # Phase 1 output — local build/run/test path
├── contracts/           # Phase 1 output — backend, SDK, discovery, app-API contracts
├── design-brief.md      # Pre-existing UI design brief
└── checklists/
    └── requirements.md  # Spec quality checklist (passed)
```

### Source Code (repository root)

```text
Cargo.toml                          # Cargo workspace manifest

crates/
├── daedalus-core/                  # Sessions, lifecycle state machine, orchestration,
│                                   #   discovery coordination, persistence, reconciliation
├── daedalus-proto/                 # Shared serde contract types (sessions, tasks, events,
│                                   #   status) consumed by core + every surface
├── daedalus-backend/               # Backend trait + common environment/provisioning types
├── daedalus-backend-workshop/      # Canonical Workshop (Linux) backend
├── daedalus-backend-macos/         # macOS local sandbox backend (Seatbelt/App Sandbox)
├── daedalus-backend-fake/          # In-memory backend for local testing (Principle III)
├── daedalus-discovery/             # mDNS, tunnel registry, multi-source de-duplication
├── daedalus-zellij/                # zellij multiplexing/attach + native terminal-view glue
├── daedalus-sdk/                   # In-Workshop SDK advertising a connectable service
└── daedalus-app/                   # App service: surface-agnostic command/query +
                                    #   event-stream API over the core, wiring + config

apps/
└── desktop/                        # Native GPUI GUI (gpui + gpui-component) — thin surface
    └── src/                        #   (apps/web/ deferred — see Complexity Tracking)
        ├── theme.rs                #   design tokens/skins/themes ported from design/
        ├── components/             #   status badge, cards, nav, titlebar, terminal pane, …
        └── screens/                #   tasks(home), fleet, session, start, discover, tools, backends, settings

design/                             # Visual source of truth (prototype + design→GPUI mapping)
├── Daedalus-Prototype-standalone.html  # canonical latest export
├── README.md                       # design system → GPUI mapping (tokens, components, screens)
└── prototype/                      # unpacked readable source (css/jsx/assets/screenshots)

tests/
├── contract/                       # Backend trait, SDK protocol, discovery, persistence
└── integration/                    # Isolation (+adversarial), lifecycle, dedup, reconciliation
```

**Structure Decision**: A Rust **Cargo workspace** with a shared `daedalus-core` plus separable
per-backend crates and a thin desktop surface. This maps directly onto the constitution and spec: the
"shared core, thin surfaces" intent (FR-007a) → `daedalus-core`/`daedalus-proto` + thin `apps/desktop`;
backend pluggability (FR-027, FR-001a) → `daedalus-backend*` crates behind one trait; local testability
(Principle III) → `daedalus-backend-fake`. `apps/desktop` depends only on `daedalus-app`/`daedalus-proto`,
never on backend internals. Keeping the surface boundary at `daedalus-app` means the deferred `apps/web`
can be added later as an additive surface without touching orchestration.

## Complexity Tracking

> No **constitution** (v1.0.0) violations — TDD, strict linting, and local-testability are all satisfiable
> as planned. The multi-crate split and the fake backend are required by backend pluggability (FR-027) and
> Principle III (local testability), not optional complexity.
>
> One recorded **spec** deviation (not a constitution violation):

| Deviation | Why | Simpler/compliant alternative deferred because |
|-----------|-----|------------------------------------------------|
| Web surface omitted from this implementation (FR-007a, SC-011 require web + parity) | Operator decision (2026-06-19) to ship a single native GPUI desktop surface first and avoid an FFI/second-toolchain UI; faster path to the core value (orchestrate + monitor) | Building web now doubles surface work before the core is proven; the `daedalus-app` boundary keeps the core surface-agnostic so web is added later without rework. `spec.md`/tasks to be updated to reflect the phased scope. |
