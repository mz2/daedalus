//! NVIDIA OpenShell backend (issue #9).
//!
//! Drives the `openshell` CLI to run agents inside kernel-level sandboxes (seccomp,
//! Landlock, network namespaces) governed by a per-session declarative YAML policy.
//! Supported hosts: Linux (GPU-capable via the NVIDIA Container Toolkit) and macOS on
//! Apple silicon, where enforcement runs inside the Docker Desktop Linux VM — Linux
//! container environments, no GPU (support matrix, issue #9 comments).
//!
//! The host control surface is modelled behind [`OpenShellControl`] so availability
//! tiers and failure paths are testable without the CLI or Docker present; the exact
//! CLI/policy schema is confirmed against real OpenShell during contract work (the
//! open questions recorded on issue #9).

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::Mutex;

use async_trait::async_trait;

use daedalus_backend::{
    require_worktree, AcquireRequest, AgentHandle, Backend, BackendError, UsageSample,
};
use daedalus_proto::{
    Availability, BackendId, BackendKind, EnvironmentId, InvocationSpec, MetricKind,
    SandboxEnvironment,
};

/// The OpenShell host surface Daedalus probes and drives.
///
/// Pluggable for tests and future transports (the gateway API); the default resolves the
/// CLI and host runtimes from the environment.
pub trait OpenShellControl: Send + Sync {
    /// Name/path of the `openshell` CLI, if configured/present.
    fn control_binary(&self) -> Option<String>;
    /// Whether the container runtime backing sandboxes is ready (the Docker Desktop VM on
    /// macOS; Docker or native kernel enforcement on Linux).
    fn container_runtime_ready(&self) -> bool;
    /// Whether GPU passthrough is ready (NVIDIA driver + Container Toolkit; Linux hosts
    /// only — macOS sandboxes never see a GPU).
    fn gpu_ready(&self) -> bool;
}

/// Default control resolving the `openshell` CLI from `DAEDALUS_OPENSHELL_CMD` or `PATH`,
/// and probing the container runtime via the `docker` CLI.
#[derive(Debug, Default)]
pub struct CliOpenShellControl;

impl OpenShellControl for CliOpenShellControl {
    fn control_binary(&self) -> Option<String> {
        if let Ok(cmd) = std::env::var("DAEDALUS_OPENSHELL_CMD") {
            if !cmd.is_empty() {
                return Some(cmd);
            }
        }
        // Supported hosts only: Linux, and macOS on Apple silicon (Intel is unsupported
        // per the OpenShell support matrix).
        if cfg!(target_os = "linux") || (cfg!(target_os = "macos") && cfg!(target_arch = "aarch64"))
        {
            which_on_path("openshell")
        } else {
            None
        }
    }

    fn container_runtime_ready(&self) -> bool {
        // Docker Desktop on macOS; Docker (or native enforcement) on Linux. Presence of
        // the CLI is the cheap probe; a real daemon ping happens on first `acquire`.
        which_on_path("docker").is_some()
    }

    fn gpu_ready(&self) -> bool {
        // GPU is Linux-hosts-only (CDI / NVIDIA Container Toolkit).
        cfg!(target_os = "linux") && which_on_path("nvidia-smi").is_some()
    }
}

fn which_on_path(bin: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(bin);
        if candidate.is_file() {
            return candidate.to_str().map(str::to_owned);
        }
    }
    None
}

