//! NVIDIA OpenShell backend (issue #9).
//!
//! Drives the `openshell` CLI to run agents inside kernel-level sandboxes (seccomp,
//! Landlock, network namespaces) governed by a per-session declarative YAML policy.
//! Supported hosts: Linux (GPU-capable via the NVIDIA Container Toolkit) and macOS on
//! Apple silicon, where enforcement runs inside the Docker Desktop Linux VM — Linux
//! container environments, no GPU (support matrix, issue #9 comments).
//!
//! The CLI surface and policy schema implemented here were confirmed against openshell
//! 0.0.77 on macOS/Apple silicon (2026-07-07): `sandbox create --name … --policy …
//! [--cpu/--memory/--gpu] --no-auto-providers`, `sandbox exec -n … --no-tty -- …`,
//! `sandbox delete <name>`; policy schema per `openshell policy get --base -o json`
//! (`filesystem_policy` / `network_policies` / `landlock`). The host surface is modelled
//! behind [`OpenShellControl`] so availability tiers and failure paths are testable
//! without the CLI or Docker present.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use daedalus_backend::{
    require_worktree, AcquireRequest, AgentHandle, Backend, BackendError, UsageSample,
};
use daedalus_proto::{
    Availability, BackendId, BackendKind, EnvironmentId, InvocationSpec, MetricKind,
    SandboxEnvironment,
};

/// Output of a completed control-plane CLI invocation.
#[derive(Debug, Clone)]
pub struct CliOutput {
    /// Whether the command exited successfully.
    pub success: bool,
    /// Captured standard output.
    pub stdout: String,
    /// Captured standard error.
    pub stderr: String,
}

/// The OpenShell host surface Daedalus probes and drives.
///
/// Pluggable for tests and future transports (the gateway's mTLS gRPC control plane, if
/// NVIDIA documents it for external clients); the default resolves the CLI and host
/// runtimes from the environment.
#[async_trait]
pub trait OpenShellControl: Send + Sync {
    /// Name/path of the `openshell` CLI, if configured/present.
    fn control_binary(&self) -> Option<String>;
    /// Whether the container runtime backing sandboxes is ready (the Docker Desktop VM on
    /// macOS; Docker or native kernel enforcement on Linux).
    fn container_runtime_ready(&self) -> bool;
    /// Whether GPU passthrough is ready (NVIDIA driver + Container Toolkit; Linux hosts
    /// only — macOS sandboxes never see a GPU).
    fn gpu_ready(&self) -> bool;
    /// Run a control-plane command (`sandbox create`/`delete`, …) to completion.
    async fn run(&self, args: &[String]) -> Result<CliOutput, String>;
    /// Spawn a long-lived command detached — the agent's `sandbox exec` lives as long as
    /// the session does and is never awaited here.
    fn spawn(&self, args: &[String]) -> Result<(), String>;
}

/// Default control resolving the `openshell` CLI from `DAEDALUS_OPENSHELL_CMD` or `PATH`,
/// and probing the container runtime via the `docker` CLI.
#[derive(Debug, Default)]
pub struct CliOpenShellControl;

impl CliOpenShellControl {
    fn binary(&self) -> Option<String> {
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
}

#[async_trait]
impl OpenShellControl for CliOpenShellControl {
    fn control_binary(&self) -> Option<String> {
        self.binary()
    }

    fn container_runtime_ready(&self) -> bool {
        // Docker Desktop on macOS; Docker (or native enforcement) on Linux. Presence of
        // the CLI is the cheap probe; the daemon answers on first `acquire`.
        which_on_path("docker").is_some()
    }

    fn gpu_ready(&self) -> bool {
        // GPU is Linux-hosts-only (CDI / NVIDIA Container Toolkit).
        cfg!(target_os = "linux") && which_on_path("nvidia-smi").is_some()
    }

