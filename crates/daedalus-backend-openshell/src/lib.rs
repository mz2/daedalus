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
    /// Operator-configured sandbox image for `sandbox create --from` (a community image
    /// name, a full container image reference, or a Dockerfile path). `None` uses
    /// OpenShell's default base image.
    fn sandbox_image(&self) -> Option<String> {
        None
    }
    /// Run a control-plane command (`sandbox create`/`delete`/`exec`, …) to completion.
    /// The agent itself is launched *detached inside the sandbox* (a background zellij
    /// session or `nohup`), so even launch commands run to completion here — a failed
    /// launch is a detected error, never a phantom-running session.
    async fn run(&self, args: &[String]) -> Result<CliOutput, String>;
}

/// Default control resolving the `openshell` CLI from `DAEDALUS_OPENSHELL_CMD` or `PATH`,
/// and probing the container runtime via the `docker` CLI.
#[derive(Default)]
pub struct CliOpenShellControl {
    /// Operator-configured image source — the app's persisted Settings value, consulted
    /// before the `DAEDALUS_OPENSHELL_FROM` environment fallback so configuration lives
    /// in the app, not the launching shell.
    image_source: Option<std::sync::Arc<dyn Fn() -> Option<String> + Send + Sync>>,
}

impl CliOpenShellControl {
    /// A control whose sandbox image comes from the given source (e.g. the app's
    /// persisted Settings), with the environment variable as fallback.
    #[must_use]
    pub fn with_image_source(
        source: std::sync::Arc<dyn Fn() -> Option<String> + Send + Sync>,
    ) -> Self {
        Self {
            image_source: Some(source),
        }
    }

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

