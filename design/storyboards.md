# Daedalus — Storyboards for the six key flows

Storyboards for the flows in design-brief §9, satisfying deliverable §14.4. Each step names the screen,
the state it is in, what the operator sees and does, and **how to reproduce that state in the prototype**
(Tweaks settings, demo buttons, or session IDs). `FR-xxx` / `SC-xxx` annotations point back to
[`specs/001-agent-orchestration/spec.md`](../specs/001-agent-orchestration/spec.md) via the
[design brief](../specs/001-agent-orchestration/design-brief.md).

## Running the prototype

```bash
cd design/prototype
python3 -m http.server
```

Open `http://localhost:8000/Daedalus%20Prototype.html` ("Daedalus Prototype.html"). The floating
**Tweaks** panel (right edge) switches theme, skin, nav, density, layouts, and — under **Screen
states** — the demo states used below. Tweaks is a design-exploration tool, not a product feature.
Session IDs (`s-901`…`s-911`) refer to the seeded mock data in `data.js`; three tweak buttons jump
straight to key sessions: "Open a stalled session →" (`s-902`), "Answer a waiting session →" (`s-909`),
"Review a session to confirm →" (`s-911`).

---

## Flow 1 — First run → first session

*Empty app → register a tool → start a session (fresh env) → land in Session detail watching it run.*

1. **Fleet, empty state** — Tweaks → Fleet layout → *Fleet state = "Empty (first run)"*, then open
   **Sessions** in the nav. The operator sees "No sessions yet" with two primary actions: **Start a
   session** and **Discover**. *(brief §6.2 empty state)*
2. **Tools, first-run empty state** — Tweaks → Screen states → *"Tools empty (first run)"* on (this
   jumps to the Tools screen). "Register your first agentic tool" explains what a tool definition is;
   single action **Register tool**. *(FR-001a)*
