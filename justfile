# Daedalus developer entry points (Constitution Principle III: one-command run/test/lint).
# `just` is optional; each recipe is a plain cargo invocation you can run by hand.

# Build the headless workspace (all crates except the GPUI feature of the desktop surface).
build:
    cargo build --workspace

# Run the desktop app against the in-memory fake backend (no Workshop needed).
# The real GPUI UI is behind the `gpui` feature; without it the binary runs headless.
run:
    DAEDALUS_BACKEND=fake cargo run -p daedalus-desktop

# Run the native GPUI desktop window (requires a GPU/display — research R-UI). On Nix,
# scripts/gpui-env.sh exposes the system libs (fontconfig, wayland/x11, vulkan, …) gpui needs.
run-gpui:
    bash -c 'source scripts/gpui-env.sh && DAEDALUS_BACKEND=fake cargo run -p daedalus-desktop --features gpui'

# Unit + contract + integration tests.
test:
    cargo test --workspace

# Lint: formatting + clippy with warnings-as-errors (Constitution Principle II).
lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings

# Auto-fix formatting.
fmt:
    cargo fmt --all
