# UI Design Brief: Daedalus

**Feature**: [spec.md](./spec.md) — Orchestrate and Monitor Agentic Tools in Sandbox Environments
**Branch**: `001-agent-orchestration`
**Created**: 2026-06-06
**Audience**: A UI/UX design effort (e.g., handed to Claude Design) to produce the visual design and
screen flows for Daedalus.

> This brief is design-focused and derived from the spec. Where the spec is technology-agnostic, so is
> this brief — it specifies *what screens, components, states, and flows* are needed, not visual taste.
> Cross-references like `FR-0xx` / `SC-0xx` point back to the spec for the authoritative requirement.

---

## 1. What Daedalus is

Daedalus is a tool for a single operator to **orchestrate and monitor autonomous "agentic" tools**
(e.g., coding agents) that run inside **isolated sandbox environments** — primarily Canonical Workshop
(Linux) and a macOS-specific local sandbox. The operator starts agent **sessions**, watches them work
through a **live terminal** and a **task-status board**, intervenes (stop / send input), and manages
many sessions across one or more hosts and backends from one place.

The product's whole reason to exist is **trustworthy visibility and control over opaque, long-running
autonomous agents**. The design should make an agent's state, progress, and safety legible at a glance.

## 2. Who uses it and where

- **Primary persona — "the Operator"**: a developer/engineer running agents to do real work. Technical,
  comfortable with terminals, values density and keyboard efficiency over hand-holding. Single-user,
  local-first; **no multi-user/login in v1** (FR-032–034).
- **Context of use**: at a desk on a desktop app (macOS or Linux), or in a browser. Often monitoring
  **several agents at once** while doing other work — so glanceability and good notifications matter
  (FR-021, SC-006).

## 3. Design goals & principles

1. **Legibility of state first.** At any moment the operator should know: what's running, what each
   agent is doing, and whether anything needs attention. Status is the hero.
2. **One glance, many sessions.** Scale gracefully from 1 to 10+ concurrent sessions (SC-006) without
   losing per-session clarity.
3. **Terminal is first-class, not an afterthought.** The embedded terminal is a primary work surface;
   it must feel like a real terminal (monospace, dark-friendly, fast, copy/paste, resize).
4. **Calm under failure.** Sandboxes fail, tunnels drop, agents stall. Abnormal states must be obvious
   but not alarming — clear reasons, clear next actions, never silent.
5. **Safety is visible.** Isolation, local-first networking, and "secrets are not shown" are part of the
   product's trust story; surface them rather than hiding them.
6. **Surface parity.** The web UI and the native desktop GUI present the **same capabilities and the same
   visual language** (FR-007a, SC-011); design once, adapt to platform conventions.

## 4. Surfaces & platforms

- **Native desktop GUI** — macOS and Linux, **genuinely native** (no Electron, no webview shell)
  (FR-007b). Design should respect platform conventions (window chrome, menus, traffic-light vs. native
  Linux controls) while keeping the Daedalus visual language consistent.
- **Web UI** — same screens and capabilities, delivered in a browser.
- **Shared visual language**: deliver a **surface-neutral design system** (tokens, components, layouts)
  that both surfaces implement. The desktop embedded terminal is a native terminal view; the web
  embedded terminal is zellij's web service — visually they should match (FR-008).
- **Responsive range**: optimize for desktop-class widths. Define behavior from ~1024px up to ultrawide;
  graceful degradation below is nice-to-have, not required.
- **Theming**: **dark and light themes both required** (operators live in terminals; dark is likely the
  default). The terminal and task board must be legible in both.

## 5. Information architecture

Top-level navigation (persistent), roughly in priority order:

1. **Sessions / Fleet** — the home: all sessions (active + recent) across sources and backends.
2. **Discover** — sessions found on local host, mDNS hosts, and tunneled Workshops; connect to any.
3. **Start session** — the create flow (can also be a prominent action on Fleet).
4. **Tools** — registry of operator-defined agentic tools.
5. **Environments / Backends** — configured backends (Workshop, macOS sandbox), their availability,
   and existing environments.
6. **Settings** — tunnels, theme, notification preferences, concurrency limit.

