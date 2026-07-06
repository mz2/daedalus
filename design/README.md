# Daedalus Design System → GPUI

This folder holds the **visual source of truth** for the Daedalus desktop app and how it maps onto the
native **GPUI** implementation (`apps/desktop`). It realizes and extends the text
[`specs/001-agent-orchestration/design-brief.md`](../specs/001-agent-orchestration/design-brief.md).

## Source files

| File | Role |
|------|------|
| `prototype/` | **Canonical, most current source** (completed 2026-07-03) — readable React/JSX + CSS. Run it via a local server from `design/prototype/` (e.g. `python3 -m http.server`) and open `Daedalus Prototype.html`. Files: `daedalus.css` (tokens), `components.css`, `data.js` (mock data incl. terminal transcripts + lifecycle events), `app.jsx` (shell), `tasks.jsx` / `fleet.jsx` / `session.jsx` / `needs.jsx` / `screens.jsx` (screens), `primitives.jsx`, `icons.jsx`, `palette.jsx`, `assets/daedalus-icon.svg`, `screenshots/`. Contains the complete state set: `awaiting` + `confirm` statuses, the Needs-you queue, the telemetry rail, Discover as a top-level screen, and the Start/Backends/Tools failure states. |
| `Daedalus-Prototype-standalone.html` | **Older single-file export** (2026-06-19). Predates the Needs-you feature, the `confirm` status, the telemetry rail, the Discover screen, and the failure-state work. Kept for reference until a fresh export replaces it. |

The design is also authored in the operator's Claude project ("Daedalus"); `design/prototype/` and that
project are kept in step (last synced 2026-07-03 — the repo copy is the same content that was pushed).

> The prototype is a **visual/interaction reference**, not shippable code — Daedalus ships a native GPUI
> desktop GUI (no webview; web surface deferred). Translate tokens and layouts to GPUI; do not embed the HTML.
> The prototype's "Tweaks" panel is a design-exploration tool, **not** a product feature — but its "Screen
> states" section is the quickest way to reproduce every cross-cutting state (see below and
> [`storyboards.md`](./storyboards.md)).

## Platform skins ↔ GPUI targets

The design ships two native skins that map directly to our two desktop targets:

- **`yaru`** — Ubuntu / Yaru. Ubuntu font, **Ubuntu orange `#E95420`** accent, GNOME/libadwaita status palette,
  squared right-side window controls. Default skin (fits the Canonical Workshop context). → Linux build.
- **`mac`** — Cupertino. SF font, **warm amber `#E0901C`** accent, left traffic-light window controls. → macOS build.

Switching skin also adopts that platform's signature accent. GPUI should select the skin from the host OS.

## Themes & density