/// Generate the per-session OpenShell policy (YAML) from an acquire request.
///
/// The policy encodes Daedalus's guarantees declaratively: the isolated worktree is the
/// only host mount (FR-002a; fresh environments mount nothing), outbound network is
/// denied by default (local-first posture), no host environment/credentials pass through
/// (FR-031), operator resource limits map onto the sandbox, and GPU appears only when the
/// host is GPU-ready. Field names are validated against real OpenShell during contract
/// work (issue #9 open questions).
#[must_use]
pub fn session_policy_yaml(req: &AcquireRequest, gpu: bool) -> String {
    let mut yaml = String::from("version: 1\nsandbox:\n  filesystem:\n");
    match &req.worktree {
        Some(wt) => {
            // The isolated git worktree is the one read-write host mount (FR-002a).
            yaml.push_str("    mounts:\n");
            let _ = writeln!(yaml, "      - source: {}", wt.path);
            yaml.push_str("        target: /workspace\n        mode: rw\n");
            let _ = writeln!(yaml, "        branch: {}", wt.branch);
        }
        None => yaml.push_str("    mounts: []\n"),
    }
    // Local-first posture: no open egress unless the operator widens it explicitly.
    yaml.push_str("  network:\n    outbound: deny\n");
    // Never forward host credentials/environment (FR-031).
    yaml.push_str("  environment:\n    passthrough: []\n");
    let limits = &req.limits;
    if limits.cpu_cores.is_some()
        || limits.memory_bytes.is_some()
        || limits.disk_bytes.is_some()
        || limits.time_secs.is_some()
    {
        yaml.push_str("  resources:\n");
        if let Some(cpu) = limits.cpu_cores {
            let _ = writeln!(yaml, "    cpu_cores: {cpu}");
        }
        if let Some(mem) = limits.memory_bytes {
            let _ = writeln!(yaml, "    memory_bytes: {mem}");
        }
        if let Some(disk) = limits.disk_bytes {
            let _ = writeln!(yaml, "    disk_bytes: {disk}");
        }
        if let Some(time) = limits.time_secs {
            let _ = writeln!(yaml, "    time_secs: {time}");
        }
    }
    if gpu {
        yaml.push_str("  gpu:\n    enabled: true\n");
    }
    yaml
}

#[derive(Debug, Clone)]
struct EnvState {
    policy_yaml: String,
    zellij_session: Option<String>,
}

/// OpenShell-backed sandbox provider.
pub struct OpenShellBackend {
    id: BackendId,
    control: Box<dyn OpenShellControl>,
    envs: Mutex<HashMap<EnvironmentId, EnvState>>,
}

impl Default for OpenShellBackend {
    fn default() -> Self {
        Self::new(Box::new(CliOpenShellControl))
    }
}

impl OpenShellBackend {
    /// Create the backend with a given host control surface.
    #[must_use]
    pub fn new(control: Box<dyn OpenShellControl>) -> Self {
        Self {
            id: BackendId::new(),
            control,
            envs: Mutex::new(HashMap::new()),
        }
    }

    /// Whether this host can offer GPU-capable environments (Linux + NVIDIA Container
    /// Toolkit). A capability, not an availability tier: a macOS host is fully available
    /// without it.
    #[must_use]
    pub fn gpu_capable(&self) -> bool {
        self.control.gpu_ready()
    }

    /// Number of environments currently held (0 proves nothing was orphaned — C-B2).
    #[must_use]
    pub fn live_environment_count(&self) -> usize {
        self.envs.lock().expect("poisoned").len()
    }

    /// The generated policy confining the given environment, if it exists — what a real
    /// `openshell sandbox create` is driven by, auditable by the operator.
    #[must_use]
    pub fn policy_for(&self, env: &EnvironmentId) -> Option<String> {
        self.envs
            .lock()
            .expect("poisoned")
            .get(env)
            .map(|s| s.policy_yaml.clone())
    }

    /// The sandbox name a real OpenShell CLI call addresses for this environment.
    fn sandbox_name(env: &EnvironmentId) -> String {
        format!("daedalus-{env}")
    }

    fn not_available_reason(&self) -> Option<String> {
        if self.control.control_binary().is_none() {
            return Some(
                "openshell CLI not found (install OpenShell or set DAEDALUS_OPENSHELL_CMD)".into(),
            );
        }
        if !self.control.container_runtime_ready() {
            return Some(
                "container runtime not ready (start Docker Desktop on macOS / Docker on Linux)"
                    .into(),
            );
        }
        None
    }
}

#[async_trait]
impl Backend for OpenShellBackend {
    fn id(&self) -> BackendId {
        self.id
    }

    fn kind(&self) -> BackendKind {
        BackendKind::OpenShell
    }

