//! Canonical Workshop (Linux) backend.
//!
//! Drives Workshop's control interface to provision/attach an isolated environment and
//! launch the agent inside it under zellij (FR-004, FR-027). The exact control surface is
//! confirmed against Workshop during implementation (research R-WS); it is modelled here
//! behind [`WorkshopControl`] so the binary/endpoint is configurable and the rest of the
//! backend is testable. When no control interface is reachable, the backend reports
//! [`Availability::Unavailable`] (FR-028) rather than failing.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;

use daedalus_backend::{
    require_worktree, AcquireRequest, AgentHandle, Backend, BackendError, UsageSample,
};
use daedalus_proto::{
    Availability, BackendId, BackendKind, EnvironmentId, InvocationSpec, MetricKind,
    SandboxEnvironment,
};

/// The Workshop control interface Daedalus drives. Defaulted to a CLI named by
/// `DAEDALUS_WORKSHOP_CMD` (or `workshop` on `PATH`); pluggable for tests / future
/// API transports.
pub trait WorkshopControl: Send + Sync {
    /// Name/path of the control binary, if one is configured and present.
    fn control_binary(&self) -> Option<String>;
}

/// Default control that resolves a `workshop` CLI from the environment / `PATH`.
#[derive(Debug, Default)]
pub struct CliWorkshopControl;

impl WorkshopControl for CliWorkshopControl {
    fn control_binary(&self) -> Option<String> {
        if let Ok(cmd) = std::env::var("DAEDALUS_WORKSHOP_CMD") {
            if !cmd.is_empty() {
                return Some(cmd);
            }
        }
        // Resolve `workshop` on PATH (Linux Workshop hosts).
        if cfg!(target_os = "linux") {
            which_on_path("workshop")
        } else {
            None
        }
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

#[derive(Debug, Clone)]
struct EnvState {
    zellij_session: Option<String>,
}

/// Workshop-backed sandbox provider.
pub struct WorkshopBackend {
    id: BackendId,
    control: Box<dyn WorkshopControl>,
    envs: Mutex<HashMap<EnvironmentId, EnvState>>,
}

impl Default for WorkshopBackend {
    fn default() -> Self {
        Self::new(Box::new(CliWorkshopControl))
    }
}

impl WorkshopBackend {
    /// Create the backend with a given control interface.
    #[must_use]
    pub fn new(control: Box<dyn WorkshopControl>) -> Self {
        Self {
            id: BackendId::new(),
            control,
            envs: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl Backend for WorkshopBackend {
    fn id(&self) -> BackendId {
        self.id
    }

    fn kind(&self) -> BackendKind {
        BackendKind::Workshop
    }

    async fn availability(&self) -> Availability {
        // Availability is a value, never an error (FR-028, C-B4).
        if self.control.control_binary().is_some() {
            Availability::Available
        } else {
            Availability::Unavailable
        }
    }

    async fn acquire(&self, req: AcquireRequest) -> Result<SandboxEnvironment, BackendError> {
        if self.control.control_binary().is_none() {
            return Err(BackendError::ProvisionFailed(
                "no reachable Workshop control interface (set DAEDALUS_WORKSHOP_CMD)".into(),
            ));
        }
        require_worktree(&req)?;

        // A real implementation calls `workshop create …` here and parses the env handle.
        let env = req.into_ready_environment(self.id);
        self.envs.lock().expect("poisoned").insert(
            env.id,
            EnvState {
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
            .ok_or_else(|| BackendError::StartFailed("Workshop control interface gone".into()))?;
        {
            let envs = self.envs.lock().expect("poisoned");
            if !envs.contains_key(env) {
                return Err(BackendError::NotFound);
            }
        }
        let zellij_session = daedalus_zellij::format_session_name(env);

        // Launch the agent inside the Workshop env, under zellij, via the control binary:
        //   <control> exec --env <id> -- zellij --session <name> -- <program> <args...>
        let mut cmd = tokio::process::Command::new(&control);
        cmd.arg("exec")
            .arg("--env")
            .arg(env.to_string())
            .arg("--")
            .arg("zellij")
            .arg("--session")
            .arg(&zellij_session)
            .arg("--")
            .arg(&tool.program)
            .args(&tool.args);

        cmd.spawn()
            .map_err(|e| BackendError::StartFailed(e.to_string()))?;

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
        // A full implementation queries Workshop's metrics; report time-only for now.
        Ok(vec![UsageSample {
            metric: MetricKind::Time,
            value: 0.0,
        }])
    }

    async fn stop(&self, env: &EnvironmentId) -> Result<(), BackendError> {
        let session = self
            .envs
            .lock()
            .expect("poisoned")
            .get(env)
            .and_then(|s| s.zellij_session.clone());
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
        let removed = self.envs.lock().expect("poisoned").remove(env);
        if removed.is_some() {
            if let Some(control) = self.control.control_binary() {
                // A real implementation calls `workshop destroy --env <id>`.
                let _ = tokio::process::Command::new(&control)
                    .arg("destroy")
                    .arg("--env")
                    .arg(env.to_string())
                    .output()
                    .await;
            }
        }
        Ok(())
    }
}