    fn sandbox_image(&self) -> Option<String> {
        self.image_source
            .as_ref()
            .and_then(|source| source())
            .or_else(|| {
                std::env::var("DAEDALUS_OPENSHELL_FROM")
                    .ok()
                    .filter(|s| !s.is_empty())
            })
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

/// The bridge command whose stdio is a session's live terminal (issue #15 / FR-008):
/// attach to the agent's in-sandbox zellij session, with `--tty` forcing the PTY the
/// bridge needs when its own stdio is piped (zellij refuses to run without one).
/// Feed this to a [`daedalus_zellij::ProcessTerminal`] resolver.
#[must_use]
pub fn attach_command(control_binary: &str, env: &EnvironmentId) -> Vec<String> {
    vec![
        control_binary.to_string(),
        "sandbox".into(),
        "exec".into(),
        "--tty".into(),
        "-n".into(),
        format!("daedalus-{env}"),
        "--".into(),
        "zellij".into(),
        "attach".into(),
        daedalus_zellij::format_session_name(env),
    ]
}

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

/// How the agent was launched inside the sandbox — determines how `stop` reaches it.
#[derive(Debug, Clone)]
enum LaunchMode {
    /// Detached zellij session of this name (the attachable path, FR-011).
    Zellij(String),
    /// `nohup`-detached process (zellij absent from the image); stopped by program name.
    Direct(String),
}

#[derive(Debug, Clone)]
struct EnvState {
    policy_yaml: String,
    policy_path: PathBuf,
    launch: Option<LaunchMode>,
}

/// OpenShell-backed sandbox provider.
pub struct OpenShellBackend {
    id: BackendId,
    control: Arc<dyn OpenShellControl>,
    envs: Mutex<HashMap<EnvironmentId, EnvState>>,
}

impl Default for OpenShellBackend {
    fn default() -> Self {
        Self::new(Arc::new(CliOpenShellControl::default()))
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

    /// Run a bounded, non-interactive command inside the sandbox. Always carries
    /// `--timeout` — the CLI otherwise waits on stdin when driven non-interactively.
    async fn exec_in(
        &self,
        env: &EnvironmentId,
        timeout_secs: u32,
        tail: &[String],
    ) -> Result<CliOutput, String> {
        let mut cmd = args(&["sandbox", "exec", "-n", &Self::sandbox_name(env)]);
        cmd.extend(args(&["--no-tty", "--timeout"]));
        cmd.push(timeout_secs.to_string());
        cmd.push("--".into());
        cmd.extend(tail.iter().cloned());
        self.control.run(&cmd).await
    }

    /// Tear everything down after a failed launch: env record, real sandbox, policy file
    /// (FR-005, C-B2 — no orphan, no phantom-running session).
    async fn abort_start(&self, env: &EnvironmentId, reason: String) -> BackendError {
        let removed = self.envs.lock().expect("poisoned").remove(env);
        self.delete_sandbox(env).await;
        if let Some(state) = removed {
            let _ = std::fs::remove_file(&state.policy_path);
        }
        BackendError::StartFailed(reason)
    }
}

/// Single-quote a word for `sh -c` (the direct-exec fallback launch line).
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Flatten an exec result into `Err(reason)` unless the command genuinely succeeded.
fn require_success(result: Result<CliOutput, String>, what: &str) -> Result<CliOutput, String> {
    match result {
        Err(e) => Err(format!("{what}: {e}")),
        Ok(out) if !out.success => Err(if out.stderr.trim().is_empty() {
            format!("{what} failed")
        } else {
            format!("{what}: {}", out.stderr.trim())
        }),
        Ok(out) => Ok(out),
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
        match self.control.sandbox_image() {
            Some(image) => {
                tracing::info!("openshell: creating sandbox {name} from image {image}");
                create.push("--from".into());
                create.push(image);
            }
            // The default base image carries no multiplexer or agent toolchain — say so
            // up front instead of letting the attach discover it (operator trap seen
            // 2026-07-08: DAEDALUS_OPENSHELL_FROM unset in the app's environment).
            None => tracing::warn!(
                "openshell: creating sandbox {name} from the DEFAULT base image \
                 (no zellij/agent tools) — set DAEDALUS_OPENSHELL_FROM to change this"
            ),
        }
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
                launch: None,
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

        // Every launch step runs to completion — a dead exec is a detected StartFailed,
        // never a phantom-running session. Does the image carry zellij (FR-011)?
        let probe = self
            .exec_in(env, 15, &args(&["sh", "-c", "command -v zellij"]))
            .await;
        let launch = match probe {
            Err(e) => {
                return Err(self
                    .abort_start(env, format!("sandbox unreachable for launch: {e}"))
                    .await)
            }
            Ok(out) if out.success && !out.stdout.trim().is_empty() => {
                // Attachable path (FR-004/011), verified against zellij 0.44:
                // a detached background session, then the tool in a pane of it.
                let create = self
                    .exec_in(
                        env,
                        30,
                        &args(&["zellij", "attach", "--create-background", &zellij_session]),
                    )
                    .await;
                if let Err(reason) = require_success(create, "zellij session create") {
                    return Err(self.abort_start(env, reason).await);
                }
                // The pane states what is running before the tool draws anything —
                // otherwise a slow-starting agent looks like an anonymous spinner.
                let display = std::iter::once(tool.program.as_str())
                    .chain(tool.args.iter().map(String::as_str))
                    .collect::<Vec<_>>()
                    .join(" ");
                let words: Vec<String> = std::iter::once(&tool.program)
                    .chain(tool.args.iter())
                    .map(|w| sh_quote(w))
                    .collect();
                let script = format!(
                    "echo {}; exec {}",
                    sh_quote(&format!("[daedalus] launching: {display}")),
                    words.join(" ")
                );
                let mut run_tool = args(&["zellij", "--session", &zellij_session, "run", "--"]);
                run_tool.extend(args(&["sh", "-c"]));
                run_tool.push(script);
                if let Err(reason) =
                    require_success(self.exec_in(env, 30, &run_tool).await, "agent launch")
                {
                    return Err(self.abort_start(env, reason).await);
                }
                LaunchMode::Zellij(zellij_session.clone())
            }
            Ok(_) => {
                // No zellij in the image: the agent still launches (nohup-detached, output
                // to a log); the attach layer states why it cannot attach (FR-028-style
                // honesty) rather than the session failing outright.
                let words: Vec<String> = std::iter::once(&tool.program)
                    .chain(tool.args.iter())
                    .map(|w| sh_quote(w))
                    .collect();
                let script = format!(
                    "nohup {} >>/sandbox/.daedalus-agent.log 2>&1 & echo daedalus-launched",
                    words.join(" ")
                );
                let launch = self.exec_in(env, 30, &args(&["sh", "-c", &script])).await;
                match require_success(launch, "agent launch (direct)") {
                    Ok(out) if out.stdout.contains("daedalus-launched") => {}
                    Ok(_) => {
                        return Err(self
                            .abort_start(env, "agent launch (direct): no confirmation".into())
                            .await)
                    }
                    Err(reason) => return Err(self.abort_start(env, reason).await),
                }
                LaunchMode::Direct(tool.program.clone())
            }
        };

        if let Some(state) = self.envs.lock().expect("poisoned").get_mut(env) {
            state.launch = Some(launch);
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
        // Idempotent (C-B5): stopping an unknown / already-stopped env is Ok. The agent
        // lives INSIDE the sandbox, so stop reaches it through `sandbox exec`.
        let launch = self
            .envs
            .lock()
            .expect("poisoned")
            .get_mut(env)
            .and_then(|s| s.launch.take());
        match launch {
            Some(LaunchMode::Zellij(session)) => {
                let _ = self
                    .exec_in(env, 30, &args(&["zellij", "kill-session", &session]))
                    .await;
            }
            Some(LaunchMode::Direct(program)) => {
                let script = format!("pkill -f {} || true", sh_quote(&program));
                let _ = self.exec_in(env, 30, &args(&["sh", "-c", &script])).await;
            }
            None => {}
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