- **Themes**: `dark`, `light`, and `system` (follows the OS appearance; the prototype's default). Both
  color themes required, WCAG-AA, color-blind-safe.
- **Density**: `compact` / `regular` (default) / `comfy` → row height 26/30/36px, card pad 10/14/19px, gap 9/12/17px.

## Status palette (the most-reused element)

Functional, hue-distinct, **always paired with icon + label** (never color alone). Maps 1:1 to
`SessionStatus`/`TaskStatus` in `daedalus-proto`. Token names in `daedalus.css`: `--st-starting`,
`--st-await`, `--st-confirm`, `--st-running`, `--st-stalled`, `--st-failed`, `--st-completed`,
`--st-stopped`, `--st-unknown`, `--st-unreach`.

| Status | Meaning | Yaru (Linux) | mac |
|--------|---------|--------------|-----|
| starting | provisioning/launching | `#3584E4` | `#0A84FF` |
| running | live | `#2EC27E` | `#2DC653` |
| stalled | no progress — attention | `#E5A50A` | `#FF9F0A` |
| failed | crash/abnormal exit | `#E01B24` | `#FF453A` |
| **awaiting** | **waiting for input** — the agent asked a question mid-run and is blocked on the operator | `#C061CB` | `#BF5AF2` |
| **confirm** | **awaiting confirmation** (FR-015a) — clean exit with tracked tasks unfinished; operator confirms completion or cleans up | `#C061CB` | `#BF5AF2` |
| completed | all tasks done / confirmed | `#1C9B8E` | `#26B5A8` |
| stopped | stopped by operator | `#77767B` | `#98989F` |
| unknown | connection lost — last-known state preserved, never shown healthy | `#9A9996` | `#B0B0B8` |
| unreachable | source gone (mDNS/tunnel) | `#77767B` | `#8E8E96` |

**`awaiting` and `confirm` deliberately share the same purple family**: purple means "blocked on an
operator decision". The two are distinguished by glyph (speech-bubble vs. clipboard-check), label
("Waiting for input" vs. "Awaiting confirmation"), the session-detail banner, and the offered controls
(answer in terminal vs. Confirm completion / Clean up) — never by hue alone. Port both tokens even
though the hex values match.

Badge styles: **pill** (filled), **dot + label**, **icon-led** (prototype default). Running uses a live
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

- Title bar (brand + maze icon, global status counts — including a purple **awaiting + confirm** count,
  ⌘K search trigger, notifications, settings; skin-correct window controls) — `app.jsx`
- Sidebar / top-bar nav with a **Hosts** list (availability dots); in top-bar mode the hosts list becomes
  the **HostsIndicator** — a compact pill (one dot per host, worst state first) opening a popover — `app.jsx`
- Status badge, chips (incl. mono, SpecKit, and issue chips), buttons (primary/tinted/danger/ghost/sm),
  icon buttons, segmented control, input, `kbd`. **Source/backend chips accept an availability state**
  and render a small dot **only when not "available"** — a green dot on every healthy chip would be noise.
- Card / surface / grouped (inset) list rows, progress meter, resource meter (`resmeter`)
- Session card (compact + detail-header variants), tracked-task card, task-status board
- **Needs-you queue** (`needs.jsx`): one `NeedsRow` (card and line tones) rendered in three placements —
  `NeedsView` (dedicated screen + nav item), `NeedsTray` (header popover), `NeedsStrip` (pinned band on
  the Tasks home). Rows carry a cue ("Asked you a question", "Run ended — confirm completion", …), the
  reason/prompt, a time-waiting readout, an **idle-cost readout** ("idle · $x.xx" at `IDLE_RATE` $/hr for
  live sandboxes; "env held" for ended sessions), and a deep-link action ("Answer in terminal" /
  "Review & confirm"). `NeedsTag` is the matching inline marker in the Tasks and Sessions lists.
- **FailureNotice + FieldError** (`primitives.jsx`) — the brief-§8 named error pattern: bold title,
  plain-language (optionally monospace) reason, and next actions; a failure is never a dead end.
  `FieldError` is the inline field-level variant (Start form, Register-tool modal).
- **Session state banners** (`StateBanner` in `session.jsx`) — one per status, incl. `banner-await`
  (quoted question + waiting time) and `banner-confirm` (agent summary, exit code, task count, and
  **Confirm completion / Clean up** controls).
- **Telemetry rail** (`session.jsx`, FR-019): CPU/memory **sparklines** (`Sparkline`/`RailSpark`), disk
  meter, runtime; a **lifecycle/event timeline** (curated `EVENTS` in `data.js`, nodes tinted
  `var(--st-*)`); an **Outcome block** for ended/disconnected sessions (state, exit code, reason,
  persisted-output note). Toggled from the session header; when hidden, a collapsible `SessionMetrics`
  footer under the task board keeps the essentials.
- Embedded terminal pane (mono, toolbar, zellij tab bar) — GPUI view over `alacritty_terminal`.
  Includes the **trimmed-output notice** (`.term-trim`): a pinned "Output trimmed — showing last N of M
  lines · full log persisted" strip for excessive-output sessions, keeping the terminal responsive.
- Command palette (⌘K), notifications popover, settings modal

## Information architecture

- **Home is the aggregate Tasks board** (what every agent is doing at the task level), as in the locked
  prototype — not the Fleet list. **Fleet** ("Sessions") remains the session-level unified view (FR-025),
  one click away. This supersedes the design-brief §5 ordering; rationale: the operator's primary
  question is "what are the agents working on / what needs attention". Reversible if usability testing
  argues otherwise.
- **Discover is a top-level nav item** (radar icon) — brief §5 is now satisfied as written. Nav order:
  Tasks · Sessions · Discover · Tools · Environments. The Environments screen cross-links to Discover.
- **Start session** is a prominent action (header button + ⌘N), not a nav item.
- **Needs you** — the unified operator-attention queue (awaiting / confirm / stalled / failed /
  disconnected) — has three candidate placements, switched by the `needsPlacement` tweak: a dedicated
  nav view (default), a header tray, or a pinned strip on the Tasks home. GPUI should ship one; the
  dedicated view is the current default.

## Screens ↔ tasks

| Screen (prototype) | Spec / design-brief | tasks.md |
|--------------------|---------------------|----------|
| Tasks board (home) | task-status board, FR-009/017 | T031, T035 |
| Needs you (queue) | operator-attention queue; extends §6.1 global status + §8 stalled/awaiting patterns, FR-015a/021 | — new, needs a task |
| Fleet (list/table) | US5, §6.2, FR-025 | T058 |
| Session detail (split + telemetry rail) | US2/US4, §6.4, FR-019 | T035, T052 |
| Start session | US1, §6.3, FR-005/026/002a | T026 |
| Discover | US3, §6.5, FR-010–014 | T046 |
| Tools | §6.6, FR-001a | T060 |
| Environments / Backends | §6.7, FR-027/028 | T059 |
| Settings (modal) | §6.8, FR-032 | T064 |

## Cross-cutting states (brief §8) — where each is demonstrated

Every named pattern is reproducible in the prototype; the "Screen states" section of the Tweaks panel
drives most of them (see [`storyboards.md`](./storyboards.md) for the full state matrix).

| Pattern | Screen | Reproduce via |
|---------|--------|---------------|
| Empty (first run) | Fleet · Tools · Discover · Environments | Tweaks: Fleet state = Empty; "Tools empty (first run)"; Discover state = Empty; Backends state = No environments |
| Loading / scanning | Fleet · Discover | Tweaks: Fleet state = Loading (skeleton cards); Discover state = Scanning (or the Rescan button) |
| Error / failure | Start flow · Tools modal · Session detail | Tweaks: Start flow state = Validation / Provisioning failed / Worktree creation failed; Register-tool modal ("Validate definition" seeds a malformed command); session `s-904` (failed) |
| Degraded / unavailable | Environments · Discover · chips everywhere | Backends state = Host down (FR-028 "other hosts keep working" reassurance); `studio-mac.local` degraded note; Discover state = Source dropped (FR-014); availability dots on chips + HostsIndicator |
| Limit reached | Start flow | Tweaks: Start flow state = Concurrency limit reached (FR-026) |
| Stalled / needs attention | Session detail · Needs you | session `s-902` ("Open a stalled session →" tweak button) |
| Awaiting input / confirmation | Session detail · Needs you | sessions `s-909`/`s-910` (awaiting; "Answer a waiting session →"), `s-911` (confirm; "Review a session to confirm →") |
| Disconnected / unknown | Session detail · Fleet | session `s-908` — banner, "last-known" chip, rail metrics unavailable, terminal connection-lost divider (FR-020) |
| Excessive output | Session detail | session `s-906` — pinned trimmed-output notice |
| Review mode (ended) | Session detail | sessions `s-903` (completed), `s-907` (stopped) — persisted output, outcome block (FR-018) |

## Keyboard shortcuts (from the prototype)

⌘K command palette · ⌘N start session · ⌘⇧L toggle theme · ⌘[ back from session detail. Carry these into
GPUI; add session-switching and focus-terminal per the design brief.

## App icon

`prototype/assets/daedalus-icon.svg` — a labyrinth (Daedalus, the maze-maker) with Ubuntu-orange walls.
Use as the app/window icon.
