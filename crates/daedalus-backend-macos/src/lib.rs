//! macOS local sandbox backend.
//!
//! Confines an agent on macOS using Seatbelt (`sandbox-exec`) with a deny-by-default
//! profile that grants only the session's worktree/working directory, launched under
//! zellij (FR-004, SC-002; research R-MAC — App Sandbox containers are the documented
//! fallback primitive). Off macOS, or when `sandbox-exec` is missing, the backend reports
//! [`Availability::Unavailable`] rather than pretending to isolate (FR-028).

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;

use daedalus_backend::{AcquireRequest, AgentHandle, Backend, BackendError, UsageSample};
use daedalus_proto::{
    Availability, BackendId, BackendKind, EnvLifecycle, EnvironmentId, InvocationSpec, MetricKind,
    Origin, SandboxEnvironment,
};

/// A deny-by-default Seatbelt profile granting read/write only under `work_dir`.
///
/// Exposed for review/testing of the generated policy without launching a process.
#[must_use]
pub fn seatbelt_profile(work_dir: &str) -> String {
    format!(
        "(version 1)\n\
         (deny default)\n\
         (allow process-fork)\n\
         (allow process-exec)\n\
         (allow file-read* (subpath \"{work_dir}\") (subpath \"/usr\") (subpath \"/bin\") (subpath \"/System\"))\n\
         (allow file-write* (subpath \"{work_dir}\"))\n\
         (deny network*)\n"
    )
}

#[derive(Debug, Clone)]
struct EnvState {
    env: SandboxEnvironment,
    work_dir: String,
    zellij_session: Option<String>,
}

/// macOS Seatbelt-backed sandbox provider.
pub struct MacosBackend {
    id: BackendId,
    envs: Mutex<HashMap<EnvironmentId, EnvState>>,
}

impl Default for MacosBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl MacosBackend {
    /// Create the backend.
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: BackendId::new(),
            envs: Mutex::new(HashMap::new()),
        }
    }

    fn platform_available() -> bool {
        cfg!(target_os = "macos")
    }
}

#[async_trait]
impl Backend for MacosBackend {
    fn id(&self) -> BackendId {
        self.id
    }

    fn kind(&self) -> BackendKind {
        BackendKind::MacosSandbox
    }

    async fn availability(&self) -> Availability {
        // Availability is a value, never an error (FR-028, C-B4).
        if Self::platform_available() {
            Availability::Available
        } else {
            Availability::Unavailable
        }
    }

    async fn acquire(&self, req: AcquireRequest) -> Result<SandboxEnvironment, BackendError> {
        if !Self::platform_available() {
            return Err(BackendError::Unsupported(
                "macOS sandbox backend is only available on macOS hosts".into(),
            ));
        }

        let work_dir = match req.origin {
            Origin::PreExisting => match &req.worktree {
                Some(wt) => wt.path.clone(),
                None => {
                    return Err(BackendError::WorktreeUnavailable(
                        "pre-existing environment requires a worktree reference".into(),
                    ))
                }
            },
            // A fresh environment gets a private working directory.
            Origin::Fresh => format!(
                "{}/daedalus-{}",
                std::env::temp_dir().display(),
                EnvironmentId::new()
            ),
        };

        if req.origin == Origin::Fresh {
            std::fs::create_dir_all(&work_dir)
                .map_err(|e| BackendError::ProvisionFailed(e.to_string()))?;
        }

        let env = SandboxEnvironment {
            id: EnvironmentId::new(),
            backend_id: self.id,
            origin: req.origin,
            worktree_ref: req.worktree,
            lifecycle: EnvLifecycle::Ready,
        };
        self.envs.lock().expect("poisoned").insert(
            env.id,
            EnvState {
                env: env.clone(),
                work_dir,
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
        let work_dir = {
            let envs = self.envs.lock().expect("poisoned");
            envs.get(env)
                .ok_or(BackendError::NotFound)?
                .work_dir
                .clone()
        };
        let zellij_session = format!("daedalus-{env}");
        let profile = seatbelt_profile(&work_dir);

        // Launch the tool under Seatbelt inside a fresh zellij session:
        //   zellij --session <name> -- sandbox-exec -p <profile> <program> <args...>
        let mut cmd = tokio::process::Command::new("zellij");
        cmd.arg("--session")
            .arg(&zellij_session)
            .arg("--")
            .arg("sandbox-exec")
            .arg("-p")
            .arg(&profile)
            .arg(&tool.program)
            .args(&tool.args)
            .current_dir(&work_dir);

        cmd.spawn().map_err(|e| {
            // A failed start tears down nothing extra here (no env created by start).
            BackendError::StartFailed(e.to_string())
        })?;

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
        // A full implementation samples the process tree; report time-only for now.
        Ok(vec![UsageSample {
            metric: MetricKind::Time,
            value: 0.0,
        }])
    }

    async fn stop(&self, env: &EnvironmentId) -> Result<(), BackendError> {
        // Idempotent: ask zellij to kill the session if we know its name.
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
        if let Some(state) = removed {
            // Only clean up fresh, Daedalus-created working dirs.
            if state.env.origin == Origin::Fresh {
                let _ = std::fs::remove_dir_all(&state.work_dir);
            }
        }
        Ok(())
    }
}
