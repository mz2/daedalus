# daedalus

A single-operator, **local-first** application for orchestrating and monitoring autonomous
agentic tools that run inside **isolated sandbox environments**. The operator starts
*sessions* (an agentic tool working an SDD-defined objective inside an environment), watches
them through an embedded terminal and a per-session task-status board, discovers sessions
across local / mDNS / tunneled-Workshop sources, attaches via [zellij](https://zellij.dev/),
and controls their lifecycle (stop / send-input / clean-up).

Daedalus is local-first by construction: **no open network listener by default**; remote
hosts are reached only over operator-established authenticated tunnels. Secrets are never
managed, injected, or persisted — captured output is redacted before it is streamed or
stored.

> Status: the orchestration core and all backends/discovery/persistence are implemented and
> tested (`cargo test --workspace`). The native desktop UI is built with **GPUI**; its
> rendering is behind the `gpui` feature (it needs a GPU/display). The **web surface is
> deferred** — the core is surface-agnostic so it can be added later. See
> [`specs/001-agent-orchestration/`](specs/001-agent-orchestration/) for the spec, plan,
> research, and tasks.

## Architecture

End-to-end **Rust** (2021 edition), one Cargo workspace, no FFI. A shared core owns all
orchestration; surfaces only issue commands/queries and subscribe to an event stream.

```text
crates/
  daedalus-proto/             shared serde contract types (ids, status, entities, app-API DTOs)
  daedalus-backend/           the Backend trait + provisioning value types
  daedalus-backend-fake/      in-memory backend for local testing (no Workshop needed)
  daedalus-backend-workshop/  Canonical Workshop (Linux) backend
  daedalus-backend-macos/     macOS local sandbox backend (Seatbelt / App Sandbox)
  daedalus-zellij/            zellij multiplexing/attach + terminal-channel glue
  daedalus-sdk/               in-Workshop advertisement + SpecKit tasks.md parser
  daedalus-discovery/         local / mDNS / tunnel sources + stable-identity de-duplication
  daedalus-core/              sessions, lifecycle state machine, persistence, redaction,
                              task board, fleet, concurrency, reconciliation
  daedalus-app/               surface-agnostic Command / AppQuery / AppEvent facade
apps/
  desktop/                    native GPUI surface (design-system port + screen view-models);
                              GPUI rendering behind the `gpui` feature (apps/web/ deferred)
tests/                        contract + integration test harness ([[test]] targets)
design/                       visual source of truth (prototype export + design→GPUI mapping)
```

Persistence is a single local SQLite file plus per-session capture files for terminal
output. See [`specs/001-agent-orchestration/data-model.md`](specs/001-agent-orchestration/data-model.md)
and [`research.md`](specs/001-agent-orchestration/research.md) for the design decisions.

## Prerequisites

- **Rust** 1.83+ (`rustup`), with `clippy` and `rustfmt`.
- **zellij** on `PATH` (session multiplexing / terminal attach).
- For the GPUI UI build only: a GPU/runtime (Metal on macOS; Vulkan/Blade on Linux).
- macOS only, for the `macos` backend: `sandbox-exec` (system-provided).

## Build · run · test · lint

```bash
cargo build --workspace

# Run the app against the in-memory fake backend (no Workshop needed).
DAEDALUS_BACKEND=fake cargo run -p daedalus-desktop          # headless build (no GPU)

cargo test --workspace                                       # unit + contract + integration
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings
./scripts/check-native.sh                                    # assert no webview/Electron deps
```

### Native GPUI window

The real window is built with **GPUI** (Zed's GPU framework) behind the `gpui` feature — it
needs a GPU/display and several system libraries (fontconfig, freetype, libxkbcommon,
wayland/x11, vulkan) that gpui `dlopen`s at run time. The simplest way to get them all is the
**Nix dev shell**, which also pins the Rust toolchain and `zellij`:

```bash
nix develop                                                  # provides rust + the system libs
DAEDALUS_BACKEND=fake cargo run -p daedalus-desktop --features gpui
```

> Without the dev shell (or `source scripts/gpui-env.sh` as a fallback), the window panics on
> startup with `NoWaylandLib` because those runtime libraries aren't on `LD_LIBRARY_PATH`. On
> a non-Nix distro, install the `-dev` packages and run directly.

The window shows the title bar with live status counts, a backends sidebar, and the fleet with
**Start session / Stop / Confirm / Clean up** controls (state refreshes ~2×/sec). A
[`justfile`](justfile) wraps everything as `just build|run|run-gpui|test|lint`. The default
headless build keeps CI and local TDD fast; the GPUI renderer is opt-in (research risk R-UI).
See [`specs/001-agent-orchestration/quickstart.md`](specs/001-agent-orchestration/quickstart.md).

## Development workflow

This repository follows a spec-first ([Spec Kit](https://github.com/github/spec-kit))
workflow and a TDD-non-negotiable constitution (`.specify/memory/`): a failing test precedes
every behavioral change, linting is warnings-as-errors, and every change is runnable locally
via the `fake` backend. Specs, plans, and the per-feature task list live under `specs/`;
Spec Kit workflow skills are in `.claude/skills/`.

## License

Licensed under the [GNU Affero General Public License v3.0](LICENSE).
