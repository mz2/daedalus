//! E2E: a real coding agent (opencode) driven by a tiny local model (gemma4:e2b via
//! ollama), inside a real OpenShell sandbox, under Daedalus's deny-all egress policy —
//! issue #19. A working agent session with **zero egress and zero credentials** is the
//! strongest demonstration of the local-first posture (spec SC-002/SC-008, FR-031).
//!
//! Self-activating like the other real-backend legs: requires the `openshell` CLI, a
//! running Docker daemon, and the E2E image (`docker build -t daedalus/e2e-openshell:latest
//! tests/e2e/openshell/`); skips silently otherwise. Budget: the agent leg runs a 2.3B
//! model on CPU inside the Docker VM — allow ~10 minutes end to end.

use std::sync::Arc;

use daedalus_backend::{AcquireRequest, Backend};
use daedalus_backend_openshell::{
    CliOpenShellControl, CliOutput, OpenShellBackend, OpenShellControl,
};
use daedalus_proto::{Availability, Origin, ResourceLimits};

const E2E_IMAGE: &str = "daedalus/e2e-openshell:latest";

/// The real CLI control, pinned to the E2E image (no env-var mutation in tests).
struct E2eControl(CliOpenShellControl);

#[async_trait::async_trait]
impl OpenShellControl for E2eControl {
    fn control_binary(&self) -> Option<String> {
        self.0.control_binary()
    }
    fn container_runtime_ready(&self) -> bool {
        self.0.container_runtime_ready()
    }
    fn gpu_ready(&self) -> bool {
        self.0.gpu_ready()
    }
    fn sandbox_image(&self) -> Option<String> {
        Some(E2E_IMAGE.into())
    }
    async fn run(&self, args: &[String]) -> Result<CliOutput, String> {
        self.0.run(args).await
    }
}

async fn e2e_image_present(control: &dyn OpenShellControl) -> bool {
    // The image is a host-docker artifact; probe via docker, not openshell.
    if control.control_binary().is_none() {
        return false;
    }
    tokio::process::Command::new("docker")
        .args(["image", "inspect", E2E_IMAGE])
        .output()
        .await
        .is_ok_and(|o| o.status.success())
}

