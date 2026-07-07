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
  daedalus-backend-openshell/ NVIDIA OpenShell backend (kernel-sandboxed agent runtime;
                              Linux GPU-capable, macOS on Apple silicon via Docker Desktop)
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

- **Rust** 1.95+ (`rust-toolchain.toml` pins it; rustup auto-installs). The pinned `gpui`
  revision needs ≥1.95 (it uses `std::hint::cold_path`).
- **zellij** on `PATH` (session multiplexing / terminal attach).
- For the GPUI UI build only: a GPU/runtime (Metal on macOS; Vulkan/Blade on Linux).
  - **macOS**: full **Xcode** plus the **Metal Toolchain** component (gpui compiles Metal
    shaders at build time). If you see `missing Metal Toolchain`, run:
    ```bash
    sudo xcode-select -s /Applications/Xcode.app/Contents/Developer
    xcodebuild -downloadComponent MetalToolchain
    ```
  - **Linux/Nix**: `nix develop` (or `source scripts/gpui-env.sh`) provides the libs.
- Optional, for the `openshell` backend's real-sandbox tests: the
  [OpenShell](https://docs.nvidia.com/openshell/home) CLI and a running Docker daemon
  (Docker Desktop on macOS/Apple silicon) — see [Testing](#testing) below. Without them the
  real-backend test legs skip; everything else runs hermetically.

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

## Testing

Every change must be verifiable locally (constitution Principle III). The suite is layered:
the hermetic tiers always run; the real-backend tiers **self-activate** when their runtime is
present and skip silently when it is not — no flags, no separate test lists.

### 1. Hermetic suite (no sandbox runtime required)

```bash
cargo test --workspace                 # unit + contract + integration, all backends mocked
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings
```

Backend-specific contract tests run against scripted controls, so CLI invocation shapes and
failure paths are asserted without the real binaries, e.g.:

```bash
cargo test -p daedalus-tests --test contract_openshell_backend
```

### 2. Real-backend integration (OpenShell)

Requires the `openshell` CLI (installs a local gateway) and a running Docker daemon —
macOS/Apple silicon with Docker Desktop, or Linux:

```bash
curl -LsSf https://raw.githubusercontent.com/NVIDIA/OpenShell/main/install.sh | sh
openshell status                       # expect: Status: Connected
```

The SC-008 portability leg then runs the identical operator lifecycle against a **live
sandbox** (create → start → monitor → send input → stop → clean up → delete):

```bash
cargo test -p daedalus-tests --test integration_portability -- --nocapture
openshell sandbox list                 # expect: no daedalus-* sandboxes left behind
```

(A host-side `zellij: command not found` line is harmless if zellij isn't installed on the
host; the attach story inside sandboxes is tracked on issue #9.)

### 3. Manual policy smoke (what the backend enforces)

Inspect and verify the per-session confinement by hand — deny-all egress means the `curl`
must fail with `CONNECT tunnel failed, response 403`:

```bash
cat > /tmp/probe-policy.yaml <<'EOF'
version: 1
filesystem_policy:
  include_workdir: true
  read_only: [/usr, /lib, /proc, /dev/urandom, /etc, /var/log]
  read_write: [/sandbox, /tmp, /dev/null]
landlock: {compatibility: best_effort}
network_policies: {}
EOF
openshell sandbox create --name probe --policy /tmp/probe-policy.yaml \
  --no-auto-providers --no-tty -- echo ok
openshell sandbox exec -n probe --no-tty --timeout 15 -- curl -sS -m 5 https://example.com \
  && echo "UNEXPECTED: egress allowed" || echo "egress blocked (expected)"
openshell sandbox delete probe
```

### 4. In the app

`DAEDALUS_BACKEND=openshell cargo run -p daedalus-desktop` (add `--features gpui` for the
window). The backends view shows OpenShell **Available** when the CLI + Docker are up,
**Degraded** with a stated reason when Docker is stopped, **Unavailable** when the CLI is
missing (FR-028).

## Development workflow

This repository follows a spec-first ([Spec Kit](https://github.com/github/spec-kit))
workflow and a TDD-non-negotiable constitution (`.specify/memory/`): a failing test precedes
every behavioral change, linting is warnings-as-errors, and every change is runnable locally
via the `fake` backend. Specs, plans, and the per-feature task list live under `specs/`;
Spec Kit workflow skills are in `.claude/skills/`.

## License

Licensed under the [GNU Affero General Public License v3.0](LICENSE).