A **session detail** view is the deep surface the operator spends the most time in; everything else
funnels into it.

## 6. Screen-by-screen brief

For each screen: **purpose**, **key elements**, **primary actions**, and **states to design**.

### 6.1 App shell / global chrome
- **Purpose**: persistent navigation + global status.
- **Key elements**: nav (section 5); a global **status indicator** (count of running / stalled / failed
  sessions); **notifications** affordance (terminal/abnormal events, FR-021); active **backend
  availability** indicator; theme + identity of the local operator context.
- **States**: backend(s) available / degraded / unavailable; new-notification badge.

### 6.2 Sessions / Fleet (home)  — *US5, FR-025*
- **Purpose**: unified view of all sessions, active and recent, across sources/backends.
- **Key elements**: a list/grid of **Session Cards**, each showing: agentic tool, objective (short),
  source/host, backend, environment (fresh vs. pre-existing), **status badge**, brief progress (e.g.,
  tracked-task completion like "4/9"), runtime/age, resource snapshot. Filters (status, backend,
  source), sort, search. Prominent **"Start session"** action.
- **Primary actions**: open a session; quick stop; quick connect; start new.
- **States**: **empty** (no sessions yet — strong first-run call to action to start or discover);
  **loading**; **many sessions** (10+, density matters); a card in each status
  (starting/running/completed/failed/stalled/stopped); **disconnected/unknown** session (e.g., backend
  went away mid-session — clearly marked, not shown healthy, FR-028 / edge cases).

