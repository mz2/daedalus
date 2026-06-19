# daedalus Development Guidelines

Auto-generated from all feature plans. Last updated: 2026-06-19

## Active Technologies

- **Language**: Rust 1.83+ (2021 edition) end-to-end — core, backends, discovery, SDK, shared contracts, and UI.
- **UI**: GPUI (gpui.rs) + `gpui-component` — native desktop GUI (macOS + Linux), GPU-rendered, no
  Electron/webview, in-process with the core (no FFI). Embedded terminal via `alacritty_terminal` attached
  to zellij. (Web surface deferred — see `specs/001-agent-orchestration/research.md` §R1.)
- **Async/runtime**: `tokio`. **Persistence**: SQLite (`sqlx`/`rusqlite`) + file-backed output capture.
- **Multiplex/discovery**: zellij; mDNS (`mdns-sd`); operator-established authenticated tunnels (no open
  listener by default).
- **Backends**: `workshop` (Canonical Workshop, Linux), `macos` (Seatbelt/App Sandbox), `fake` (local testing).

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
specs/001-agent-orchestration/   # spec, plan, research, data-model, contracts, quickstart
```

## Commands

```bash
cargo build --workspace
DAEDALUS_BACKEND=fake cargo run -p daedalus-desktop   # run locally, no Workshop needed
cargo test --workspace                                 # unit + contract + integration
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings
```

## Code Style & Principles (constitution v1.0.0)

- **TDD (non-negotiable)**: write a failing test first; red → green → refactor. Bugs start with a regression test.
- **Strict linting**: `clippy -D warnings` + `rustfmt`; no blanket `#[allow(...)]` (narrow + justified only).
- **Locally testable**: every change runnable locally via the `fake` backend / quickstart.
- **Shared core, thin surfaces**: orchestration lives in `daedalus-core`; surfaces issue commands/queries only.
- **Sandbox isolation is absolute**; **local-first, no open listener**; **secrets never persisted/displayed**.

## Recent Changes

- 001-agent-orchestration: Rust workspace + GPUI desktop UI decided; web surface deferred (recorded spec
  deviation). Plan, research, data-model, contracts, and quickstart generated.

<!-- MANUAL ADDITIONS START -->
<!-- MANUAL ADDITIONS END -->
