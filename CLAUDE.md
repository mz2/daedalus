# daedalus Development Guidelines

Auto-generated from all feature plans. Last updated: 2026-07-06

## Active Technologies

- **Language**: Rust 1.83+ (2021 edition) end-to-end — core, backends, discovery, SDK, shared contracts, and UI.
- **UI**: GPUI (gpui.rs) + `gpui-component` — native desktop GUI (macOS + Linux), GPU-rendered, no
  Electron/webview, in-process with the core (no FFI). Embedded terminal via `alacritty_terminal` attached
  to zellij. (Web surface deferred — see `specs/001-agent-orchestration/research.md` §R1.)
- **Async/runtime**: `tokio`. **Persistence**: SQLite (`sqlx`/`rusqlite`) + file-backed output capture.
- **Multiplex/discovery**: zellij; mDNS (`mdns-sd`); operator-established authenticated tunnels (no open
  listener by default).
- **Backends**: `workshop` (Canonical Workshop, Linux), `macos` (Seatbelt/App Sandbox), `fake` (local testing).
- **Design system**: locked to `design/` (canonical `design/Daedalus-Prototype-standalone.html`; GPUI mapping
  in `design/README.md`) — Yaru/mac skins, dark+light, color-blind-safe status palette (icon+label). Port it.

## Project Structure

```text
crates/
  daedalus-core/        daedalus-proto/        daedalus-app/
  daedalus-backend/     daedalus-backend-workshop/  daedalus-backend-macos/  daedalus-backend-fake/
  daedalus-discovery/   daedalus-zellij/       daedalus-sdk/
apps/
  desktop/              # GPUI surface (apps/web/ deferred)
tests/
  contract/             integration/
design/                 # visual source of truth: prototype export + design→GPUI mapping (README.md)
specs/001-agent-orchestration/   # spec, plan, research, data-model, contracts, quickstart, tasks
```

## Commands

```bash
cargo build --workspace
DAEDALUS_BACKEND=fake cargo run -p daedalus-desktop   # run locally, no Workshop needed
cargo test --workspace                                 # unit + contract + integration
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings
```

## Code Style & Principles (constitution v1.1.0)

- **TDD (non-negotiable)**: write a failing test first; red → green → refactor. Bugs start with a regression test.
- **Strict linting**: `clippy -D warnings` + `rustfmt`; no blanket `#[allow(...)]` (narrow + justified only).
- **Locally testable**: every change runnable locally via the `fake` backend / quickstart.
- **Design fidelity**: UI implementation designs MUST be validated against the HTML prototypes in
  `design/prototype/` (screens, states, themes, skins — see `design/README.md` + `design/storyboards.md`);
  deviations are recorded, never silent; UI without a prototype counterpart gets a prototype first.
- **Shared core, thin surfaces**: orchestration lives in `daedalus-core`; surfaces issue commands/queries only.
- **Sandbox isolation is absolute**; **local-first, no open listener**; **secrets never persisted/displayed**.

## Recent Changes

- 001-agent-orchestration (design + spec update 2026-07-06): the Claude Design prototype was **completed**
  (canonical runnable source now `design/prototype/`; the standalone export is superseded) — Needs-you
  attention queue, `awaiting`(waiting-for-input) + `confirm`(FR-015a) statuses, telemetry rail, Discover
  screen with source-drop, start/backends/tools failure states, storyboards. spec.md gained US6 +
  FR-015b/016a/019a/021a/021b/025a (+ SC-014/015); data-model gained `WaitingForInput`/`Unknown`,
  AttentionItem, prompt conventions, idle rates. Constitution is v1.1.0 (Principle IV: validate UI
  implementation designs against the HTML prototypes). **New unstarted work: tasks.md Phases 9–10
  (T069–T088)** — everything through Phase 8 remains green as below.
- 001-agent-orchestration (implemented 2026-06-19): Cargo workspace built out test-first. Headless stack
  complete and green (`cargo test --workspace`: 86 passing; `clippy -D warnings` clean) — `daedalus-proto`,
  `daedalus-backend` (+ `fake`/`workshop`/`macos`), SQLite persistence + file-backed capture, the session
  state machine, secret redaction, discovery (local/mDNS/tunnel + dedup) and the in-Workshop SDK, the
  orchestration `daedalus-core`, and the surface-agnostic `daedalus-app` API. Contract + integration tests
  live under `tests/` (`[[test]]` targets). `apps/desktop` ports the design system to typed tokens and builds
  screen view-models as compiled, unit-tested Rust. The native **GPUI window is implemented** behind the
  non-default `gpui` feature (`apps/desktop/src/gpui_ui.rs`): real `gpui`/`gpui_platform` deps, a live-
  refreshing fleet with Start/Stop/Confirm/Clean-up controls and AccessKit roles/labels; it builds, links,
  and opens a window (verified on Linux/Wayland). On Nix, `source scripts/gpui-env.sh` exposes the system
  libs gpui links against. The default build stays headless and CI-checked via `scripts/check-native.sh`.
  Pending (need real hardware): perf validation and the macOS run.
- 001-agent-orchestration: Rust workspace + GPUI desktop UI decided; web surface deferred (recorded spec
  deviation). Plan, research, data-model, contracts, and quickstart generated.

<!-- MANUAL ADDITIONS START -->
<!-- MANUAL ADDITIONS END -->