    async fn run(&self, args: &[String]) -> Result<CliOutput, String> {
        let binary = self
            .binary()
            .ok_or_else(|| "openshell CLI gone".to_string())?;
        let output = tokio::process::Command::new(&binary)
            .args(args)
            .output()
            .await
            .map_err(|e| e.to_string())?;
        Ok(CliOutput {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    fn spawn(&self, args: &[String]) -> Result<(), String> {
        let binary = self
            .binary()
            .ok_or_else(|| "openshell CLI gone".to_string())?;
        tokio::process::Command::new(&binary)
            .args(args)
            .spawn()
            .map(drop)
            .map_err(|e| e.to_string())
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

/// Where a pre-existing session's worktree lands inside the sandbox (FR-002a). Delivery
/// into this path (upload vs driver-level bind) is tracked on issue #9.
pub const WORKTREE_MOUNT: &str = "/sandbox/worktree";

/// Generate the per-session OpenShell policy (YAML) from an acquire request, in the real
/// schema (`policy get --base -o json`, openshell 0.0.77).
///
/// The policy encodes Daedalus's guarantees declaratively: deny-all egress via an empty
/// `network_policies` map (the enforcing proxy 403s every CONNECT no policy allows —
/// local-first posture), the baseline filesystem grants the built-in default uses (so the
/// sandbox supervisor stays functional) plus [`WORKTREE_MOUNT`] read-write only for
/// pre-existing environments (FR-002a), and no provider/credential entries (FR-031 —
/// resource limits and GPU ride `sandbox create` flags instead, see `acquire`).
#[must_use]
pub fn session_policy_yaml(req: &AcquireRequest) -> String {
    let mut yaml = String::from(
        "version: 1\n\
         filesystem_policy:\n  \
           include_workdir: true\n  \
           read_only:\n    \
             - /usr\n    \
             - /lib\n    \
             - /proc\n    \
             - /dev/urandom\n    \
             - /etc\n    \
             - /var/log\n  \
           read_write:\n    \
             - /sandbox\n    \
             - /tmp\n    \
             - /dev/null\n",
    );
    if req.worktree.is_some() {
        let _ = writeln!(yaml, "    - {WORKTREE_MOUNT}");
    }
    yaml.push_str("landlock:\n  compatibility: best_effort\nnetwork_policies: {}\n");
    yaml
}

#[derive(Debug, Clone)]
struct EnvState {
    policy_yaml: String,
    policy_path: PathBuf,
    zellij_session: Option<String>,
}

/// OpenShell-backed sandbox provider.
pub struct OpenShellBackend {
    id: BackendId,
    control: Arc<dyn OpenShellControl>,
    envs: Mutex<HashMap<EnvironmentId, EnvState>>,
}

impl Default for OpenShellBackend {
    fn default() -> Self {
        Self::new(Arc::new(CliOpenShellControl))
    }
}

impl OpenShellBackend {
    /// Create the backend with a given host control surface.
    #[must_use]
    pub fn new(control: Arc<dyn OpenShellControl>) -> Self {
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

    /// The generated policy confining the given environment, if it exists — what
    /// `sandbox create --policy` was driven by, auditable by the operator.
    #[must_use]
    pub fn policy_for(&self, env: &EnvironmentId) -> Option<String> {
        self.envs
            .lock()
            .expect("poisoned")
            .get(env)
            .map(|s| s.policy_yaml.clone())
    }

    /// The sandbox name the OpenShell CLI addresses for this environment.
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

    async fn delete_sandbox(&self, env: &EnvironmentId) {
        let _ = self
            .control
            .run(&args(&["sandbox", "delete", &Self::sandbox_name(env)]))
            .await;
    }
}

fn args(strs: &[&str]) -> Vec<String> {
    strs.iter().map(|s| (*s).to_string()).collect()
}

/// `--memory` accepts human units (`512Mi`, `4Gi`); map a byte ceiling to whole MiB,
/// rounding up so the limit is never tightened past what the operator granted.
fn memory_flag(bytes: u64) -> String {
    format!("{}Mi", bytes.div_ceil(1024 * 1024))
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

        let policy_yaml = session_policy_yaml(&req);
        let limits = req.limits;
        let env = req.into_ready_environment(self.id);
        let name = Self::sandbox_name(&env.id);

        // The policy rides a file handed to `--policy`; kept for the sandbox's lifetime
        // so the confinement in force stays auditable on disk too.
        let policy_path = std::env::temp_dir().join(format!("{name}.policy.yaml"));
        std::fs::write(&policy_path, &policy_yaml)
            .map_err(|e| BackendError::ProvisionFailed(format!("could not write policy: {e}")))?;

        let mut create = args(&["sandbox", "create", "--name", &name]);
        create.push("--policy".into());
        create.push(policy_path.to_string_lossy().into_owned());
        if let Some(cpu) = limits.cpu_cores {
            create.push("--cpu".into());
            create.push(cpu.to_string());
        }
        if let Some(mem) = limits.memory_bytes {
            create.push("--memory".into());
            create.push(memory_flag(mem));
        }
        if self.control.gpu_ready() {
            create.push("--gpu".into());
        }
        // Non-interactive: never auto-create credential providers (FR-031 — the operator
        // pre-provisions secrets); no TTY; the initial command exits immediately and the
        // sandbox stays ready for `start_agent` (create keeps sandboxes by default).
        create.extend(args(&["--no-auto-providers", "--no-tty", "--", "true"]));
        // NOTE: wall-clock (`time_secs`) has no create flag; the core enforces it
        // (FR-025). `disk_bytes` likewise has no CLI mapping yet (issue #9).

        let result = self.control.run(&create).await;
        let failed = match &result {
            Err(e) => Some(e.clone()),
            Ok(out) if !out.success => Some(if out.stderr.is_empty() {
                "sandbox create failed".to_string()
            } else {
                out.stderr.trim().to_string()
            }),
            Ok(_) => None,
        };
        if let Some(reason) = failed {
            // Failure leaves NO environment behind (C-B2).
            let _ = std::fs::remove_file(&policy_path);
            return Err(BackendError::ProvisionFailed(reason));
        }

        self.envs.lock().expect("poisoned").insert(
            env.id,
            EnvState {
                policy_yaml,
                policy_path,
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
        if !self.envs.lock().expect("poisoned").contains_key(env) {
            return Err(BackendError::NotFound);
        }
        let zellij_session = daedalus_zellij::format_session_name(env);

        // Launch the agent inside the sandbox, under zellij (FR-004, FR-011):
        //   openshell sandbox exec -n <name> --no-tty -- zellij --session <s> -- <tool>…
        let mut exec = args(&["sandbox", "exec", "-n", &Self::sandbox_name(env)]);
        exec.extend(args(&["--no-tty", "--", "zellij", "--session"]));
        exec.push(zellij_session.clone());
        exec.push("--".into());
        exec.push(tool.program.clone());
        exec.extend(tool.args.iter().cloned());

        if let Err(e) = self.control.spawn(&exec) {
            // A failed start tears the environment down — no orphan (FR-005, C-B2),
            // including the real sandbox.
            let removed = self.envs.lock().expect("poisoned").remove(env);
            self.delete_sandbox(env).await;
            if let Some(state) = removed {
                let _ = std::fs::remove_file(&state.policy_path);
            }
            return Err(BackendError::StartFailed(e));
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
        // The CLI exposes no stats surface yet (0.0.77); report time-only until the
        // driver-level metrics path is confirmed (issue #9 open questions, FR-019).
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
        if let Some(state) = removed {
            self.delete_sandbox(env).await;
            let _ = std::fs::remove_file(&state.policy_path);
        }
        Ok(())
    }
}
