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
  *Implementation note (T077)*: the mock data shows "env held" on every ended row; the app derives chip
  presence from the core's `CostIndication` (FR-021b), so failed/disconnected rows whose environment is
  not reported as held show no cost chip — chip copy is unchanged.
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
| Tasks board (home, aggregate) | FR-025a, FR-009/017 | T031, T035; aggregate home: T079–T080 |
| Needs you (queue) | US6, FR-015b/021/021a/021b | T069–T078 |
| Fleet (list/table) | US5, §6.2, FR-025 | T058 (re-validate: T085) |
| Session detail (split + telemetry rail) | US2/US4, §6.4, FR-019/019a/016a | T035, T052; rail/trim refresh: T081–T082 |
| Start session | US1, §6.3, FR-005/026/002a | T026 (re-validate: T085) |
| Discover | US3, §6.5, FR-010–014 | T046 (re-validate: T085) |
| Tools | §6.6, FR-001a | T060 (re-validate: T085) |
| Environments / Backends | §6.7, FR-027/028 | T059; degraded+shell indicator: T083 |
| Settings (modal) | §6.8, FR-032, FR-021b | T064; system theme: T084; idle rates: T086 |

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

## Recorded deviations (Phase 10 validation, 2026-07-06)

Validated against the served prototype (tasks board grouping/counts/filters/sub-lines; `s-911` rail;
`s-906` trim notice; `s-908` metrics-unavailable). Copy matches exactly, including "Output trimmed —
showing last N of M lines · full log persisted", "Live metrics unavailable — connection lost. Showing
last-known state only.", "Output persisted — readable in review mode.", "Last-known output preserved
with its timestamp.", "Ran for"/"Runtime", "SIGTERM (operator)"/"code N", the `NEEDS_META` inline tag
cues, and the "Hide/Show telemetry rail" toggle. Deliberate deviations:

- **Timeline labels are derived, not curated**: the prototype's `EVENTS` are hand-written mock lines
  (e.g. "Agent started (claude -p)"). The implementation derives entries from persisted records —
  lifecycle transitions ("Created — provisioning environment", "Agent started", status label + note),
  `OperatorAction` events ("Started by operator", "Input sent by operator", …), and task-status
  changes as dim notes ("T005 — Done"). Shape (status-colored node · label · timestamp) is preserved.
- **Resource units**: the prototype shows CPU/mem/disk all as `%`; real metrics carry memory/disk in
  bytes, so the rail renders CPU as `%` and memory/disk as human-readable bytes, disk as a meter.