3. **Register tool modal** — the operator fills in Name, Launch command (run in the feature worktree),
   Version, and the "Accepts interactive input" toggle; **Validate definition** dry-checks the command
   against the sandbox PATH. On success the tool appears in the registry list. *(FR-001a; validation
   errors are Flow-1's error branch — see the state matrix)*
4. **Start a session** — ⌘N or the **Start session** button. Step 1 *Objective*: pick a repository
   (e.g. `acme/gateway`) and an existing spec branch (`specs/053-webhook-retry` — "spec.md · plan.md ·
   tasks.md", 9 tracked tasks), or describe a new objective (the form previews the worktree + `specify`
   plan). Step 2 *Environment & agentic tool*: **Fresh** environment, host "This host", backend type
   Workshop, then pick the registered tool card. Footer reads "Ready to launch". *(FR-001, FR-002)*
5. **Review & launch modal** — summary rows (Tool / Repository / Objective / Environment) plus the
   trust banner: "Secrets are pre-provisioned in the environment — Daedalus never displays or captures
   them." **Launch session**. *(FR-031)*
6. **Session detail, starting** — lands on `s-905` ("Build webhook retry queue") in the **starting**
   state: blue spinner badge, "Provisioning environment…" banner, and an authentic transcript — Workshop
   environment `ws-4187` created, repo cloned, worktree `agent/053-webhook-retry` mounted, the Daedalus
   SDK registered inside the sandbox ("advertising on localhost"), tool launched under zellij, waiting
   for the agent handshake. The board shows T001 "Provision environment" in progress; the telemetry rail
   timeline begins. *(US1; FR-003/004; SC-001)*

### Flow 1b — Confirm-completion coda (FR-015a)

Later, a run can end cleanly with tracked tasks unfinished. Tweaks → *"Review a session to confirm →"*
opens `s-911` (status **confirm**, "Awaiting confirmation", purple clipboard-check badge): the banner
quotes the agent's summary ("Finished 7 of 9 tasks…"), shows "exited cleanly (code 0) · 7/9 tasks", and
offers **Confirm completion** / **Clean up**. The board shows T008/T009 blocked ("Skipped by agent —
needs a production backfill-window decision"); the transcript ends "run ended with 2 tracked tasks
unconfirmed — confirm completion or clean up"; the rail's Outcome block records the clean exit. Flow 5
step 5 revisits this from the fleet.

---

## Flow 2 — Monitor & intervene

*A running session needs the operator — a stall, or a question. Both paths surface through status,
notifications, and the Needs-you queue.* *(US2/US4; FR-016, FR-021)*

### 2a — Stalled path (`s-902`)

1. **Anywhere** — the titlebar GlobalStatus shows an amber stalled count; the notifications bell badges
   and lists "Session stalled — Migrate billing service — no progress for 6m" with a jump-to-session
   action. On the Tasks home, T003's card carries the detail "Waiting on test fixture — no output for
   6m." and an attention flag. *(FR-021)*
2. **Needs you** — the queue lists `s-902` with cue "No output — may be stuck", the blocked-task reason,
   time waiting, and a live idle-cost readout (sandbox $/hr while blocked).
3. **Session detail, stalled** — open via Tweaks *"Open a stalled session →"* (or any of the above).
   Amber stalled banner: "No progress for 6m. The agent may be stuck waiting on a fixture or input."
   with **Send input** and **Stop** actions. The terminal shows the fixture wait ("⚠ fixture
   'postgres-async' not ready after 360s … (no output for 6m 12s)"); the timeline's last node is
   "No output — possible stall". *(FR-016, FR-022/023)*
4. **Intervene** — the operator types into the terminal input ("Send input to the focused pane…") or
   presses **Stop**. *(FR-022/023; no pause/resume — FR-022)*

### 2b — Awaiting-input path (`s-909`, via Needs you)

1. **Anywhere** — the titlebar shows the purple "waiting on you" count; the notification reads "Waiting
   for your input — question about the `sessions` FK scope".
2. **Needs you** — `s-909` sits at the top (awaiting entries sort first, longest wait first): purple
   speech-bubble glyph, cue "Asked you a question", the quoted prompt ("spec.md doesn't cover the
   `sessions` table foreign key — migrate it in this pass too, or leave it for a follow-up?"), waiting
   readout "8m", idle-cost readout, and the **Answer in terminal** action. Try all three placements via
   Tweaks → Needs you → *Surface* (dedicated screen / header tray / strip on Tasks home).
3. **Session detail, answer mode** — "Answer in terminal" (or Tweaks *"Answer a waiting session →"*)
   opens `s-909` terminal-primary, scrolled to the agent's prompt line ("? Migrate `sessions.user_id`
   in this pass too…"), with the input focused and placeholder "Reply to Claude Code…  (↵ to send)".
   The purple awaiting banner quotes the question and shows the waiting time. *(FR-015 input; the
   `awaiting` status is new — see README palette note)*
4. Variant: `s-910` is the **approval** flavour — "About to run a DESTRUCTIVE reset on staging to
   re-seed. Approve? [y/N]".

---

## Flow 3 — Discover & attach

*A session started elsewhere appears in Discover → connect → interact via the embedded terminal.*
*(US3; FR-010–014)*

1. **Discover** (top-level nav, radar icon) — sessions grouped by source: **Local host**,
   **mDNS-advertised hosts** (`studio-linux.local`), **Tunneled Workshops** (`eu-fra-1`,
   `us-east-2`), each row with status badge, tool, objective, host, and a source chip. *(FR-010/011/012)*
2. **De-duplication note** — the footer reads "1 session advertised from multiple sources was
   de-duplicated" (the local advertisement of `s-901` is folded into its existing entry). *(FR-013)*
3. **Not attachable** — the completed session on `us-east-2` shows "Review only" with the reason on
   hover: "Session ended — not attachable via zellij. Open in review mode instead." *(brief §6.5 —
   never silent)*
4. **Connect** — on an attachable tunnel row attaches via zellij and opens the session detail (the
   prototype shortcuts every Connect to `s-906`): the transcript header reads "attached to session
   s-906 (zellij · tunnel eu-fra-1)" and the terminal input passes keystrokes through. `s-906` also
   demonstrates the excessive-output state: a pinned "Output trimmed — showing last 10,000 of 1,834,211
   lines · full log persisted" notice keeps the terminal responsive. *(FR-012, FR-008; excessive-output
   edge case)*

Other Discover states (Tweaks → Screen states → *Discover state*): **Scanning** (spinners per group, or
click Rescan), **Empty** (explains *why* nothing was found — mDNS reach, SDK not registered in a
Workshop, tunnels disconnected — with Rescan / Set up a tunnel actions), and **Source dropped
(tunnel)** — the tunnel group is flagged "tunnel eu-fra-1 dropped 2m ago" with a Reconnect action, its
rows turn **unreachable** ("Last seen before the tunnel dropped — kept listed until it reconnects"),
and Connect is disabled with a reason. *(FR-014)*

---

## Flow 4 — Run on a pre-existing environment

*Start a session targeting an existing env → see the worktree-isolation reassurance → the agent works
without trampling existing work.* *(US1; FR-002a)*

1. **Start a session → Environment: Pre-existing** — the environment list shows existing repos
   (`acme/monorepo`, `acme/web-app`, `acme/infra`) with backend type, host, and branch.
2. **Worktree reassurance** — an info banner below the picker: "Work happens in an **isolated git
   worktree** — your existing checkout and uncommitted work are never disturbed." *(FR-002a)*
3. **Pick tool, review, launch** — as Flow 1 steps 4–5.
4. **The session in flight** — `s-901` ("Add OAuth device-flow to gateway") is the seeded example:
   running on existing env `acme/monorepo`, its header chips include the env name and the shield chip
   **worktree-isolated**; the transcript shows edits landing on the agent's worktree branch.

Failure branches (Tweaks → Screen states → *Start flow state*):

- **Environment unreachable** — `acme/infra` renders unreachable ("host eu-fra-1 not responding since
  14:02"), unselectable, with a FailureNotice: reason plus next actions (Rescan / Pick another
  environment / Use a fresh environment) and the reassurance "Everything else here still works."
- **Worktree creation failed** — FailureNotice with the raw reason (`git worktree add failed: branch
  'feature/auth' is already checked out at /work/repo`), actions (Try another branch / Use a fresh
  environment), and the note "Your existing checkout and uncommitted work were not touched — the
  worktree never attached." *(brief §6.3)*

---

## Flow 5 — Fleet at scale

*10+ sessions across backends → scan statuses → drill into the one that failed.* *(US5; FR-025; SC-006)*

1. **Sessions (Fleet)** — 11 seeded sessions across three hosts/backends (local Workshop, tunneled
   Workshop `eu-fra-1`, macOS sandbox on `studio-mac.local`). Default sort is **Attention**: awaiting →
   confirm → stalled → failed → disconnected → starting → running → ended. Rows carry status badge,
   objective, spec branch, tool, backend (availability dot only when its host is degraded/down),
   progress (e.g. "4/9"), age, and an inline **NeedsTag** on anything blocked on the operator. Try
   Tweaks → Density = compact and Fleet layout = Table for the densest scan; status filter chips and
   search narrow the list. *(FR-025, SC-006)*
2. **Scan** — one glance separates the two purple attention states (`s-909`/`s-910` waiting for input,
   `s-911` awaiting confirmation), the amber stall (`s-902`), the red failure (`s-904`), and the grey
   disconnect (`s-908`, explicitly *not* shown healthy — "Tunnel eu-fra-1 dropped — last-known state
   preserved"). *(FR-020, FR-028)*
3. **Drill into the failure** — open `s-904` ("Refactor auth module to remove deadlocks"): red failed
   banner with the exit reason ("Exited (1) — test suite failed: 3 deadlock tests still failing"),
   the failing tests in the persisted transcript, T004 blocked on the board, and the rail's Outcome
   block (State: Failed · Exit: code 1 · output persisted, readable in review mode). Actions: **Start
   similar** / **Clean up**. *(FR-018, SC-007)*
4. **Review-mode neighbours** — `s-903` (completed) and `s-907` (stopped by operator, SIGTERM) show the
   ended-session treatment: read-only terminal ("persisted output"), review-mode input notice, Clean-up
   action. *(FR-018, FR-024)*
5. **Confirm the near-miss** — `s-911` from Flow 1b appears in the attention band; **Review & confirm**
   from the Needs-you queue or open it directly, then **Confirm completion** (marks it completed) or
   **Clean up** (releases the environment). *(FR-015a, FR-024)*

---

## Flow 6 — Cross-platform

*The same flow on macOS (macOS sandbox or remote Workshop) and Linux (Workshop), one visual language.*
*(FR-007b; SC-011)*

1. **Linux / Yaru** — Tweaks → Platform skin = Linux: Ubuntu font, Ubuntu-orange accent, GNOME status
   palette, squared right-side window controls. Any flow above runs against the local Workshop backend.
2. **macOS** — Platform skin = macOS: SF font, warm-amber accent, left traffic-lights. The macOS-side
   sessions are seeded: `s-904`/`s-907`/`s-910` run in the macOS sandbox on `studio-mac.local`; remote
   Workshops are reached over the authenticated tunnel (`s-902`, `s-906`, `s-908`, `s-909` on
   `eu-fra-1`). *(FR-027)*
3. **Hosts availability, both platforms** — sidebar Hosts list, or in top-bar mode the **HostsIndicator**
   pill (one dot per host, worst first; popover lists each host with kind, availability, and the note
   "Availability from local checks, mDNS presence, and tunnel state"). `studio-mac.local` is seeded
   degraded ("High memory pressure — new environments may be slow"). *(brief §6.1, FR-028)*
4. **Theme parity** — Theme = System / Light / Dark (System follows the OS); status palette, terminal,
   and board stay legible in both themes on both skins. *(brief §4)*
5. **Trust story** — Settings shows the local-first banner ("no open network listener by default") and
   per-tunnel state; Add tunnel explains "Outbound only … the token is never displayed or stored".
   *(FR-032, SC-012)*

---

## Appendix — state matrix

Every cross-cutting state (brief §8) × where to see it. "Tweak" = Tweaks panel setting; sessions are
opened from any list or the tweak jump buttons.

| State | Where | Reproduce |
|-------|-------|-----------|
| Empty — Fleet first run | Sessions | Tweak: Fleet state = "Empty (first run)" |
| Empty — Tools first run | Tools | Tweak: "Tools empty (first run)" |
| Empty — nothing discovered (with reasons) | Discover | Tweak: Discover state = "Empty" |
| Empty — no environments | Environments | Tweak: Backends state = "No environments" |
| Empty — Needs-you caught up | Needs you | designed (`NeedsEmpty`, "You're all caught up") but not reachable with the seeded data |
| Loading — skeleton cards | Sessions | Tweak: Fleet state = "Loading" |
| Scanning | Discover | Tweak: Discover state = "Scanning", or the Rescan button |
| Validation errors (form) | Start session | Tweak: Start flow state = "Validation errors" (field errors + footer hint) |
| Validation errors (tool definition) | Tools → Register modal | blur empty/duplicate Name; "Validate definition" with an empty command seeds the `claude--dangerously` typo → mono "command not found in sandbox PATH" (FR-001a) |
| Provisioning failed | Start session | Tweak: Start flow state = "Provisioning failed" — in-form panel, and the same failure inside the Review & launch modal on Launch; "No session was created — nothing to clean up" (FR-005) |
| Concurrency limit reached | Start session | Tweak: Start flow state = "Concurrency limit reached" — warn notice, Launch disabled (FR-026) |
| Pre-existing env unreachable | Start session | Tweak: Start flow state = "Environment unreachable" (FR-002/§6.3) |
| Worktree creation failed | Start session | Tweak: Start flow state = "Worktree creation failed" (FR-002a) |
| Stalled | Session detail / Needs you / notifications | session `s-902` (FR-016) |
| Waiting for input (`awaiting`) | Session detail / Needs you | sessions `s-909` (question), `s-910` (approval) |
| Awaiting confirmation (`confirm`) | Session detail / Needs you | session `s-911` (FR-015a) |
| Failed (reason + next action) | Session detail | session `s-904` |
| Stopped by operator | Session detail | session `s-907` |
| Completed / review mode | Session detail | session `s-903` (FR-018, SC-007) |
| Disconnected / unknown (last-known preserved) | Session detail / Fleet | session `s-908` (FR-020) |
| Excessive output (trimmed scrollback) | Session detail terminal | session `s-906` |
| Source dropped → unreachable rows | Discover | Tweak: Discover state = "Source dropped (tunnel)" (FR-014) |
| De-duplicated discovery | Discover | Discover state = "Normal" — dedup footnote (FR-013) |
| Host down, others keep working | Environments | Tweak: Backends state = "Host down (tunnel)" (FR-028) |
| Degraded host | Environments / Hosts list / chips | seeded `studio-mac.local` note; availability dots on chips appear only when not "available" |
| Send-input unavailable (headless tool) | Session detail | any session whose tool has `interactive: false` — the input row explains "does not accept interactive input" (none seeded; toggle a tool's capability in `data.js` to preview) |
| No open listener (security posture) | Settings | Settings modal banner + Add-tunnel "Outbound only" note (FR-032, SC-012) |