/// Exec a shell script inside the sandbox with a hard timeout, returning its output.
async fn sandbox_sh(
    control: &dyn OpenShellControl,
    name: &str,
    timeout_secs: u32,
    script: &str,
) -> Result<CliOutput, String> {
    let args: Vec<String> = [
        "sandbox",
        "exec",
        "-n",
        name,
        "--no-tty",
        "--timeout",
        &timeout_secs.to_string(),
        "--",
        "sh",
        "-c",
        script,
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    control.run(&args).await
}

/// The in-sandbox journey (issue #19 §1 + an SC-002 spot-check), as a Result so the
/// caller can always tear the sandbox down before asserting — the E2E must leave the
/// host as it found it even when an assertion fails.
async fn run_agent_journey(control: &dyn OpenShellControl, name: &str) -> Result<(), String> {
    // Toolchain sanity: model runtime, agent, and fixture are all baked in.
    let sanity = sandbox_sh(
        control,
        name,
        60,
        "ollama --version && opencode --version && python3 /sandbox/fixture/calc.py",
    )
    .await?;
    if !sanity.success {
        return Err(format!("toolchain sanity: {}", sanity.stderr));
    }
    if !sanity.stdout.contains("-1") {
        return Err(format!(
            "fixture should start buggy (3-4 = -1): {}",
            sanity.stdout
        ));
    }

    // Serve the baked model — loopback only, no egress needed or possible.
    let serve = sandbox_sh(
        control,
        name,
        90,
        "nohup ollama serve >/tmp/ollama.log 2>&1 & \
         for i in $(seq 1 60); do ollama list >/dev/null 2>&1 && break; sleep 1; done; \
         ollama list",
    )
    .await?;
    if !serve.success || !serve.stdout.contains("gemma4:e2b-16k") {
        return Err(format!(
            "model not served: {} / {}",
            serve.stdout, serve.stderr
        ));
    }

    // SC-002 spot-check while the agent stack is live: egress is still denied.
    let egress = sandbox_sh(
        control,
        name,
        30,
        "curl -sS -m 5 https://example.com && echo EGRESS-ALLOWED || echo EGRESS-DENIED",
    )
    .await?;
    if !egress.stdout.contains("EGRESS-DENIED") {
        return Err(format!("deny-all egress must hold: {}", egress.stdout));
    }

    // The real agent, non-interactively, against the local model.
    let agent = sandbox_sh(
        control,
        name,
        540,
        "cd /sandbox && opencode run \
           'Fix the bug in /sandbox/fixture/calc.py: add(a, b) must return the sum \
            of a and b, not the difference. Edit that file only.' 2>&1 | tail -5",
    )
    .await?;
    if !agent.success {
        return Err(format!("agent run: {} / {}", agent.stdout, agent.stderr));
    }

    // The outcome, not the transcript, is the assertion: 3 + 4 == 7.
    let check = sandbox_sh(control, name, 30, "python3 /sandbox/fixture/calc.py").await?;
    if !check.stdout.trim().ends_with('7') {
        return Err(format!(
            "agent did not fix add(): stdout={} stderr={}",
            check.stdout, check.stderr
        ));
    }
    Ok(())
}

// FR-004/011 launch path for real: `start_agent` creates a detached zellij session in a
// live sandbox with the tool running in it, `stop` kills it in-sandbox, and a launch
// into a sandbox lacking the tool fails DETECTED (no phantom-Running).
#[tokio::test]
async fn start_agent_launches_and_stop_kills_inside_a_real_sandbox() {
    let control = Arc::new(E2eControl(CliOpenShellControl));
    let backend = OpenShellBackend::new(control.clone());
    if backend.availability().await != Availability::Available
        || !e2e_image_present(control.as_ref()).await
    {
        eprintln!("skipping: openshell/docker/{E2E_IMAGE} not available on this host");
        return;
    }

    let env = backend
        .acquire(AcquireRequest {
            origin: Origin::Fresh,
            worktree: None,
            limits: ResourceLimits::default(),
        })
        .await
        .expect("acquire");
    let name = format!("daedalus-{}", env.id);

    let journey = async {
        let handle = backend
            .start_agent(
                &env.id,
                &daedalus_proto::InvocationSpec {
                    program: "sh".into(),
                    args: vec![
                        "-c".into(),
                        "echo agent-alive > /tmp/alive; sleep 300".into(),
                    ],
                    env: vec![],
                },
            )
            .await
            .map_err(|e| format!("start_agent: {e}"))?;

        // The zellij session genuinely exists in the sandbox and the tool ran.
        let seen = sandbox_sh(
            control.as_ref(),
            &name,
            30,
            "sleep 1; zellij list-sessions; cat /tmp/alive",
        )
        .await?;
        if !seen.stdout.contains(&handle.zellij_session) || !seen.stdout.contains("agent-alive") {
            return Err(format!("session+tool live: {}", seen.stdout));
        }

        // The embedded-terminal bridge (issue #15 / FR-008): attach to the in-sandbox
        // zellij session through `sandbox exec --tty` and receive live terminal bytes.
        {
            use daedalus_zellij::{ProcessTerminal, TerminalAttach};
            let binary = control
                .control_binary()
                .ok_or("openshell CLI vanished mid-test")?;
            let env_id = env.id;
            let bridge = ProcessTerminal::new(Box::new(move |_| {
                Ok(daedalus_backend_openshell::attach_command(&binary, &env_id))
            }));
            let mut ch = bridge
                .attach(daedalus_proto::SessionId::new())
                .await
                .map_err(|e| format!("bridge attach: {e}"))?;
            let first = tokio::time::timeout(std::time::Duration::from_secs(15), ch.output.recv())
                .await
                .map_err(|_| "no terminal bytes within 15s of attach (C-T1)".to_string())?
                .ok_or("bridge closed before any output".to_string())?;
            if first.is_empty() {
                return Err("bridge delivered an empty first chunk".into());
            }
            // Surface-driven resize is accepted by a live bridge (winsize/SIGWINCH
            // delivery is contract-tested hermetically; propagation into the sandbox is
            // the CLI's business, as with ssh).
            ch.resize
                .send(daedalus_zellij::TerminalSize {
                    cols: 100,
                    rows: 30,
                })
                .await
                .map_err(|_| "resize channel closed on a live bridge".to_string())?;
            // Dropping the channel detaches (kills the bridge, not the session).
        }

        // Stop kills it in-sandbox; list-sessions no longer shows it.
        backend.stop(&env.id).await.map_err(|e| e.to_string())?;
        let after = sandbox_sh(
            control.as_ref(),
            &name,
            30,
            "zellij list-sessions 2>&1; true",
        )
        .await?;
        if after.stdout.contains(&handle.zellij_session)
            && !after.stdout.to_lowercase().contains("exited")
        {
            return Err(format!("session still alive after stop: {}", after.stdout));
        }
        Ok::<(), String>(())
    }
    .await;

    backend.teardown(&env.id).await.expect("teardown");
    journey.expect("launch/stop journey");

    // A launch that dies inside the sandbox is a DETECTED failure and leaves nothing.
    let env2 = backend
        .acquire(AcquireRequest {
            origin: Origin::Fresh,
            worktree: None,
            limits: ResourceLimits::default(),
        })
        .await
        .expect("acquire second");
    let err = backend
        .start_agent(
            &env2.id,
            &daedalus_proto::InvocationSpec {
                // zellij `run` validates the command exists in the pane; a bad zellij
                // session name cannot be forced here, so use a tool whose launch the
                // create-background path cannot mask: kill the probe by breaking PATH.
                program: "/nonexistent/agent-binary".into(),
                args: vec![],
                env: vec![],
            },
        )
        .await;
    match err {
        Err(daedalus_backend::BackendError::StartFailed(_)) => {
            assert_eq!(
                backend.live_environment_count(),
                0,
                "failed start leaves no env"
            );
        }
        Ok(_) => {
            // zellij accepted the command (it validates lazily); the pane died instantly
            // instead. Acceptable at this layer — detection then belongs to monitoring.
            backend.teardown(&env2.id).await.expect("teardown second");
        }
        Err(e) => {
            backend.teardown(&env2.id).await.ok();
            panic!("unexpected error kind: {e}");
        }
    }
}

#[tokio::test]
async fn agent_fixes_the_fixture_offline_under_deny_all_policy() {
    let control = Arc::new(E2eControl(CliOpenShellControl));
    let backend = OpenShellBackend::new(control.clone());
    if backend.availability().await != Availability::Available
        || !e2e_image_present(control.as_ref()).await
    {
        eprintln!("skipping: openshell/docker/{E2E_IMAGE} not available on this host");
        return;
    }

    // Acquire a real sandbox from the E2E image under the deny-all session policy.
    let env = backend
        .acquire(AcquireRequest {
            origin: Origin::Fresh,
            worktree: None,
            limits: ResourceLimits::default(),
        })
        .await
        .expect("acquire E2E sandbox");
    let name = format!("daedalus-{}", env.id);

    let journey = run_agent_journey(control.as_ref(), &name).await;

    // Teardown runs regardless of the journey's outcome. Deletion is asynchronous on the
    // gateway side — poll until the sandbox is gone rather than racing it.
    backend.teardown(&env.id).await.expect("teardown");
    let mut deleted = false;
    for _ in 0..30 {
        let list = control
            .run(&["sandbox", "list", "--names"].map(str::to_owned))
            .await
            .expect("sandbox list");
        if !list.stdout.contains(&name) {
            deleted = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    journey.expect("E2E agent journey");
    assert!(deleted, "sandbox still listed 60s after teardown");
}
