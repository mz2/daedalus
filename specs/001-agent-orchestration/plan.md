# Implementation Plan: Orchestrate and Monitor Agentic Tools in Sandbox Environments

**Branch**: `001-agent-orchestration` | **Date**: 2026-06-19 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/001-agent-orchestration/spec.md`
**Updated**: 2026-07-06 — spec incorporated the completed design prototype (US6 "Needs you",
FR-015b/016a/019a/021a/021b/025a, system theme, degraded availability); constitution check re-run
against v1.1.0 (Principle IV — design fidelity); design references repointed to the canonical
`design/prototype/`. New work is tasked in tasks.md Phase 9 (US6) and Phase 10.
**Release gates** (spec § "Delivery Phasing & Release Gates"): before any release claims spec
compliance — real Workshop integration (#11), SC-002 demonstrated against real backends (#11/#9/#12/#13),
live AccessKit/keyboard wiring (#2), renderer depth (#3), and perf validation (#7). Everything else in
Phases 1–10 is delivered and green.

## Summary

Daedalus is a single-operator, local-first application for orchestrating and monitoring autonomous
agentic tools that run inside isolated sandbox environments. The operator starts **sessions** (an agentic
tool working an SDD-defined objective inside an environment), watches them through an embedded terminal
and a per-session task-status board, discovers sessions across local/mDNS/tunneled-Workshop sources,
attaches via zellij, and controls lifecycle (stop / send-input / clean-up). v1 ships the Canonical
Workshop (Linux) backend behind one common abstraction (plus `fake` for local testing); Daedalus never
implements its own isolation — further backends integrate existing agent-oriented sandbox runtimes
(amended 2026-07-06; the bespoke macOS Seatbelt backend was removed, NVIDIA OpenShell is the planned
integrated runtime incl. macOS/Apple silicon — repo issue #9).

**Technical approach**: a single shared **Rust core** owns all orchestration (sessions + state machine,
backend abstraction, discovery, persistence, reconciliation). The presentation surface is a genuinely
native desktop GUI built with **GPUI** ([gpui.rs](https://www.gpui.rs/)) and the
**[longbridge/gpui-component](https://github.com/longbridge/gpui-component)** widget library (macOS +
Linux, GPU-rendered, no Electron/webview) that renders core state and issues core commands in-process —
no FFI boundary. zellij provides session multiplexing/attachment; the desktop embedded terminal is a native
GPUI view (via `alacritty_terminal`) attached to zellij. The **web surface is deferred** for this
implementation (operator decision, 2026-06-19); the core stays surface-agnostic so a web UI can be added
later. See [research.md](./research.md) §R1 for the decision and the recorded spec deviation.

The **visual design is locked** to the prototype design system in [`design/`](../../design/) — canonical
source `design/prototype/` (completed 2026-07-03; runnable HTML/JSX), mapped to GPUI in `design/README.md`
with flow storyboards in `design/storyboards.md`; the older single-file export
`Daedalus-Prototype-standalone.html` is superseded. Two platform-native skins (Yaru/Ubuntu → Linux,
Cupertino → macOS), dark/light/system themes, adjustable density, and a color-blind-safe status palette
(paired with icon+label) that maps 1:1 to `SessionStatus`/`TaskStatus`, plus the realized Needs-you queue,
telemetry rail, and cross-cutting failure states. The GPUI surface ports these tokens and components rather
than inventing new ones, and per constitution Principle IV every screen/state implementation is validated
against the prototype rendering.

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
persistence); the Canonical
Workshop control interface for the Workshop backend (further backends integrate existing agent-oriented
sandbox runtimes, e.g. the NVIDIA OpenShell CLI — never Daedalus-maintained isolation primitives).  
**Storage**: SQLite (single local file) for session metadata, status, tracked-task history, and a
lifecycle/event log; captured terminal output persisted to per-session capture files referenced from the
DB. Retain-until-deleted (no auto-expiry in v1).  
**Testing**: `cargo test` / `cargo nextest`; `clippy -D warnings` + `rustfmt`; contract tests for the
backend trait, the in-Workshop SDK protocol, discovery, and persistence; integration tests for isolation
(incl. adversarial agents), lifecycle state transitions, discovery de-duplication, and restart
reconciliation. UI-toolkit-specific test runner per the research decision.  
**Target Platform**: macOS (Apple silicon + Intel) and Linux desktop for the native GPUI GUI (Windows not a
v1 target; web surface deferred); Linux for Workshop-hosted agents; macOS reaches remote Workshops over tunnels (local isolation on macOS
arrives with the integrated OpenShell backend, issue #9).  
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
**Scale/Scope**: single operator; ≥10 concurrent sessions; Workshop + fake backends in v1 (extensible
via integrated agent-sandbox runtimes); ~8 primary
screens (per the design brief), desktop-only for now; open set of operator-registered agentic tools.

**UI decision (resolved in research.md §R1)**: native desktop GUI with **GPUI + gpui-component** (Rust,
in-process with the core, no FFI). The **web surface is deferred** by operator decision — a recorded
deviation from FR-007a/SC-011, tracked in Complexity Tracking below.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Constitution v1.1.0 — four principles plus Quality Gates.

| Principle | How this plan satisfies it | Status |
|-----------|----------------------------|--------|
| **I. Red/Green TDD (NON-NEGOTIABLE)** | Every contract (backend trait, SDK advertisement protocol, discovery, persistence) and every behavior (session state machine, dedup, reconciliation, isolation) gets a failing test before implementation. tasks.md will order test tasks ahead of their implementation tasks. | ✅ PASS |
| **II. Strict Linting — Warnings Are Errors** | Rust: `cargo clippy -D warnings` + `rustfmt --check` in CI; no blanket `#[allow]`. UI: linter chosen with the toolkit (`dart analyze`/`flutter analyze` for Flutter, or `clippy -D warnings` for Slint/egui). Web assets (if any) linted likewise. | ✅ PASS |
| **III. Locally Testable Runtime Environment** | A documented single-command build/run path (quickstart.md). A `daedalus-backend-fake` in-memory backend lets sessions, discovery, persistence, and lifecycle be exercised locally **without** Workshop or remote access; real-backend testing runs against Workshop where reachable (the bespoke macOS backend was removed 2026-07-06). Workshop integration is verified via the fake backend + contract tests where Workshop is unavailable. | ✅ PASS |
| **IV. Design Fidelity to the Prototype** (added in v1.1.0) | The canonical runnable prototype is `design/prototype/` (screens, cross-cutting states, both themes × skins), catalogued in `design/README.md` and reproducible via `design/storyboards.md`. Every GPUI screen/component task names its prototype counterpart and includes a validate-against-prototype step; deviations are recorded in `design/README.md`, and UI with no prototype counterpart gets a prototype first (as done for the Needs-you queue before US6 was tasked). | ✅ PASS |

