# Contract: Backend abstraction

**Crate**: `daedalus-backend` (trait) — implemented by `daedalus-backend-{workshop,openshell,fake}`;
further backends integrate existing agent-oriented sandbox runtimes (srt #12, CodeRunner #13) — Daedalus
never implements its own isolation primitives (2026-07-06 amendment).
**Satisfies**: FR-004, FR-005, FR-002/002a, FR-022/024, FR-027/028, SC-002, SC-013.

All sandbox backends MUST implement one common async trait. Adding a backend MUST NOT change the operator
workflow (FR-027) or the surfaces. Backend-specific behavior stays behind this boundary.

```rust
#[async_trait]
pub trait Backend: Send + Sync {
    fn id(&self) -> BackendId;
    fn kind(&self) -> BackendKind;

    /// Current availability; MUST NOT error — unavailability is a value, not a failure (FR-028).
    async fn availability(&self) -> Availability;

    /// Provision a fresh isolated environment, or attach a pre-existing one the operator selected.
    /// For PreExisting, MUST establish an isolated git worktree; if it cannot, return
    /// Err(WorktreeUnavailable) and leave NO orphaned environment (FR-002a, FR-005).
    async fn acquire(&self, req: AcquireRequest) -> Result<Environment, BackendError>;

    /// Launch the agentic tool inside the environment, under zellij. Confined to the sandbox (FR-004).
    async fn start_agent(&self, env: &EnvironmentId, tool: &InvocationSpec)
        -> Result<AgentHandle, BackendError>;

    /// Observed resource usage for the environment (FR-019).
    async fn resource_usage(&self, env: &EnvironmentId) -> Result<Vec<ResourceUsageMetric>, BackendError>;

    /// Stop the running agent (FR-022). Idempotent.
    async fn stop(&self, env: &EnvironmentId) -> Result<(), BackendError>;

    /// Release/tear down the environment, freeing resources without affecting others (FR-024).
    async fn teardown(&self, env: &EnvironmentId) -> Result<(), BackendError>;
}
```

### Contract rules (test these first — Principle VI)

- **C-B1**: `acquire` with `Origin::PreExisting` and an un-creatable worktree returns `WorktreeUnavailable`
  and leaves no environment (FR-002a). *(integration + contract test)*
- **C-B2**: any failure in `acquire`/`start_agent` leaves no orphaned environment (FR-005). *(test)*
- **C-B3**: a started agent cannot read/modify the host filesystem/processes — verified with an adversarial
  agent for each real backend (SC-002). *(integration test; the fake backend asserts the contract shape)*
- **C-B4**: `availability` never returns `Err`; a down backend reports `Unavailable` while other backends
  keep working (FR-028). *(test)*
- **C-B5**: `stop` and `teardown` are idempotent and isolated to one environment (FR-024). *(test)*

### v1 implementations

| Backend | Isolation primitive | Notes |
|---------|---------------------|-------|
| `workshop` | Canonical Workshop (Linux) env | exact control surface confirmed during impl (R-WS) |
| `openshell` | NVIDIA OpenShell kernel sandbox (seccomp/Landlock/netns) | Linux (GPU-capable) + macOS Apple silicon via Docker Desktop, no GPU; per-session YAML policy — worktree mount, deny-outbound, no env passthrough (issue #9) |
| `fake` | in-process simulation | Principle III local testability; drives all contract tests |
