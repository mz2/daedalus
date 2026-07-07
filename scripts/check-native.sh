#!/usr/bin/env bash
# T066a / FR-007b: fail if any Electron/CEF/system-webview dependency appears in the
# `daedalus-desktop` dependency tree. Daedalus ships a genuinely-native GPUI surface — no
# webview shell. Run this in CI and locally.
set -euo pipefail

# Inspect every feature combination so the `gpui` feature is covered too.
TREE="$(cargo tree -p daedalus-desktop --all-features --prefix none 2>/dev/null || cargo tree -p daedalus-desktop --prefix none)"

FORBIDDEN='^(wry|tao|webkit2gtk|webkit2gtk-sys|cef|electron|sciter|servo|webview|webview2|webview2-com)( |$)'

if echo "$TREE" | grep -Eiq "$FORBIDDEN"; then
    echo "ERROR: a webview/Electron dependency leaked into daedalus-desktop (FR-007b violated):" >&2
    echo "$TREE" | grep -Ei "$FORBIDDEN" >&2
    exit 1
fi

echo "OK: daedalus-desktop has no Electron/CEF/system-webview dependency (genuinely native)."
