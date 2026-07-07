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
    fn spawn(&self, args: &[String]) -> Result<(), String> {
        self.0.spawn(args)
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