- **GPUI sparkline depth**: the headless view-model carries the full recent sample history
  (`SPARKLINE_SAMPLES` = 28, matching the prototype's point count); the current GPUI layer summarizes
  it (value + sample count) pending a drawn polyline, at parity with its existing rendering depth.
- **Tasks landing filters**: the full filter state (status chips, session/tool/backend selects, text
  query, needs-attention chip, "No tasks match" + Clear filters) lives tested in the view-model; the
  GPUI layer currently renders the unfiltered grouped list with the needs strip on top.

Validated 2026-07-06 (T083/T084/T086): hosts pill + popover (title "Hosts — availability", one dot per
host worst-first, rows dot + kind icon + name + availability text, footnote "Availability from local
checks, mDNS presence, and tunnel state."), Backends `host-down` ("Other hosts keep working — sessions
elsewhere are unaffected.", "Reconnect", degraded/down `brow-note` reasons), Settings theme segment
(System | Light | Dark, System default + pressed, "Following your operating system's Light / Dark
appearance." only under System). Further deliberate deviations:

- **`idle` availability**: prototype hosts/tunnels carry an `idle` state rendered with the degraded
  dot. The implementation's `Availability` is `available | degraded | unavailable` (data-model);
  idle-shaped states map to `Degraded`. The dot hues match (`AvailDot`: running/stalled/failed).
- **Hosts popover vs. sidebar**: the tested `HostsIndicator` view-model carries the pill (dots,
  count, accessible label) and the full popover row data; the current GPUI layer renders the pill in
  the titlebar and the rows persistently in the sidebar footer (the prototype's sidebar-mode hosts
  list) rather than as a click-popover, at parity with its existing depth.
- **Idle-rate rows (FR-021b)**: the prototype Settings modal has no idle-rate control; the spec
  requires one. Implemented as Settings rows (`IdleRateRow`, "$x.xx/hr", clearable) in the tested
  view-model; the GPUI layer exposes a single idle-rate field for the default backend kind.
- **Reconnect**: the prototype's host-down `Reconnect` button is a mock; the implementation surfaces
  it on unavailable rows and availability re-resolves on the shell's refresh tick (tunnel
  re-establishment is the operator's action, FR-032).

## Recorded implementation deviations (T085)

Re-validated the pre-update screens (Start T026 · Session T035 · Discover T046 · Fleet T058 ·
Backends T059 · Tools T060 · Settings T064) against the completed prototype, state by state
(served `design/prototype/`, Tweaks "Screen states"). **Copy now matches verbatim**, pinned by unit
tests in `apps/desktop/src/screens/*`: all six Start-flow states (validation field errors + footer
hints, and the FailureNotice set — "Provisioning failed" / "Concurrency limit reached — N of M
sessions running" / "Environment unreachable" / "Couldn't create an isolated worktree", with the
"No session was created — nothing to clean up." and "…the worktree never attached." reassurances,
FR-005/026/002a), the worktree + secrets trust banners (FR-002a/031), every Session `StateBanner`
(starting/awaiting/confirm/stalled/failed/unknown/completed/stopped incl. quoted prompt/summary and
the "exited cleanly (code N) · d/t tasks · waiting w" meta line) plus the review-mode input notice
and the prototype header-control rules (Clean-up disabled while live, Stop ↔ Confirm completion ↔
Start similar), Discover's three always-rendered groups with per-group empty notes, "Review only" /
"Not attachable" reasons, the FR-013 dedup footnote, the FR-014 source-dropped treatment (kept
listed, "Last seen before the tunnel dropped…", disabled Connect with reason, group Reconnect) and
the nothing-discovered explanation, Fleet's harmonized row (spec sub-line, progress, inline
NeedsTag, attention-first order, full 8-status filter set, "No sessions yet" empty state), the
Backends "No environments yet" empty state (§6.7; backed by a new `Core::environments()` query),
Tools' first-run empty state and register-modal validation (required/duplicate name, required
command, unbalanced quotes, mono "command not found in sandbox PATH", FR-001a), and the Settings
local-first / outbound-only / tunnels-note / concurrency copy with the 1–16 slider bounds and the
five prototype notification toggles. Deliberate deviations that remain:

- **GPUI rendering depth**: the fixes above live in the tested headless view-models (the surface
  contract); the current `gpui_ui.rs` layer still renders its earlier, shallower composition of
  each screen (e.g. it does not yet draw the StateBanner, FailureNotice, or Discover grouping).
  Follow-up: port the new view-model fields into the gpui layer screen by screen.
- **Fleet spec sub-line**: the prototype's `row-branch` shows a spec *branch* name
  (`specs/044-device-flow`); the implementation carries the tracked SDD artifact ref
  (`…/tasks.md` via `SessionSummary.spec`) — same slot, real data. Row age and CPU/mem cells
  (prototype table layout) are not yet carried; `SessionSummary` would need timestamps.
- **Discover dropped-group header**: the prototype's warn header names the tunnel and drop time
  ("tunnel eu-fra-1 dropped 2m ago"); discovery does not yet expose a source display-name or
  drop timestamp, so the view-model carries `dropped: bool` + Reconnect and the per-row FR-014
  note instead. Scanning spinners and the Rescan button stay renderer-transient (no state field).
- **Start-failure wiring**: the `StartFailure` notices are constructed from core errors by the
  surface; the prototype's "View host" action has no navigation target yet (Environments screen
  row focus is a follow-up).
- **Settings persistence**: `TunnelRow` (name/endpoint/active + Connect) and the five per-event
  `NotificationToggle`s are modeled and tested, but core persists only the single
  notifications-enabled flag and tunnels are operator-established outside the app (FR-032) — the
  rows are populated by the surface, not yet by a core settings store.
- **Session stalled duration**: the banner's "No progress for {d}" derives from the last persisted
  event timestamp (the stall *is* the absence of newer events); the prototype uses mock
  `lastEvent` seconds. Same meaning, derived source.

## Recorded deviations (Start + Session GPUI deepening, 2026-07-08)

The GPUI layer now draws the prototype structure for the **Start flow** (numbered step
sections, the 2-column tool card grid with icon tile + capability chip + accent selection
border, the launch footer bar with the tested hint copy/tones, × Cancel in the header) and
the **Session detail** (header controls row, objective as headline, `sess-chips` meta chips
incl. the green `worktree-isolated`, the `term-bar` liveness pill — LIVE / persisted output /
last-known output — with the send-input row under the pane, and the Tasks panel: "from
SpecKit tasks.md", done/total progress bar, four status columns with per-card id + dot and
the accent border on in-progress cards). Deliberate deviations that remain:

- **Start: no repository / spec-branch browser** (prototype step 2's repo grid + Existing/New
  spec toggle): the core has no repo/spec discovery yet — the objective + tasks.md fields
  carry the same data directly. Follow-up when a repo-browsing query exists.
- **Start: no Review & launch modal**: the footer's primary action launches directly (button
  reads "Start session →", not "Review & launch"). FailureNotice rendering also still
  pending in GPUI (copy lives tested in the view-model).
- **Start: tool cards show no version/description**: `ToolDef` has neither field; cards show
  name + capability chip only.
- **Session: terminal icon buttons** (info/search/copy/jump-to-latest) and the **zellij tab
  strip** are not drawn — the embedded terminal renders zellij's own UI inside the pane; the
  in-progress-column collapse toggle and rail sparklines are still view-model-only.

## Keyboard shortcuts (from the prototype)

⌘K command palette · ⌘N start session · ⌘⇧L toggle theme · ⌘[ back from session detail. Carry these into
GPUI; add session-switching and focus-terminal per the design brief.

## App icon

`prototype/assets/daedalus-icon.svg` — a labyrinth (Daedalus, the maze-maker) with Ubuntu-orange walls.
Use as the app/window icon.
