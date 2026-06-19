# Daedalus Design System → GPUI

This folder holds the **visual source of truth** for the Daedalus desktop app and how it maps onto the
native **GPUI** implementation (`apps/desktop`). It realizes and extends the text
[`specs/001-agent-orchestration/design-brief.md`](../specs/001-agent-orchestration/design-brief.md).

## Source files

| File | Role |
|------|------|
| `Daedalus-Prototype-standalone.html` | **Canonical, latest export** (2026-06-19) — single self-contained HTML/React prototype. Open in a browser to view all screens/skins/themes. |
| `prototype/` | Week-older **unpacked** version (2026-06-12) for readable source — `daedalus.css` (tokens), `components.css`, `*.jsx` (screens), `data.js` (mock data), `assets/daedalus-icon.svg`, `screenshots/`. Slightly stale: missing the awaiting-confirmation status added in the standalone. |

> The prototype is a **visual/interaction reference**, not shippable code — Daedalus ships a native GPUI
> desktop GUI (no webview; web surface deferred). Translate tokens and layouts to GPUI; do not embed the HTML.
> The prototype's "Tweaks" panel is a design-exploration tool, **not** a product feature.

## Platform skins ↔ GPUI targets

The design ships two native skins that map directly to our two desktop targets:

- **`yaru`** — Ubuntu / Yaru. Ubuntu font, **Ubuntu orange `#E95420`** accent, GNOME/libadwaita status palette,
  squared right-side window controls. Default skin (fits the Canonical Workshop context). → Linux build.
- **`mac`** — Cupertino. SF font, **warm amber `#E0901C`** accent, left traffic-light window controls. → macOS build.

Switching skin also adopts that platform's signature accent. GPUI should select the skin from the host OS.

## Themes & density

- **Themes**: `dark` (default) and `light` — both required, WCAG-AA, color-blind-safe.
- **Density**: `compact` / `regular` (default) / `comfy` → row height 26/30/36px, card pad 10/14/19px, gap 9/12/17px.

## Status palette (the most-reused element)

Functional, hue-distinct, **always paired with icon + label** (never color alone). Maps 1:1 to
`SessionStatus`/`TaskStatus` in `daedalus-proto`.

| Status | Meaning | Yaru (Linux) | mac |
|--------|---------|--------------|-----|
| starting | provisioning/launching | `#3584E4` | `#0A84FF` |
| running | live | `#2EC27E` | `#2DC653` |
| stalled | no progress — attention | `#E5A50A` | `#FF9F0A` |
| failed | crash/abnormal exit | `#E01B24` | `#FF453A` |
| **awaiting** | **awaiting operator confirmation** (FR-015a) | `#C061CB` | `#BF5AF2` |
| completed | all tasks done / confirmed | `#1C9B8E` | `#26B5A8` |
| stopped | stopped by operator | `#77767B` | `#98989F` |
| unknown | indeterminate | `#9A9996` | `#B0B0B8` |
| unreachable | source gone (mDNS/tunnel) | `#77767B` | `#8E8E96` |

Badge styles: **pill** (filled, default), **dot + label**, **icon-led**. Stalled/running use an attention
pulse; starting uses a spinner. Respect `prefers-reduced-motion`.

## Typography

- **UI**: Ubuntu (yaru) / SF Pro (mac), system fallbacks.
- **Mono** (terminal, IDs, metrics): JetBrains Mono.

## Tokens (translate to GPUI theme constants)

- **Radii**: xs 4 / sm 6 / md 9 (yaru 6) / lg 13 (yaru 8) / xl 18 (yaru 10) / pill 999; window 11–12, control = sm/md.
- **Motion**: ease `cubic-bezier(.32,.72,.32,1)`; fast .13s / med .22s.
- **Surfaces (dark/yaru)**: app `#0d0d0d`, window `#242424`, content `#1e1e1e`, elevated `#2f2f2f`,
  sidebar `#2a2a2a`, titlebar `#303030`. Fills/separators/text-1/2/3 and card/pop shadows per
  `prototype/daedalus.css` (the authoritative token list — port every `--var` for each theme×skin).

## Components ↔ GPUI

Build these as reusable GPUI components (port from `prototype/components.css` + `*.jsx`):

- Title bar (brand + maze icon, global status counts, ⌘K search trigger, notifications, settings; skin-correct window controls)
- Sidebar / top-bar nav with a **Hosts** list (availability dots) — `app.jsx`
- Status badge, chips (incl. mono + SpecKit chip), buttons (primary/tinted/danger/ghost/sm), icon buttons, segmented control, input, `kbd`
- Card / surface / grouped (inset) list rows, progress meter, resource meter (`resmeter`)
- Session card (compact + detail-header variants), tracked-task card, task-status board
- Embedded terminal pane (mono, toolbar) — GPUI view over `alacritty_terminal`
- Command palette (⌘K), notifications popover, settings modal

## Information architecture decision (home view)

**Decision**: the **landing/home view is the aggregate Tasks board** (what every agent is doing at the
task level), as in the locked prototype — not the Fleet list. **Fleet** remains the session-level unified
view (FR-025) and is one click away in the nav. This supersedes the design-brief §5 ordering (which listed
Fleet first). Rationale: the operator's primary question is "what are the agents working on / what needs
attention," which the task board answers most directly; Fleet is for session-level triage. Reversible if
usability testing argues for a Fleet-first landing.

## Screens ↔ tasks

| Screen (prototype) | Spec / design-brief | tasks.md |
|--------------------|---------------------|----------|
| Tasks board (home) | task-status board, FR-009/017 | T031, T035 |
| Fleet (list/table) | US5, §6.2, FR-025 | T058 |
| Session detail (split) | US2/US4, §6.4 | T035, T052 |
| Start session | US1, §6.3 | T026 |
| Discover | US3, §6.5 | T046 |
| Tools | §6.6, FR-001a | T060 |
| Environments / Backends | §6.7, FR-027/028 | T059 |
| Settings (modal) | §6.8 | T064 |

## Keyboard shortcuts (from the prototype)

⌘K command palette · ⌘N start session · ⌘⇧L toggle theme · ⌘[ back from session detail. Carry these into
GPUI; add session-switching and focus-terminal per the design brief.

## App icon

`prototype/assets/daedalus-icon.svg` — a labyrinth (Daedalus, the maze-maker) with Ubuntu-orange walls.
Use as the app/window icon.