**Quality Gates** (tests green, lint clean, locally exercised, no silent scope-narrowing) are encoded into
the task plan and review checklist. **No violations** — Complexity Tracking is empty.

Re-check after Phase 1 design: see end of [research.md](./research.md) / data-model — no new violations
introduced (the workspace decomposition and the fake backend serve Principles III and pluggability, not
gold-plating).

Re-check 2026-07-06 (constitution v1.1.0 + spec update): Principle IV added above — PASS as planned;
retroactively, already-built screens (T026/T035/T046/T058–T060/T064) MUST be re-validated against the
completed prototype as part of the Phase 9/10 work since the realized design postdates them (tracked as
T085).

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
├── daedalus-backend-fake/          # In-memory backend for local testing (Principle III)
│                                   #   (further backends integrate existing agent sandboxes, issue #9)
├── daedalus-discovery/             # mDNS, tunnel registry, multi-source de-duplication
├── daedalus-zellij/                # zellij multiplexing/attach + native terminal-view glue
├── daedalus-sdk/                   # In-Workshop SDK advertising a connectable service
└── daedalus-app/                   # App service: surface-agnostic command/query +
                                    #   event-stream API over the core, wiring + config

apps/
└── desktop/                        # Native GPUI GUI (gpui + gpui-component) — package `daedalus-desktop`,
    └── src/                        #   thin surface (apps/web/ deferred — see Complexity Tracking)
        ├── theme.rs                #   design tokens/skins/themes ported from design/
        ├── components/             #   status badge, cards, nav, titlebar, terminal pane, …
        └── screens/                #   tasks(home), fleet, session, start, discover, tools, backends, settings

design/                             # Visual source of truth (prototype + design→GPUI mapping)
├── prototype/                      # CANONICAL runnable source (css/jsx/assets/screenshots, 2026-07-03)
├── README.md                       # design system → GPUI mapping (tokens, components, screens, states)
├── storyboards.md                  # flow storyboards + cross-cutting state matrix
└── Daedalus-Prototype-standalone.html  # older single-file export (superseded)

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
> One **phased-scope** decision (now reflected in the spec — not an open deviation, not a constitution violation):

| Scope decision | Why | How it stays compliant |
|----------------|-----|------------------------|
| Deliver the native GPUI desktop surface first; web UI is a planned subsequent surface | Operator decision (2026-06-19): ship the core value (orchestrate + monitor) on one native surface first and avoid an FFI/second-toolchain UI before the core is proven | spec.md FR-007a/SC-011 now phase the surfaces explicitly; the `daedalus-app` boundary keeps the core surface-agnostic so the web UI is additive with no rework, preserving capability parity across delivered surfaces. |