### 6.3 Start a session  — *US1, FR-001/001a/002/002a*
- **Purpose**: launch an agent into a sandbox.
- **Flow (a few clear steps)**:
  1. **Pick a registered agentic tool** (from Tools registry; show capabilities, e.g., "accepts
     interactive input").
  2. **Define the objective** following an **SDD convention (e.g., SpecKit)** — reference to spec/plan
     artifacts rather than a free-form prompt box (FR-001). Make clear the objective decomposes into
     tracked tasks.
  3. **Choose environment**: **fresh** (Daedalus provisions) **or pre-existing** (operator selects one).
     If pre-existing, surface the **git-worktree isolation** note — work happens in an isolated worktree
     so existing work isn't disturbed (FR-002a).
  4. **Choose backend / host** when more than one applies (Workshop Linux, macOS local sandbox, or a
     remote Workshop over a tunnel).
  5. **Confirm & launch** → transitions to Session detail in "starting" state.
- **States**: validation errors (no tool selected, objective missing); **provisioning failure** with a
  clear reason and no orphaned session (FR-005); concurrency limit reached (clear rejection, FR-026);
  selected pre-existing environment unreachable; worktree cannot be created (fail with reason).

### 6.4 Session detail  — *US2/US4, FR-006–FR-024*
The core working surface. Suggested layout: a **header** (identity + status + controls), a primary
**split** between the **embedded terminal** and the **task-status board**, plus a **telemetry rail**.

- **Header**: tool, objective, source/backend/environment, **status badge**, runtime; **controls**:
  **Stop**, **Send input** (only if the tool accepts it — otherwise disabled with explanation),
  **Confirm completion** (surfaced when the session is **awaiting confirmation** — a clean agent exit
  with tracked tasks unfinished; confirming marks it completed), **Clean up** (release environment)
  (FR-015a, FR-022–024). Note: **no pause/resume in v1** — don't design it.
- **Embedded terminal** (FR-008): live streaming output; real-terminal behaviors (scrollback, copy,
  selection, resize, "jump to latest"); an **input affordance** for send-input-capable agents; visually
  identical across desktop (native view) and web (zellij web service).
- **Task-status board** (FR-009/017): a board/kanban of the session's **Tracked Tasks**, each a card
  with description + **status**, grouped by status (e.g., To do / In progress / Done / Blocked). Status
  is **derived from the session's SDD artifacts (e.g., SpecKit `tasks.md`)** and updates live (SC-010).
  Show overall progress (e.g., 4/9).
- **Telemetry rail**: **resource usage** (CPU/memory/disk/time) (FR-019); lifecycle/event timeline
  (status changes, key actions); terminal outcome when ended.
- **States**: starting; running; **stalled** (no progress for an interval — visually distinct, suggests
  intervene); **awaiting confirmation** (agent run ended with tracked tasks unfinished — prompts the
  operator to confirm completion or clean up, FR-015a); completed; failed (show exit reason);
  stopped-by-operator; **connection lost** to environment (last-known state preserved, clearly flagged,
  FR-020); **review mode** for an ended session (read persisted output, task history, outcome — FR-018,
  SC-007); excessive-output handling (terminal stays responsive, FR — edge case).

### 6.5 Discover  — *US3, FR-010–FR-014*
- **Purpose**: find and connect to sessions beyond the ones the operator started.
- **Key elements**: list grouped by **Source**: **local host**, **mDNS-advertised hosts**, **tunneled
  Workshops**. Each discovered session shows source, identity, status; a **Connect** action that
  attaches via zellij into the embedded terminal. **De-duplicate** sessions advertised from multiple
  sources to a single entry (FR-013).
- **States**: empty (nothing discovered — explain *why*, e.g., no hosts advertising, SDK not advertised
  in a Workshop, FR — edge cases); a source going away (host stops advertising / tunnel drops → its
  sessions marked **unreachable**, FR-014); a session **not attachable via zellij** (clear reason, not
  silent); loading/scanning.

### 6.6 Tools (agentic tool registry)  — *FR-001a*
- **Purpose**: register and manage the declarative tool definitions that become selectable at start.
- **Key elements**: list of registered tools (name, how it's invoked in a sandbox, capabilities such as
  interactive-input support); add/edit/remove a definition.
- **States**: empty (first-run: register your first tool); validation errors on a malformed definition.

### 6.7 Environments / Backends  — *FR-027/028, SC-013*
- **Purpose**: see configured backends and existing environments.
- **Key elements**: backend cards — **Canonical Workshop (Linux)** and **macOS-specific sandbox** —
  with **availability** status and capabilities/limits; list of existing environments (selectable when
  starting a session); indicate tunneled/remote Workshops on macOS.
- **States**: backend available / degraded / **unavailable** (clearly marked; other backends keep
  working, FR-028); no environments yet.

### 6.8 Settings
- **Purpose**: tunnels (authenticated remote reach), concurrency limit, notifications, theme, and the
  **local-first security posture** (make it visible that there is no open network listener by default —
  FR-032, SC-012).

## 7. Component inventory (design-system level)

- **Status badge** — the single most reused element. Define a clear, color-blind-safe visual for every
  session/task state: starting, running, **awaiting confirmation**, completed, failed, **stalled**,
  stopped, **disconnected/unknown**, unreachable. Stalled, failed, and awaiting-confirmation must be
  unmistakable.
- **Session card** — compact + expanded variants (Fleet vs. detail header).
- **Tracked-task card** — for the board; description + status + (optional) detail.
- **Task-status board** — column/kanban layout with live updates and progress summary.
- **Embedded terminal pane** — chrome, toolbar (copy, search, jump-to-latest, input), resize handles.
- **Telemetry/resource widgets** — small CPU/mem/disk/time meters and a sparkline/timeline.
- **Source chip** (local / mDNS / tunneled-Workshop) and **backend chip** (Workshop / macOS sandbox)
  with availability state.
- **Notification item** — terminal/abnormal events with a jump-to-session action.
- **Empty / error / loading** states as first-class, reusable patterns (see section 8).

## 8. Cross-cutting states to design (don't skip)

Design these as **named, reusable patterns** — they appear across many screens:
- **Empty** (first-run and per-screen): inviting, with the right primary action.
- **Loading / scanning**: especially Discover and Fleet.
- **Error / failure**: provisioning failed, worktree failed, attach failed — always with a **reason**
  and a **next action**; never a dead end.
- **Degraded / unavailable**: backend down, tunnel dropped, host stopped advertising — partial
  functionality continues, the unavailable part is clearly flagged (FR-028, FR-014).
- **Stalled / needs attention**: the operator's cue to intervene.
- **Disconnected / unknown**: last-known state preserved, explicitly *not* shown as healthy.

## 9. Key flows to storyboard

1. **First run → first session**: empty app → register a tool → start a session (fresh env) → land in
   Session detail watching it run.
2. **Monitor & intervene**: running session stalls → operator notices via status/notification → opens
   detail → sends input or stops.
3. **Discover & attach**: a session started elsewhere appears in Discover (e.g., tunneled Workshop) →
   connect → interact via embedded terminal.
4. **Run on a pre-existing environment**: start a session targeting an existing env → see the
   worktree-isolation reassurance → agent works without trampling existing work.
5. **Fleet at scale**: 10+ sessions across two backends → scan statuses → drill into the one that failed.
6. **Cross-platform**: same flow on macOS (macOS sandbox or remote Workshop) and Linux (Workshop).

## 10. Visual & interaction guidelines

- **Density**: information-dense but scannable; this is a power-user tool, not a consumer app.
- **Status-driven color**: a small, consistent, **color-blind-safe** status palette used everywhere;
  don't rely on color alone (pair with icon/label).
- **Typography**: clear UI type for chrome; **monospace** for terminal and for SDD task identifiers.
- **Motion**: subtle; use it to signal state changes (a task moving to Done, a status flip) without
  distracting from terminal output.
- **Keyboard-first**: operators expect shortcuts (switch sessions, focus terminal, stop, jump to
  latest). Note intended shortcuts in the design.

## 11. Accessibility & internationalization

- Meet **WCAG AA** contrast in both themes; status never conveyed by color alone.
- Full **keyboard navigation**; visible focus states; screen-reader labels for status and controls.
- Terminal content accessibility is inherently limited — at minimum ensure surrounding controls and
  status are accessible.
- Copy should be localizable (no hard-coded concatenation); v1 ships English.

## 12. Constraints & non-goals (design must respect)

- **Native desktop, no Electron / no webview shell** for the desktop GUI (FR-007b). Deliverable is a
  **surface-neutral design system**; don't assume web-only components on desktop.
- **No multi-user, no login** in v1 — don't design account/role/permission UI (FR-034).
- **No pause/resume** — controls are stop, send-input, clean-up only (FR-022).
- **Secrets are never shown**: the operator pre-provisions secrets in the environment; Daedalus does
  not manage or display them, and must not surface them in logs/terminal capture (FR-031). Don't design
  secret-entry UI.
- **Local-first, no open listener by default** (FR-032, SC-012): the security posture should be
  *visible and reassuring*, not a barrier.
- **Objective is SDD-convention-based** (e.g., SpecKit), not a free-form chat prompt — design the
  objective step accordingly (FR-001).

## 13. Glossary (use these terms consistently in the UI)

- **Session** — one run of an agentic tool against an objective inside an environment. (Avoid mixing
  "run"/"job"/"task" for this concept.)
- **Agentic tool** — a registered autonomous agent Daedalus can launch.
- **Objective** — the overall goal for a session, expressed via an SDD convention; decomposes into
  tracked tasks.
- **Tracked task** — one unit of work shown on the board, status sourced from SDD artifacts.
- **Environment** — the sandbox hosting a session (fresh or pre-existing).
- **Backend** — a provider of environments (Canonical Workshop; macOS sandbox).
- **Source / host** — where a session is discovered (local, mDNS host, tunneled Workshop).
- **Operator** — the single user.

## 14. Deliverables requested from the design effort

1. A **design system / component library**: tokens (color incl. status palette, type, spacing),
   dark + light themes, and the components in section 7.
2. **High-fidelity screens** for: Fleet/home, Start-session flow, Session detail (the priority screen),
   Discover, Tools, Environments/Backends, Settings.
3. **All the cross-cutting states** in section 8 for the major screens (empty/loading/error/degraded/
   stalled/disconnected).
4. **Storyboards** for the six flows in section 9.
5. **Responsive/desktop layout notes** and **platform-adaptation notes** (macOS vs. Linux vs. web)
   while preserving one visual language.
6. **Annotations** mapping key screens/components back to the spec's requirements where useful.

> Priority order if scope must be cut: **Session detail** → **Fleet/home** → **Start-session** →
> **Discover** → the rest. Session detail is where operators live.
