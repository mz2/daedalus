/* DAEDALUS — mock data. Surface-neutral; consumed by all screens. */
(function () {
  // ── Agentic tools (registry) ──────────────────────────────────────────────
  const TOOLS = [
    { id: "claude-code", name: "Claude Code", invoke: 'claude -p "/speckit.implement"', interactive: true, speckit: true, track: "tasks.md + output",
      desc: "Anthropic's coding agent. Runs the SpecKit implement pass and prints each task as it works through tasks.md.", version: "2.4.1",
      mono: "C", tint: "oklch(0.66 0.045 45)" },
    { id: "antigravity", name: "Antigravity", invoke: 'agy -p "/speckit.implement"', interactive: true, speckit: true, track: "tasks.md + output",
      desc: "Google's agy harness (Gemini 3). Plans, then implements the SpecKit task graph.", version: "1.0.5",
      mono: "A", tint: "oklch(0.64 0.045 250)" },
    { id: "opencode", name: "OpenCode", invoke: 'opencode run "/speckit.implement"', interactive: true, speckit: true, track: "tasks.md + output",
      desc: "Open-source terminal agent. Steps through the SpecKit tasks.md sequentially.", version: "0.6.2",
      mono: "O", tint: "oklch(0.64 0.045 155)" },
  ];

  // ── Backends ────────────────────────────────────────────────────────────────
  const BACKENDS = [
    { id: "workshop", name: "Canonical Workshop", platform: "Linux", availability: "available",
      desc: "Isolated Linux sandboxes. Fresh or pre-existing environments.", limit: "8 concurrent · 4 vCPU / 8 GB each", icon: "linux" },
    { id: "macos", name: "macOS Sandbox", platform: "macOS", availability: "degraded",
      desc: "Local sandbox-exec environment on this host.", limit: "2 concurrent · shares host resources", icon: "apple",
      note: "High memory pressure — provisioning may be slow." },
    { id: "workshop-remote", name: "Workshop (tunnel: eu-fra-1)", platform: "Linux", availability: "available",
      desc: "Remote Workshop reached over an authenticated tunnel.", limit: "16 concurrent · 8 vCPU / 16 GB each", icon: "tunnel" },
    { id: "openshell", name: "NVIDIA OpenShell", platform: "Linux · GPU", availability: "available",
      desc: "GPU-accelerated sandboxes for agents that build, test, or run ML and CUDA workloads.", limit: "4 concurrent · 1×L4 GPU / 16 vCPU each", icon: "gpu" },
  ];

  // ── Environments ──────────────────────────────────────────────────────────
  const ENVIRONMENTS = [
    { id: "env-fresh", name: "(fresh environment)", backend: "workshop", kind: "fresh" },
    { id: "env-monorepo", name: "acme/monorepo", backend: "workshop", kind: "existing", branch: "main", note: "worktree-isolated" },
    { id: "env-web", name: "acme/web-app", backend: "macos", kind: "existing", branch: "develop", note: "worktree-isolated" },
    { id: "env-infra", name: "acme/infra", backend: "workshop-remote", kind: "existing", branch: "main", note: "worktree-isolated" },
  ];

  // ── Tracked-task templates ──────────────────────────────────────────────────
  const tasks = (arr) => arr.map((t, i) => ({ id: t[0], title: t[1], status: t[2], detail: t[3] || null }));

  // ── Sessions ────────────────────────────────────────────────────────────────
  const SESSIONS = [
    {
      id: "s-901", tool: "claude-code", toolName: "Claude Code",
      objective: "Add OAuth device-flow to gateway",
      issue: { tracker: "github", ref: "acme/gateway#412" },
      spec: "specs/044-device-flow", backend: "workshop", backendName: "Workshop",
      env: "env-monorepo", envName: "acme/monorepo", envKind: "existing",
      source: "local", status: "running", started: 1380, lastEvent: 4,
      cpu: 47, mem: 62, disk: 18, net: 12,
      tasks: tasks([
        ["T001", "Read spec & plan artifacts", "done"],
        ["T002", "Scaffold device-flow endpoints", "done"],
        ["T003", "Token polling + rate-limit", "done"],
        ["T004", "Wire gateway middleware", "done"],
        ["T005", "Persist device codes (redis)", "doing"],
        ["T006", "Expiry & cleanup job", "todo"],
        ["T007", "Contract tests", "todo"],
        ["T008", "Integration test: full flow", "todo"],
        ["T009", "Update OpenAPI + docs", "todo"],
      ]),
    },
    {
      id: "s-902", tool: "opencode", toolName: "OpenCode",
      objective: "Migrate billing service to async DB driver",
      issue: { tracker: "jira", ref: "BILL-204" },
      spec: "specs/039-async-billing", backend: "workshop-remote", backendName: "Workshop · eu-fra-1",
      env: "env-infra", envName: "acme/infra", envKind: "existing",
      source: "tunnel", status: "stalled", started: 2640, lastEvent: 372,
      cpu: 3, mem: 41, disk: 22, net: 0,
      tasks: tasks([
        ["T001", "Audit sync DB call-sites", "done"],
        ["T002", "Introduce async pool", "done"],
        ["T003", "Migrate billing repository", "doing", "Waiting on test fixture — no output for 6m."],
        ["T004", "Migrate invoice repository", "todo"],
        ["T005", "Backfill migration script", "todo"],
        ["T006", "Load test under async", "blocked", "Blocked by T003."],
      ]),
    },
    {
      id: "s-903", tool: "antigravity", toolName: "Antigravity",
      objective: "Generate API client SDKs from OpenAPI",
      spec: "specs/051-sdk-gen", backend: "workshop", backendName: "Workshop",
      env: "env-fresh", envName: "fresh", envKind: "fresh",
      source: "local", status: "completed", started: 540, lastEvent: 0, ended: 60,
      cpu: 0, mem: 0, disk: 9, net: 0, exit: "Completed — 5 SDKs generated, all checks passed.",
      tasks: tasks([
        ["T001", "Parse OpenAPI 3.1 spec", "done"],
        ["T002", "Generate TypeScript client", "done"],
        ["T003", "Generate Python client", "done"],
        ["T004", "Generate Go client", "done"],
        ["T005", "Publish to internal registry", "done"],
      ]),
    },
    {
      id: "s-904", tool: "opencode", toolName: "OpenCode",
      objective: "Refactor auth module to remove deadlocks",
      issue: { tracker: "jira", ref: "AUTH-77" },
      spec: "specs/047-auth-locks", backend: "macos", backendName: "macOS Sandbox",
      env: "env-web", envName: "acme/web-app", envKind: "existing",
      source: "local", status: "failed", started: 900, lastEvent: 120, ended: 120,
      cpu: 0, mem: 0, disk: 14, net: 0, exit: "Exited (1) — test suite failed: 3 deadlock tests still failing.",
      tasks: tasks([
        ["T001", "Map lock acquisition order", "done"],
        ["T002", "Introduce lock hierarchy", "done"],
        ["T003", "Refactor session store", "done"],
        ["T004", "Regression tests", "blocked", "3 tests failing — run aborted."],
      ]),
    },
    {
      id: "s-905", tool: "claude-code", toolName: "Claude Code",
      objective: "Build webhook retry queue",
      spec: "specs/053-webhook-retry", backend: "workshop", backendName: "Workshop",
      env: "env-fresh", envName: "fresh", envKind: "fresh",
      source: "local", status: "starting", started: 12, lastEvent: 2,
      cpu: 8, mem: 14, disk: 2, net: 30,
      tasks: tasks([
        ["T001", "Provision environment", "doing"],
        ["T002", "Read spec & plan", "todo"],
        ["T003", "Design queue schema", "todo"],
        ["T004", "Implement retry worker", "todo"],
      ]),
    },
    {
      id: "s-906", tool: "antigravity", toolName: "Antigravity",
      objective: "Add rate-limiting to public API",
      issue: { tracker: "github", ref: "acme/api#88" },
      spec: "specs/048-ratelimit", backend: "workshop-remote", backendName: "Workshop · eu-fra-1",
      env: "env-infra", envName: "acme/infra", envKind: "existing",
      source: "tunnel", status: "running", started: 420, lastEvent: 8,
      cpu: 71, mem: 55, disk: 11, net: 22,
      trimmed: { shown: 10000, total: 1834211 }, // excessive output — terminal shows a trimmed scrollback

      tasks: tasks([
        ["T001", "Choose token-bucket strategy", "done"],
        ["T002", "Middleware implementation", "doing"],
        ["T003", "Per-key quota config", "todo"],
        ["T004", "429 responses + headers", "todo"],
        ["T005", "Tests", "todo"],
      ]),
    },
    {
      id: "s-907", tool: "claude-code", toolName: "Claude Code",
      objective: "Document internal event schema",
      spec: "specs/050-event-docs", backend: "macos", backendName: "macOS Sandbox",
      env: "env-web", envName: "acme/web-app", envKind: "existing",
      source: "local", status: "stopped", started: 300, lastEvent: 200, ended: 200,
      cpu: 0, mem: 0, disk: 6, net: 0, exit: "Stopped by operator.",
      tasks: tasks([
        ["T001", "Crawl event emitters", "done"],
        ["T002", "Build schema registry", "done"],
        ["T003", "Generate markdown docs", "todo"],
      ]),
    },
    {
      id: "s-908", tool: "opencode", toolName: "OpenCode",
      objective: "Nightly dependency upgrade sweep",
      spec: "specs/021-dep-sweep", backend: "workshop-remote", backendName: "Workshop · eu-fra-1",
      env: "env-infra", envName: "acme/infra", envKind: "existing",
      source: "tunnel", status: "unknown", started: 1800, lastEvent: 900,
      cpu: null, mem: null, disk: null, net: null,
      lastKnown: "running", note: "Tunnel eu-fra-1 dropped — last-known state preserved.",
      tasks: tasks([
        ["T001", "Resolve dependency graph", "done"],
        ["T002", "Bump minor versions", "doing"],
        ["T003", "Run test matrix", "todo"],
      ]),
    },
    {
      id: "s-909", tool: "claude-code", toolName: "Claude Code",
      objective: "Migrate users table to UUID primary keys",
      issue: { tracker: "jira", ref: "DATA-318" },
      spec: "specs/056-uuid-pks", backend: "workshop-remote", backendName: "Workshop · eu-fra-1",
      env: "env-infra", envName: "acme/infra", envKind: "existing",
      source: "tunnel", status: "awaiting", started: 960, lastEvent: 500, waitingFor: 500,
      cpu: 2, mem: 38, disk: 16, net: 0,
      prompt: "spec.md doesn’t cover the `sessions` table foreign key — migrate it in this pass too, or leave it for a follow-up?",
      tasks: tasks([
        ["T001", "Add uuid columns", "done"],
        ["T002", "Backfill uuids (1.2M rows)", "done"],
        ["T003", "Swap primary keys", "doing", "Paused — waiting for your answer on FK scope."],
        ["T004", "Update foreign keys", "todo"],
        ["T005", "Drop legacy int ids", "todo"],
      ]),
    },
    {
      id: "s-910", tool: "antigravity", toolName: "Antigravity",
      objective: "Add device-flow auth to the mobile API",
      issue: { tracker: "github", ref: "acme/mobile#212" },
      spec: "specs/057-mobile-deviceflow", backend: "macos", backendName: "macOS Sandbox",
      env: "env-web", envName: "acme/web-app", envKind: "existing",
      source: "local", status: "awaiting", started: 300, lastEvent: 140, waitingFor: 140,
      cpu: 5, mem: 24, disk: 7, net: 0,
      prompt: "About to run a destructive reset on the staging DB to re-seed fixtures — approve?",
      tasks: tasks([
        ["T001", "Scaffold device-flow endpoints", "done"],
        ["T002", "Token exchange handler", "doing", "Paused — needs approval to reset the staging DB."],
        ["T003", "Rate-limit the poll endpoint", "todo"],
        ["T004", "Integration tests", "todo"],
      ]),
    },
    {
      id: "s-911", tool: "claude-code", toolName: "Claude Code",
      objective: "Refactor billing reconciliation into idempotent jobs",
      issue: { tracker: "jira", ref: "BILL-231" },
      spec: "specs/058-idempotent-recon", backend: "workshop", backendName: "Workshop",
      env: "env-monorepo", envName: "acme/monorepo", envKind: "existing",
      source: "local", status: "confirm", started: 5460, lastEvent: 720, ended: 720, waitingFor: 720,
      cpu: 0, mem: 0, disk: 19, net: 0, exitCode: 0,
      exit: "Exited (0) — agent finished; 2 tracked tasks unconfirmed.",
      summary: "Finished 7 of 9 tasks — reconciliation jobs are idempotent and the replay suite is green. Skipped the two production cut-over migrations pending a decision on the backfill window.",
      tasks: tasks([
        ["T001", "Map reconciliation call-sites", "done"],
        ["T002", "Extract job units with idempotency keys", "done"],
        ["T003", "Dedupe ledger writes", "done"],
        ["T004", "Replay-safe cursor checkpoints", "done"],
        ["T005", "Retry / backoff policy", "done"],
        ["T006", "Replay tests (idempotency suite)", "done"],
        ["T007", "Docs + runbook update", "done"],
        ["T008", "Migration: backfill idempotency keys (prod)", "blocked", "Skipped by agent — needs a production backfill-window decision."],
        ["T009", "Migration: enable dedupe constraint (prod)", "blocked", "Skipped by agent — depends on the T008 backfill."],
      ]),
    },
  ];

  // discovered (not started by operator)
  const DISCOVERED = [
    { id: "d-201", group: "mdns", host: "studio-linux.local", tool: "Claude Code",
      objective: "Implement search indexer", status: "running", attachable: true },
    { id: "d-202", group: "mdns", host: "studio-linux.local", tool: "OpenCode",
      objective: "Fix flaky e2e tests", status: "stalled", attachable: true },
    { id: "d-203", group: "tunnel", host: "Workshop · eu-fra-1", tool: "Antigravity",
      objective: "Backfill analytics events", status: "running", attachable: true },
    { id: "d-206", group: "tunnel", host: "Workshop · eu-fra-1", tool: "Claude Code",
      objective: "Rotate service signing keys", status: "running", attachable: true },
    { id: "d-204", group: "tunnel", host: "Workshop · us-east-2", tool: "OpenCode",
      objective: "Regenerate API docs", status: "completed", attachable: false,
      reason: "Session ended — not attachable via zellij. Open in review mode instead." },
    { id: "d-205", group: "local", host: "this host", tool: "Claude Code",
      objective: "Add OAuth device-flow to gateway", status: "running", attachable: true, dupOf: "s-901" },
  ];

  // notifications
  const NOTIFS = [
    { id: "n7", kind: "confirm", session: "s-911", title: "Awaiting confirmation", body: "Claude Code finished with 2 tasks unconfirmed — review & confirm.", t: 12 },
    { id: "n5", kind: "awaiting", session: "s-909", title: "Waiting for your input", body: "Migrate users to UUID PKs — question about the `sessions` FK scope.", t: 8 },
    { id: "n6", kind: "awaiting", session: "s-910", title: "Waiting for your approval", body: "Mobile device-flow — approve a destructive staging DB reset?", t: 2 },
    { id: "n1", kind: "stalled", session: "s-902", title: "Session stalled", body: "Migrate billing service — no progress for 6m.", t: 5 },
    { id: "n2", kind: "failed", session: "s-904", title: "Session failed", body: "Refactor auth module — 3 deadlock tests failing.", t: 14 },
    { id: "n3", kind: "unknown", session: "s-908", title: "Connection lost", body: "Tunnel eu-fra-1 dropped. Last-known state preserved.", t: 22 },
    { id: "n4", kind: "completed", session: "s-903", title: "Session completed", body: "Generate API client SDKs — all checks passed.", t: 41 },
  ];

  // Terminal snapshot lines: [class, text]. class ∈ '', dim, ok, warn, err, accent, prompt, cmd
  const TERMINAL = {
    "s-901": [
      ["dim", "$ claude -p \"/speckit.implement\""],
      ["dim", "daedalus: attached to session s-901 (zellij)"],
      ["", "Reading specs/044-device-flow/spec.md … 142 lines"],
      ["", "Reading specs/044-device-flow/plan.md … 86 lines"],
      ["", "Reading specs/044-device-flow/tasks.md … 9 tasks"],
      ["accent", "▸ T004  Wire gateway middleware"],
      ["", "  edit  gateway/middleware/device_flow.go (+118 −4)"],
      ["", "  edit  gateway/router.go (+12)"],
      ["ok", "  ✓ go build ./gateway/… — ok (3.1s)"],
      ["ok", "  ✓ T004 complete"],
      ["accent", "▸ T005  Persist device codes (redis)"],
      ["", "  edit  store/redis/device_codes.go (+64)"],
      ["", "  run   redis-cli PING"],
      ["ok", "  PONG"],
      ["", "  test  store/redis/device_codes_test.go"],
      ["dim", "  → running 8 tests …"],
      ["ok", "  ok   store/redis  0.42s  (7 passed)"],
      ["warn", "  ⚠ device_codes_test.go:88 — TTL assertion off by 1s, retrying"],
      ["dim", "  → patching expiry rounding …"],
      ["", "  edit  store/redis/device_codes.go (+3 −3)"],
      ["dim", "  → re-running store/redis tests …"],
      ["__cursor__", ""],
    ],
    "s-902": [
      ["dim", "$ opencode run \"/speckit.implement\""],
      ["", "Loaded 6 tracked tasks."],
      ["ok", "  ✓ T001 Audit sync DB call-sites (37 sites)"],
      ["ok", "  ✓ T002 Introduce async pool"],
      ["accent", "▸ T003  Migrate billing repository"],
      ["", "  edit  billing/repository.py (+96 −74)"],
      ["", "  test  tests/billing/test_repository.py"],
      ["dim", "  → spinning up fixture: postgres-async …"],
      ["dim", "  → waiting for fixture readiness …"],
      ["warn", "  ⚠ fixture 'postgres-async' not ready after 360s"],
      ["dim", "  (no output for 6m 12s)"],
      ["__cursor__", ""],
    ],
    "s-903": [
      ["dim", "$ agy -p \"/speckit.implement\""],
      ["", "Parsing OpenAPI 3.1 spec … 214 operations"],
      ["ok", "  ✓ TypeScript client generated (sdk/ts)"],
      ["ok", "  ✓ Python client generated (sdk/py)"],
      ["ok", "  ✓ Go client generated (sdk/go)"],
      ["ok", "  ✓ Published 5 SDKs to internal registry"],
      ["ok", "✓ Session completed — all checks passed."],
      ["dim", "daedalus: session ended · output persisted · review mode"],
    ],
    "s-904": [
      ["dim", "$ opencode run \"/speckit.implement\""],
      ["ok", "  ✓ Lock hierarchy introduced"],
      ["ok", "  ✓ Session store refactored"],
      ["accent", "▸ T004  Regression tests"],
      ["", "  test  ./auth/… (deadlock suite)"],
      ["err", "  ✗ TestConcurrentLogin_NoDeadlock — timeout (30s)"],
      ["err", "  ✗ TestTokenRefresh_Ordering — deadlock detected"],
      ["err", "  ✗ TestSessionEvict_Race — timeout (30s)"],
      ["err", "FAIL  ./auth  3 failed, 11 passed"],
      ["err", "✗ Run aborted — exit code 1"],
      ["dim", "daedalus: session failed · output persisted · review mode"],
    ],
    "s-905": [
      ["dim", "daedalus: starting session s-905 (backend: workshop)"],
      ["", "Creating Workshop environment … fresh sandbox (4 vCPU / 8 GB)"],
      ["ok", "  ✓ environment ws-4187 created (2.1s)"],
      ["", "Cloning acme/gateway … done (24 MB)"],
      ["", "Mounting worktree agent/053-webhook-retry … done"],
      ["", "Registering daedalus SDK inside sandbox … ok (advertising on localhost)"],
      ["", "Launching tool: claude -p \"/speckit.implement\" (zellij session s-905)"],
      ["dim", "  → waiting for agent handshake …"],
      ["__cursor__", ""],
    ],
    "s-906": [
      ["dim", "$ agy -p \"/speckit.implement\""],
      ["dim", "daedalus: attached to session s-906 (zellij · tunnel eu-fra-1)"],
      ["", "Reading specs/048-ratelimit/tasks.md … 5 tasks"],
      ["ok", "  ✓ T001  Choose token-bucket strategy"],
      ["accent", "▸ T002  Middleware implementation"],
      ["", "  edit  api/middleware/ratelimit.go (+204)"],
      ["", "  test  ./api/middleware/… -run TestBucket -v -count=100"],
      ["dim", "  → verbose test output (100 iterations × 412 cases) …"],
      ["", "  === RUN   TestBucket/refill_rate_burst_97/iter_99"],
      ["", "  --- PASS: TestBucket/refill_rate_burst_97/iter_99 (0.00s)"],
      ["", "  === RUN   TestBucket/refill_rate_burst_98/iter_99"],
      ["", "  --- PASS: TestBucket/refill_rate_burst_98/iter_99 (0.00s)"],
      ["", "  === RUN   TestBucket/refill_rate_burst_99/iter_99"],
      ["__cursor__", ""],
    ],
    "s-907": [
      ["dim", "$ claude -p \"/speckit.implement\""],
      ["dim", "daedalus: attached to session s-907 (zellij)"],
      ["", "Reading specs/050-event-docs/tasks.md … 3 tasks"],
      ["ok", "  ✓ T001  Crawl event emitters (214 emit sites)"],
      ["accent", "▸ T002  Build schema registry"],
      ["", "  edit  docs/events/registry.yaml (+402)"],
      ["ok", "  ✓ T002 complete"],
      ["accent", "▸ T003  Generate markdown docs"],
      ["", "  edit  docs/events/overview.md (+38)"],
      ["warn", "✱ Stopped by operator (SIGTERM) — 12:41:07"],
      ["dim", "daedalus: session stopped · output persisted · review mode"],
    ],
    "s-908": [
      ["dim", "$ opencode run \"/speckit.implement\""],
      ["dim", "daedalus: attached to session s-908 (zellij · tunnel eu-fra-1)"],
      ["ok", "  ✓ T001 Resolve dependency graph (312 crates)"],
      ["accent", "▸ T002  Bump minor versions"],
      ["", "  edit  Cargo.toml (+41 −41)"],
      ["", "  run   cargo update -p serde -p tokio -p axum"],
      ["dim", "  → compiling workspace (round 2/3) …"],
      ["", "   Compiling daedalus-core v0.4.2"],
      ["", "   Compiling daedalus-pro"],
      ["dim", "── connection lost · showing last-known output as of 12:03:41 (15m ago) ──"],
    ],
    "s-909": [
      ["dim", "$ claude -p \"/speckit.implement\""],
      ["dim", "daedalus: attached to session s-909 (zellij)"],
      ["", "Reading specs/056-uuid-pks/tasks.md … 5 tasks"],
      ["ok", "  ✓ T001  Add uuid columns"],
      ["ok", "  ✓ T002  Backfill uuids (1.2M rows, 4m11s)"],
      ["accent", "▸ T003  Swap primary keys"],
      ["", "  edit  db/migrations/0094_uuid_pks.sql (+58)"],
      ["warn", "  ⚠ spec.md doesn’t cover the `sessions` table foreign key."],
      ["prompt", "? Migrate `sessions.user_id` in this pass too, or leave for a follow-up?"],
      ["prompt", "    [1] migrate now    [2] follow-up"],
      ["dim", "  (waiting for input — 8m)"],
      ["__cursor__", ""],
    ],
    "s-910": [
      ["dim", "$ agy -p \"/speckit.implement\""],
      ["dim", "daedalus: attached to session s-910 (zellij)"],
      ["", "Reading specs/057-mobile-deviceflow/tasks.md … 4 tasks"],
      ["ok", "  ✓ T001  Scaffold device-flow endpoints"],
      ["accent", "▸ T002  Token exchange handler"],
      ["", "  test  needs seeded fixtures in staging"],
      ["warn", "  ⚠ staging DB holds stale fixtures from a prior run."],
      ["prompt", "? About to run a DESTRUCTIVE reset on staging to re-seed. Approve? [y/N]"],
      ["dim", "  (waiting for input — 2m)"],
      ["__cursor__", ""],
    ],
    "s-911": [
      ["dim", "$ claude -p \"/speckit.implement\""],
      ["dim", "daedalus: attached to session s-911 (zellij)"],
      ["", "Reading specs/058-idempotent-recon/tasks.md … 9 tasks"],
      ["ok", "  ✓ T005  Retry / backoff policy"],
      ["accent", "▸ T006  Replay tests (idempotency suite)"],
      ["", "  test  tests/billing/test_replay.py"],
      ["ok", "  ok   billing.replay  4.8s  (23 passed)"],
      ["ok", "  ✓ T006 complete"],
      ["accent", "▸ T007  Docs + runbook update"],
      ["", "  edit  docs/runbooks/reconciliation.md (+64 −18)"],
      ["ok", "  ✓ T007 complete"],
      ["", ""],
      ["", "Summary: 7/9 tasks complete. T008 and T009 are production cut-over"],
      ["", "migrations — skipped pending a decision on the backfill window."],
      ["ok", "✓ Agent run finished."],
      ["dim", "process exited (code 0)"],
      ["dim", "daedalus: run ended with 2 tracked tasks unconfirmed — confirm completion or clean up"],
    ],
  };

  function pickTerminal(id) { return TERMINAL[id] || TERMINAL["s-901"]; }

  // ── Lifecycle events (telemetry-rail timeline) ─────────────────────────────
  // Oldest → newest. kind ∈ "status" | "action" | "note"; status events carry
  // `st` (a session-status key) so the rail can color the node var(--st-<st>).
  const EVENTS = {
    "s-901": [
      { t: "23m ago", kind: "status", st: "starting", label: "Created — provisioning environment" },
      { t: "22m ago", kind: "note", label: "Worktree mounted (acme/monorepo @ main)" },
      { t: "22m ago", kind: "status", st: "running", label: "Agent started (claude -p)" },
      { t: "12m ago", kind: "note", label: "T003 token polling + rate-limit done" },
      { t: "8m ago", kind: "note", label: "T004 gateway middleware wired" },
      { t: "4s ago", kind: "note", label: "Persisting device codes (T005)" },
    ],
    "s-902": [
      { t: "44m ago", kind: "status", st: "starting", label: "Created — provisioning environment" },
      { t: "43m ago", kind: "status", st: "running", label: "Agent started (opencode run)" },
      { t: "31m ago", kind: "note", label: "T002 async pool introduced" },
      { t: "14m ago", kind: "note", label: "Fixture postgres-async requested" },
      { t: "6m ago", kind: "status", st: "stalled", label: "No output — possible stall" },
    ],
    "s-904": [
      { t: "15m ago", kind: "status", st: "starting", label: "Created — provisioning environment" },
      { t: "14m ago", kind: "status", st: "running", label: "Agent started (opencode run)" },
      { t: "7m ago", kind: "note", label: "Lock hierarchy introduced (T002)" },
      { t: "3m ago", kind: "note", label: "Deadlock regression suite started" },
      { t: "2m ago", kind: "status", st: "failed", label: "Exited (1) — 3 deadlock tests failing" },
    ],
    "s-908": [
      { t: "30m ago", kind: "status", st: "starting", label: "Created — provisioning environment" },
      { t: "29m ago", kind: "status", st: "running", label: "Agent started (opencode run)" },
      { t: "21m ago", kind: "note", label: "T001 dependency graph resolved" },
      { t: "16m ago", kind: "note", label: "Bumping minor versions (T002)" },
      { t: "15m ago", kind: "status", st: "unknown", label: "Tunnel eu-fra-1 dropped — connection lost" },
    ],
    "s-909": [
      { t: "16m ago", kind: "status", st: "starting", label: "Created — provisioning environment" },
      { t: "15m ago", kind: "status", st: "running", label: "Agent started (claude -p)" },
      { t: "10m ago", kind: "note", label: "T002 backfilled 1.2M uuids (4m11s)" },
      { t: "8m ago", kind: "status", st: "awaiting", label: "Asked about the `sessions` FK scope" },
    ],
    "s-911": [
      { t: "1h 31m ago", kind: "status", st: "starting", label: "Created — provisioning environment" },
      { t: "1h 30m ago", kind: "status", st: "running", label: "Agent started (claude -p)" },
      { t: "48m ago", kind: "note", label: "Replay-safe cursor checkpoints (T004)" },
      { t: "19m ago", kind: "note", label: "Idempotency replay suite green (T006)" },
      { t: "12m ago", kind: "note", label: "T008/T009 skipped — production cut-over pending" },
      { t: "12m ago", kind: "status", st: "confirm", label: "Exited cleanly (0) — 2 tasks unconfirmed" },
    ],
  };
  const ago = (sec) => sec == null ? "—"
    : sec < 60 ? sec + "s ago"
    : sec < 3600 ? Math.floor(sec / 60) + "m ago"
    : Math.floor(sec / 3600) + "h " + Math.floor((sec % 3600) / 60) + "m ago";
  // Fallback generator for sessions without a curated event list.
  function sessionEvents(sOrId) {
    const s = typeof sOrId === "string" ? SESSIONS.find((x) => x.id === sOrId) : sOrId;
    if (!s) return [];
    if (EVENTS[s.id]) return EVENTS[s.id];
    const ev = [{ t: ago(s.started), kind: "status", st: "starting", label: "Created — provisioning environment" }];
    if (s.status !== "starting") {
      ev.push({ t: ago(Math.max(0, s.started - 30)), kind: "status", st: "running", label: "Agent started" });
      if (s.status !== "running") ev.push({ t: ago(s.ended ?? s.lastEvent), kind: "status", st: s.status, label: STATUS_LABEL[s.status] || s.status });
      else ev.push({ t: ago(s.lastEvent), kind: "note", label: "Last output received" });
    }
    return ev;
  }

  // ── Backend TYPES (sandboxing technologies) and HOSTS (physical machines) ──
  // An Environment lives on a Host and is OF a backend type.
  const BACKEND_TYPES = [
    { id: "workshop", name: "Canonical Workshop", icon: "linux", kind: "sandbox",
      desc: "Isolated Linux sandbox. Fresh or pre-existing environments." },
    { id: "openshell", name: "NVIDIA OpenShell", icon: "gpu", kind: "sandbox",
      desc: "GPU-accelerated sandbox for ML / CUDA workloads." },
    { id: "macos", name: "macOS sandbox", icon: "apple", kind: "sandbox",
      desc: "Local sandbox-exec isolation on macOS." },
    { id: "onhost", name: "On host", icon: "host", kind: "direct",
      desc: "Runs directly on the host — no isolation. Shares the host filesystem." },
  ];
  const HOSTS = [
    { id: "local", name: "This host", kind: "local", os: "Linux", availability: "available",
      supports: ["workshop", "openshell", "onhost"], note: "The machine Daedalus runs on." },
    { id: "eu-fra-1", name: "eu-fra-1", kind: "tunnel", os: "Linux", availability: "available",
      supports: ["workshop", "openshell"], note: "Remote Workshop over an authenticated tunnel." },
    { id: "us-east-2", name: "us-east-2", kind: "tunnel", os: "Linux", availability: "idle",
      supports: ["workshop"], note: "Tunnel idle — connect to use." },
    { id: "studio-mac", name: "studio-mac.local", kind: "mdns", os: "macOS", availability: "degraded",
      supports: ["macos", "onhost"], note: "High memory pressure — new environments may be slow." },
  ];
  // Maps a session/environment's legacy `backend` value → { type, host }.
  const BACKEND_MAP = {
    workshop: { type: "workshop", host: "local" },
    "workshop-remote": { type: "workshop", host: "eu-fra-1" },
    macos: { type: "macos", host: "studio-mac" },
    openshell: { type: "openshell", host: "local" },
  };
  const placement = (backend) => BACKEND_MAP[backend] || { type: backend, host: "local" };

  // Reachability of the host a session runs on, for availability-aware chips.
  // A dropped source (status "unknown") always reads unreachable regardless of
  // what the host list says — the session's own link is what's gone.
  function sessionHostAvail(s) {
    if (s.status === "unknown") return "unavailable";
    const h = HOSTS.find((x) => x.id === placement(s.backend).host);
    if (!h) return "available";
    return h.availability === "idle" ? "degraded" : h.availability;
  }

  const NAV = [
    { id: "tasks", label: "Tasks", icon: "board" },
    { id: "fleet", label: "Sessions", icon: "fleet", count: "running" },
    { id: "discover", label: "Discover", icon: "radar" },
    { id: "tools", label: "Tools", icon: "tools" },
    { id: "backends", label: "Environments", icon: "backends" },
  ];

  const STATUS_LABEL = {
    starting: "Starting", running: "Running", awaiting: "Waiting for input", confirm: "Awaiting confirmation",
    stalled: "Stalled", failed: "Failed",
    completed: "Completed", stopped: "Stopped", unknown: "Connection lost", unreachable: "Unreachable",
  };

  const TASK_STATUS_LABEL = { todo: "To do", doing: "In progress", blocked: "Blocked", done: "Done" };

  // Flatten every tracked task across all sessions, carrying session context.
  // This is the unit the operator actually cares about — session becomes a field.
  function allTasks() {
    const out = [];
    SESSIONS.forEach((s) => {
      s.tasks.forEach((t, i) => {
        const attention = t.status === "blocked" ||
          (t.status === "doing" && ["stalled", "failed", "unknown", "awaiting", "confirm"].includes(s.status));
        out.push({
          ...t,
          key: s.id + ":" + t.id,
          seq: i,
          sessionId: s.id,
          objective: s.objective,
          tool: s.tool, toolName: s.toolName,
          backend: s.backend, backendName: s.backendName,
          env: s.env, envName: s.envName, envKind: s.envKind,
          source: s.source, spec: s.spec, issue: s.issue,
          sessionStatus: s.status,
          attention,
        });
      });
    });
    return out;
  }

  // ── “Needs you” queue ────────────────────────────────────────────────────
  // Anything blocked on the operator: the agent asked a question (awaiting),
  // a clean exit left tracked tasks unfinished (confirm), a run stalled,
  // failed, or the connection dropped. Detection stays dumb — “awaiting”
  // simply means the agent printed a prompt and is blocked on input.
  const IDLE_RATE = { workshop: 0.45, "workshop-remote": 0.90, macos: 0.30 }; // $/hr while a sandbox sits blocked
  function needsYou() {
    const order = { awaiting: 0, confirm: 1, stalled: 2, unknown: 3, failed: 4 };
    return SESSIONS
      .filter((s) => ["awaiting", "confirm", "stalled", "failed", "unknown"].includes(s.status))
      .map((s) => {
        const blockedTask = s.tasks.find((t) => t.status === "doing" && t.detail) || s.tasks.find((t) => t.status === "blocked" && t.detail);
        const waitingSec = (s.status === "awaiting" || s.status === "confirm") ? s.waitingFor
          : s.status === "stalled" ? s.lastEvent
          : (s.ended ?? s.lastEvent);
        const reason = s.status === "awaiting" ? s.prompt
          : s.status === "confirm" ? s.summary
          : s.status === "stalled" ? (blockedTask ? blockedTask.detail : "No output — possible stall.")
          : s.status === "failed" ? s.exit
          : s.note;
        const cue = s.status === "awaiting" ? "Asked you a question"
          : s.status === "confirm" ? "Run ended — confirm completion"
          : s.status === "stalled" ? "No output — may be stuck"
          : s.status === "failed" ? "Run failed — needs a call"
          : "Connection lost";
        const live = ["awaiting", "stalled"].includes(s.status); // still holding a live sandbox (confirm has exited — env held)
        const costUsd = live ? +((waitingSec / 3600) * (IDLE_RATE[s.backend] || 0.5)).toFixed(2) : 0;
        return { ...s, reasonKind: s.status, reason, cue, waitingSec, costUsd, live };
      })
      .sort((a, b) => (order[a.reasonKind] - order[b.reasonKind]) || (b.waitingSec - a.waitingSec));
  }

  window.DATA = { TOOLS, BACKENDS, BACKEND_TYPES, HOSTS, BACKEND_MAP, placement, sessionHostAvail, ENVIRONMENTS, SESSIONS, DISCOVERED, NOTIFS, NAV,
    STATUS_LABEL, TASK_STATUS_LABEL, pickTerminal, EVENTS, sessionEvents, allTasks, needsYou };
})();