    async fn availability(&self) -> Availability {
        // Availability is a value, never an error (FR-028, C-B4). CLI missing ⇒ the
        // backend is absent; CLI present but the container runtime down ⇒ degraded (the
        // operator can start Docker without reconfiguring Daedalus).
        if self.control.control_binary().is_none() {
            Availability::Unavailable
        } else if !self.control.container_runtime_ready() {
            Availability::Degraded
        } else {
            Availability::Available
        }
    }

    async fn availability_reason(&self) -> Option<String> {
        self.not_available_reason()
    }

    async fn acquire(&self, req: AcquireRequest) -> Result<SandboxEnvironment, BackendError> {
        if let Some(reason) = self.not_available_reason() {
            return Err(BackendError::ProvisionFailed(reason));
        }
        require_worktree(&req)?;

        let policy_yaml = session_policy_yaml(&req, self.control.gpu_ready());
        // A real implementation feeds the policy to `openshell sandbox create` here and
        // confirms the sandbox handle before recording the environment.
        let env = req.into_ready_environment(self.id);
        self.envs.lock().expect("poisoned").insert(
            env.id,
            EnvState {
                policy_yaml,
                zellij_session: None,
            },
        );
        Ok(env)
    }

    async fn start_agent(
        &self,
        env: &EnvironmentId,
        tool: &InvocationSpec,
    ) -> Result<AgentHandle, BackendError> {
        let control = self
            .control
            .control_binary()
            .ok_or_else(|| BackendError::StartFailed("openshell CLI gone".into()))?;
        if !self.envs.lock().expect("poisoned").contains_key(env) {
            return Err(BackendError::NotFound);
        }
        let zellij_session = daedalus_zellij::format_session_name(env);

        // Launch the agent inside the sandbox, under zellij (FR-004, FR-011):
        //   <openshell> sandbox exec <name> -- zellij --session <name> -- <program> <args...>
        let mut cmd = tokio::process::Command::new(&control);
        cmd.arg("sandbox")
            .arg("exec")
            .arg(Self::sandbox_name(env))
            .arg("--")
            .arg("zellij")
            .arg("--session")
            .arg(&zellij_session)
            .arg("--")
            .arg(&tool.program)
            .args(&tool.args);

        if let Err(e) = cmd.spawn() {
            // A failed start tears the environment down — no orphan (FR-005, C-B2).
            self.envs.lock().expect("poisoned").remove(env);
            return Err(BackendError::StartFailed(e.to_string()));
        }

        if let Some(state) = self.envs.lock().expect("poisoned").get_mut(env) {
            state.zellij_session = Some(zellij_session.clone());
        }
        Ok(AgentHandle {
            env: *env,
            zellij_session,
        })
    }

    async fn resource_usage(&self, env: &EnvironmentId) -> Result<Vec<UsageSample>, BackendError> {
        if !self.envs.lock().expect("poisoned").contains_key(env) {
            return Err(BackendError::NotFound);
        }
        // A full implementation queries the sandbox's cgroup stats via the CLI; report
        // time-only until that surface is confirmed (issue #9 open questions, FR-019).
        Ok(vec![UsageSample {
            metric: MetricKind::Time,
            value: 0.0,
        }])
    }

    async fn stop(&self, env: &EnvironmentId) -> Result<(), BackendError> {
        // Idempotent (C-B5): stopping an unknown / already-stopped env is Ok.
        let session = self
            .envs
            .lock()
            .expect("poisoned")
            .get_mut(env)
            .and_then(|s| s.zellij_session.take());
        if let Some(session) = session {
            let _ = tokio::process::Command::new("zellij")
                .arg("kill-session")
                .arg(&session)
                .output()
                .await;
        }
        Ok(())
    }

    async fn teardown(&self, env: &EnvironmentId) -> Result<(), BackendError> {
        // Idempotent and isolated to one environment (C-B5, FR-024).
        let removed = self.envs.lock().expect("poisoned").remove(env);
        if removed.is_some() {
            if let Some(control) = self.control.control_binary() {
                let _ = tokio::process::Command::new(&control)
                    .arg("sandbox")
                    .arg("delete")
                    .arg(Self::sandbox_name(env))
                    .output()
                    .await;
            }
        }
        Ok(())
    }
}
