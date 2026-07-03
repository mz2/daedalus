# Quickstart: Daedalus (local build / run / test)

**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

This is the documented, reproducible local path required by Constitution Principle III. It uses the
**`fake` backend** so the whole app — sessions, discovery, persistence, lifecycle, task board — runs on a
developer's (or agent's) machine **without** Canonical Workshop or any remote access.

## Prerequisites

- **Rust** 1.83+ (`rustup`), with `clippy` and `rustfmt` components.
- **zellij** on `PATH` (session multiplexing / terminal attach).
- Desktop GPU/runtime for **GPUI**:
  - macOS: full Xcode + the Metal Toolchain component (`xcodebuild -downloadComponent MetalToolchain`).
  - Linux: Vulkan-capable drivers (GPUI's Blade backend) + standard build tooling.
- macOS only, for the `macos` backend: `sandbox-exec` (system-provided).

> Pin the `gpui` / `gpui-component` revisions in `Cargo.toml` (sourced from their git repos); validate the
> desktop build on both macOS and Linux early (research risk R-UI).

## Build

```bash
cargo build --workspace
```

## Run (fake backend — no Workshop needed)

```bash
# Default build is headless: wires the app to the fake backend, reconciles, and prints the
# backend availability + fleet summary. Fast, no GPU required (validates wiring + quickstart).
DAEDALUS_BACKEND=fake cargo run -p daedalus-desktop

# The native GPUI window is behind the `gpui` feature (it needs a GPU/display — research
# R-UI). On Nix, source the helper first to expose the system libs gpui links against:
source scripts/gpui-env.sh    # Nix only; otherwise install the -dev packages
DAEDALUS_BACKEND=fake cargo run -p daedalus-desktop --features gpui
```

In the GPUI UI: register a sample agentic tool → start a session (fresh environment) → watch the
embedded terminal stream and the task-status board update → stop / confirm / clean up. This exercises
US1–US2 and the lifecycle state machine end-to-end locally. The same flows are covered headlessly by
`cargo test --workspace` against the `fake` backend.

## Test

```bash
cargo test --workspace          # unit + contract + integration (or: cargo nextest run)
```

Key suites (authored test-first per Principle VI):
- **contract/** — backend trait, app API, discovery + SDK, terminal attach (see `contracts/`).
- **integration/** — isolation (incl. an adversarial agent against `fake`), session lifecycle transitions,
  discovery de-duplication, restart reconciliation.

## Lint (must be clean — Constitution Principle II)

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

## Backends beyond `fake`

- **macOS sandbox**: `DAEDALUS_BACKEND=macos cargo run -p daedalus-desktop` (macOS host; validates SC-002
  isolation locally).
- **Workshop (Linux)**: requires a reachable Workshop control interface; on macOS, reach a remote Linux
  Workshop over an operator-established authenticated tunnel (no open listener by default).

## What is NOT in this implementation yet

- **Web surface** — deferred (operator decision); the core is surface-agnostic so it is additive later.
  See `plan.md` Complexity Tracking and `research.md` §R1.
