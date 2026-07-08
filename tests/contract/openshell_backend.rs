//! Contract tests for the NVIDIA OpenShell backend (issue #9): C-B1/C-B2/C-B4/C-B5 against
//! a scripted control, plus the per-session policy and CLI-invocation guarantees — the
//! worktree's sandbox path writable (FR-002a), deny-all egress (local-first posture; empty
//! `network_policies`), no credential/env passthrough (FR-031), and GPU gated on host
//! readiness (Linux hosts only).
//!
//! The CLI surface asserted here is the real one, confirmed against openshell 0.0.77 on
//! macOS/Apple silicon (2026-07-07): `sandbox create --name … --policy … [--cpu/--memory/
//! --gpu] --no-auto-providers -- …`, `sandbox exec -n … --no-tty -- …`,
//! `sandbox delete <name>`, and the `filesystem_policy`/`network_policies`/`landlock`
//! policy schema reported by `openshell policy get --base -o json`.

use std::sync::{Arc, Mutex};

use daedalus_backend::{AcquireRequest, Backend, BackendError};
use daedalus_backend_openshell::{
    session_policy_yaml, CliOutput, OpenShellBackend, OpenShellControl,
};
use daedalus_proto::{Availability, BackendKind, Origin, ResourceLimits, WorktreeRef};

/// A control whose host state is scripted and whose invocations are recorded, so every
/// availability tier, CLI call shape, and failure path is exercisable without an
/// `openshell` CLI or Docker on the test host.
#[derive(Default)]
struct ScriptedControl {
    binary: Option<String>,
    runtime_ready: bool,
    gpu_ready: bool,
    sandbox_image: Option<String>,
    /// Whether the sandbox image carries zellij (the `command -v zellij` probe).
    zellij_in_image: bool,
    fail_run: Option<String>,
    /// When set, the zellij session launch (`attach --create-background`) fails.
    fail_zellij_launch: bool,
    calls: Mutex<Vec<Vec<String>>>,
}

impl ScriptedControl {
    /// A fully healthy OpenShell host (CLI + container runtime, no GPU) whose sandbox
    /// image carries zellij.
    fn ready() -> Self {
        Self {
            binary: Some("/opt/homebrew/bin/openshell".into()),
            runtime_ready: true,
            zellij_in_image: true,
            ..Self::default()
        }
    }

    /// Recorded invocations starting with the given subcommand words.
    fn recorded(&self, subcommand: &[&str]) -> Vec<Vec<String>> {
        let prefix: Vec<String> = subcommand.iter().map(|s| (*s).to_string()).collect();
        self.calls
            .lock()
            .expect("poisoned")
            .iter()
            .filter(|c| c.starts_with(&prefix))
            .cloned()
            .collect()
    }
}

#[async_trait::async_trait]
impl OpenShellControl for ScriptedControl {
    fn control_binary(&self) -> Option<String> {
        self.binary.clone()
    }
    fn container_runtime_ready(&self) -> bool {
        self.runtime_ready
    }
    fn gpu_ready(&self) -> bool {
        self.gpu_ready
    }
    fn sandbox_image(&self) -> Option<String> {
        self.sandbox_image.clone()
    }
    async fn run(&self, args: &[String]) -> Result<CliOutput, String> {
        self.calls.lock().expect("poisoned").push(args.to_vec());
        if let Some(reason) = &self.fail_run {
            return Err(reason.clone());
        }
        let joined = args.join(" ");
        if joined.contains("command -v zellij") {
            return Ok(CliOutput {
                success: self.zellij_in_image,
                stdout: if self.zellij_in_image {
                    "/usr/local/bin/zellij\n".into()
                } else {
                    String::new()
                },
                stderr: String::new(),
            });
        }
        if self.fail_zellij_launch && joined.contains("--create-background") {
            return Ok(CliOutput {
                success: false,
                stdout: String::new(),
                stderr: "zellij exploded".into(),
            });
        }
        Ok(CliOutput {
            success: true,
            // A successful direct launch echoes its confirmation marker.
            stdout: if joined.contains("daedalus-launched") {
                "daedalus-launched\n".into()
            } else {
                String::new()
            },
            stderr: String::new(),
        })
    }
}

/// Backend plus a kept handle on its scripted control, for invocation assertions.
fn backend_with(control: ScriptedControl) -> (Arc<ScriptedControl>, OpenShellBackend) {
    let control = Arc::new(control);
    (control.clone(), OpenShellBackend::new(control))
}

fn backend(control: ScriptedControl) -> OpenShellBackend {
    backend_with(control).1
}

fn fresh_req() -> AcquireRequest {
    AcquireRequest {
        origin: Origin::Fresh,
        worktree: None,
        limits: ResourceLimits::default(),
    }
}

fn preexisting_req() -> AcquireRequest {
    AcquireRequest {
        origin: Origin::PreExisting,
        worktree: Some(WorktreeRef {
            path: "/work/repo/.worktrees/feat".into(),
            branch: "feat".into(),
        }),
        limits: ResourceLimits::default(),
    }
}

#[test]
fn kind_is_openshell() {
    assert_eq!(
        backend(ScriptedControl::ready()).kind(),
        BackendKind::OpenShell
    );
}

// C-B4: availability is a value, never an error, and states why when not available
// (FR-028) — CLI missing ⇒ unavailable; CLI present but container runtime down (e.g.
// Docker Desktop stopped on macOS) ⇒ degraded.
#[tokio::test]
async fn c_b4_missing_cli_is_unavailable_with_reason() {
    let b = backend(ScriptedControl::default());
    assert_eq!(b.availability().await, Availability::Unavailable);
    let reason = b.availability_reason().await.expect("states a reason");
    assert!(
        reason.contains("openshell"),
        "names the missing CLI: {reason}"
    );
}

#[tokio::test]
async fn c_b4_runtime_down_is_degraded_with_reason() {
    let b = backend(ScriptedControl {
        binary: Some("/opt/homebrew/bin/openshell".into()),
        runtime_ready: false,
        ..ScriptedControl::default()
    });
    assert_eq!(b.availability().await, Availability::Degraded);
    let reason = b.availability_reason().await.expect("states a reason");
    assert!(
        reason.contains("container runtime"),
        "names the runtime: {reason}"
    );
}

#[tokio::test]
async fn c_b4_healthy_host_is_available_without_reason() {
    let b = backend(ScriptedControl::ready());
    assert_eq!(b.availability().await, Availability::Available);
    assert_eq!(b.availability_reason().await, None);
}

// GPU is a capability, not an availability tier: a macOS host (no GPU passthrough) is
// still fully available; only `gpu_capable` differs.
#[tokio::test]
async fn gpu_is_a_capability_not_an_availability_tier() {
    let without = backend(ScriptedControl::ready());
    assert!(!without.gpu_capable());
    assert_eq!(without.availability().await, Availability::Available);

    let with = backend(ScriptedControl {
        gpu_ready: true,
        ..ScriptedControl::ready()
    });
    assert!(with.gpu_capable());
    assert_eq!(with.availability().await, Availability::Available);
}

// C-B1: a pre-existing acquire without a worktree reference fails with
// WorktreeUnavailable and leaves no environment behind (FR-002a).
#[tokio::test]
async fn c_b1_preexisting_without_worktree_fails_and_leaves_nothing() {
    let b = backend(ScriptedControl::ready());
    let req = AcquireRequest {
        origin: Origin::PreExisting,
        worktree: None,
        limits: ResourceLimits::default(),
    };
    let err = b.acquire(req).await.unwrap_err();
    assert!(matches!(err, BackendError::WorktreeUnavailable(_)));
    assert_eq!(b.live_environment_count(), 0, "no orphan environment");
}

// C-B2: acquiring on a host that is not available fails with the stated reason and
// leaves nothing behind.
#[tokio::test]
async fn c_b2_acquire_on_unready_host_fails_and_leaves_nothing() {
    let b = backend(ScriptedControl {
        binary: Some("/opt/homebrew/bin/openshell".into()),
        runtime_ready: false,
        ..ScriptedControl::default()
    });
    let err = b.acquire(fresh_req()).await.unwrap_err();
    assert!(matches!(err, BackendError::ProvisionFailed(_)));
    assert_eq!(b.live_environment_count(), 0);
}

// C-B2: a failed `sandbox create` fails the acquire and leaves nothing behind.
#[tokio::test]
async fn c_b2_failed_create_fails_acquire_and_leaves_nothing() {
    let b = backend(ScriptedControl {
        fail_run: Some("image pull failed".into()),
        ..ScriptedControl::ready()
    });
    let err = b.acquire(fresh_req()).await.unwrap_err();
    assert!(matches!(err, BackendError::ProvisionFailed(_)));
    assert_eq!(b.live_environment_count(), 0);
}

// C-B2: a failed agent launch is DETECTED (the launch runs to completion, it is not
// fire-and-forget), tears the environment down — no orphan, no phantom-Running session
// (FR-005) — and the real sandbox is deleted.
#[tokio::test]
async fn c_b2_failed_start_is_detected_and_tears_the_environment_down() {
    let (control, b) = backend_with(ScriptedControl {
        fail_zellij_launch: true,
        ..ScriptedControl::ready()
    });
    let env = b.acquire(fresh_req()).await.unwrap();
    let tool = daedalus_proto::InvocationSpec {
        program: "echo".into(),
        args: vec![],
        env: vec![],
    };
    let err = b.start_agent(&env.id, &tool).await.unwrap_err();
    assert!(matches!(err, BackendError::StartFailed(_)));
    assert_eq!(
        b.live_environment_count(),
        0,
        "env torn down on failed start"
    );
    assert!(
        !control.recorded(&["sandbox", "delete"]).is_empty(),
        "the real sandbox is deleted on failed start"
    );
}

// C-B5: stop and teardown are idempotent and isolated to one environment (FR-024).
#[tokio::test]
async fn c_b5_stop_and_teardown_are_idempotent_and_isolated() {
    let (control, b) = backend_with(ScriptedControl::ready());
    let a = b.acquire(fresh_req()).await.unwrap();
    let other = b.acquire(preexisting_req()).await.unwrap();

    b.stop(&a.id).await.unwrap();
    b.stop(&a.id).await.unwrap();
    b.teardown(&a.id).await.unwrap();
    b.teardown(&a.id).await.unwrap();

    assert_eq!(b.live_environment_count(), 1, "the other env is untouched");
    assert!(b.policy_for(&other.id).is_some());
    assert_eq!(
        control.recorded(&["sandbox", "delete"]).len(),
        1,
        "repeat teardown does not re-delete"
    );
}

#[tokio::test]
async fn resource_usage_on_unknown_environment_is_not_found() {
    let b = backend(ScriptedControl::ready());
    let ghost = daedalus_proto::EnvironmentId::new();
    assert!(matches!(
        b.resource_usage(&ghost).await.unwrap_err(),
        BackendError::NotFound
    ));
}

// --- Real CLI invocation shape (openshell 0.0.77) ---------------------------------------

// Acquire drives `sandbox create` with a per-session name, the generated policy file,
// provider auto-creation disabled, and — FR-031 — never any `--env` injection.
#[tokio::test]
async fn acquire_issues_sandbox_create_without_env_injection() {
    let (control, b) = backend_with(ScriptedControl::ready());
    let env = b.acquire(fresh_req()).await.unwrap();

    let creates = control.recorded(&["sandbox", "create"]);
    assert_eq!(creates.len(), 1, "exactly one create: {creates:?}");
    let create = &creates[0];
    let name_pos = create.iter().position(|a| a == "--name").expect("--name");
    assert_eq!(create[name_pos + 1], format!("daedalus-{}", env.id));
    assert!(create.contains(&"--policy".to_string()), "{create:?}");
    assert!(
        create.contains(&"--no-auto-providers".to_string()),
        "credentials are pre-provisioned by the operator, never auto-created: {create:?}"
    );
    assert!(
        !create.contains(&"--env".to_string()),
        "no host env/credential injection (FR-031): {create:?}"
    );
}

// Operator resource limits map onto real create flags; unset limits stay absent so
// backend defaults apply. Wall-clock (`time_secs`) has no CLI flag — the core enforces it.
#[tokio::test]
async fn acquire_maps_resource_limits_onto_create_flags() {
    let (control, b) = backend_with(ScriptedControl::ready());
    let mut req = fresh_req();
    req.limits = ResourceLimits {
        cpu_cores: Some(2.5),
        memory_bytes: Some(1_073_741_824),
        disk_bytes: None,
        time_secs: Some(3600),
    };
    b.acquire(req).await.unwrap();

    let create = &control.recorded(&["sandbox", "create"])[0];
    let cpu = create.iter().position(|a| a == "--cpu").expect("--cpu");
    assert_eq!(create[cpu + 1], "2.5");
    let mem = create
        .iter()
        .position(|a| a == "--memory")
        .expect("--memory");
    assert_eq!(create[mem + 1], "1024Mi");
    assert!(
        !create.contains(&"--gpu".to_string()),
        "no GPU on this host"
    );
}

// An operator-configured sandbox image rides `--from` (community name, image reference,
// or Dockerfile path — DAEDALUS_OPENSHELL_FROM on the default control); unset means
// OpenShell's default base image, no flag.
#[tokio::test]
async fn acquire_passes_the_configured_image_via_from() {
    let (control, b) = backend_with(ScriptedControl {
        sandbox_image: Some("daedalus/e2e-openshell:latest".into()),
        ..ScriptedControl::ready()
    });
    b.acquire(fresh_req()).await.unwrap();
    let create = &control.recorded(&["sandbox", "create"])[0];
    let from = create.iter().position(|a| a == "--from").expect("--from");
    assert_eq!(create[from + 1], "daedalus/e2e-openshell:latest");

    let (control, b) = backend_with(ScriptedControl::ready());
    b.acquire(fresh_req()).await.unwrap();
    let create = &control.recorded(&["sandbox", "create"])[0];
    assert!(!create.contains(&"--from".to_string()), "{create:?}");
}

// GPU is requested at create time only when the host is GPU-ready.
#[tokio::test]
async fn acquire_requests_gpu_only_when_host_is_ready() {
    let (control, b) = backend_with(ScriptedControl {
        gpu_ready: true,
        ..ScriptedControl::ready()
    });
    b.acquire(fresh_req()).await.unwrap();
    let create = &control.recorded(&["sandbox", "create"])[0];
    assert!(create.contains(&"--gpu".to_string()), "{create:?}");
}

// The agent launches under zellij inside the sandbox (FR-004/011), synchronously and
// verifiably (openshell 0.0.77 + zellij 0.44, verified live 2026-07-07):
//   1. probe:  sandbox exec … -- sh -c 'command -v zellij'
//   2. create: sandbox exec … -- zellij attach --create-background <session>
//   3. run:    sandbox exec … -- zellij --session <session> run -- <tool> <args…>
// Each step runs to completion; a failure at any step is a detected StartFailed.
#[tokio::test]
async fn start_agent_launches_a_detached_zellij_session_with_the_tool() {
    let (control, b) = backend_with(ScriptedControl::ready());
    let env = b.acquire(fresh_req()).await.unwrap();
    let tool = daedalus_proto::InvocationSpec {
        program: "claude".into(),
        args: vec!["--continue".into()],
        env: vec![],
    };
    let handle = b.start_agent(&env.id, &tool).await.unwrap();
    let name = format!("daedalus-{}", env.id);

    let execs = control.recorded(&["sandbox", "exec"]);
    assert_eq!(execs.len(), 3, "probe + create-background + run: {execs:?}");
    for exec in &execs {
        let n = exec.iter().position(|a| a == "-n").expect("-n flag");
        assert_eq!(exec[n + 1], name);
        assert!(exec.contains(&"--no-tty".to_string()), "{exec:?}");
        assert!(
            exec.contains(&"--timeout".to_string()),
            "control-plane execs are bounded (the CLI hangs on stdin without one): {exec:?}"
        );
    }

    let create = &execs[1];
    let sep = create.iter().position(|a| a == "--").expect("separator");
    assert_eq!(
        &create[sep + 1..],
        &[
            "zellij".to_string(),
            "attach".into(),
            "--create-background".into(),
            handle.zellij_session.clone()
        ]
    );

    let run = &execs[2];
    let joined = run.join(" ");
    assert!(joined.contains("zellij --session"), "{joined}");
    assert!(joined.contains(&handle.zellij_session), "{joined}");
    // The pane announces what is launching, then execs the tool (quoted).
    assert!(
        joined.contains("[daedalus] launching: claude --continue"),
        "{joined}"
    );
    assert!(joined.contains("exec 'claude' '--continue'"), "{joined}");
}

// Without zellij in the image, the tool still launches — direct detached exec (nohup)
// with output to a log; the operator gets a session, just not an attachable one (the
// attach layer states why — FR-028-style honesty, not a silent failure).
#[tokio::test]
async fn start_agent_falls_back_to_direct_exec_without_zellij() {
    let (control, b) = backend_with(ScriptedControl {
        zellij_in_image: false,
        ..ScriptedControl::ready()
    });
    let env = b.acquire(fresh_req()).await.unwrap();
    let tool = daedalus_proto::InvocationSpec {
        program: "opencode".into(),
        args: vec!["run".into(), "fix it".into()],
        env: vec![],
    };
    b.start_agent(&env.id, &tool).await.unwrap();

    let execs = control.recorded(&["sandbox", "exec"]);
    assert_eq!(execs.len(), 2, "probe + direct launch: {execs:?}");
    let launch = execs[1].join(" ");
    assert!(launch.contains("nohup"), "{launch}");
    assert!(launch.contains("opencode"), "{launch}");
    assert!(!launch.contains("zellij"), "{launch}");
}

// Stop kills the agent INSIDE the sandbox (the zellij session lives there, not on the
// host) and stays idempotent (C-B5).
#[tokio::test]
async fn stop_kills_the_zellij_session_inside_the_sandbox() {
    let (control, b) = backend_with(ScriptedControl::ready());
    let env = b.acquire(fresh_req()).await.unwrap();
    let tool = daedalus_proto::InvocationSpec {
        program: "claude".into(),
        args: vec![],
        env: vec![],
    };
    let handle = b.start_agent(&env.id, &tool).await.unwrap();

    b.stop(&env.id).await.unwrap();
    b.stop(&env.id).await.unwrap();

    let kills: Vec<_> = control
        .recorded(&["sandbox", "exec"])
        .into_iter()
        .filter(|c| c.join(" ").contains("kill-session"))
        .collect();
    assert_eq!(kills.len(), 1, "one in-sandbox kill, idempotent: {kills:?}");
    assert!(kills[0].contains(&handle.zellij_session));
}

// Teardown deletes the real sandbox by name.
#[tokio::test]
async fn teardown_deletes_the_sandbox() {
    let (control, b) = backend_with(ScriptedControl::ready());
    let env = b.acquire(fresh_req()).await.unwrap();
    b.teardown(&env.id).await.unwrap();
    let deletes = control.recorded(&["sandbox", "delete"]);
    assert_eq!(deletes.len(), 1);
    assert_eq!(deletes[0][2], format!("daedalus-{}", env.id));
}

// The embedded terminal reaches the agent's zellij session through a bridge process
// (issue #15 / FR-008): `openshell sandbox exec --tty -n <sandbox> -- zellij attach <s>`.
// `--tty` is load-bearing — zellij needs the forced PTY when the bridge's stdio is piped.
#[test]
fn attach_command_execs_zellij_attach_with_forced_tty() {
    let env = daedalus_proto::EnvironmentId::new();
    let cmd = daedalus_backend_openshell::attach_command("/opt/homebrew/bin/openshell", &env);
    assert_eq!(cmd[0], "/opt/homebrew/bin/openshell");
    assert!(cmd.contains(&"--tty".to_string()), "{cmd:?}");
    let n = cmd.iter().position(|a| a == "-n").expect("-n");
    assert_eq!(cmd[n + 1], format!("daedalus-{env}"));
    let sep = cmd.iter().position(|a| a == "--").expect("separator");
    assert_eq!(
        &cmd[sep + 1..],
        &[
            "zellij".to_string(),
            "attach".into(),
            daedalus_zellij::format_session_name(&env)
        ]
    );
}

// --- Per-session policy (real schema: policy get --base -o json, openshell 0.0.77) ------

// FR-002a: a pre-existing environment gets its worktree's sandbox path writable; the
// worktree lands under /sandbox/worktree (delivery mechanism tracked on issue #9).
#[test]
fn policy_grants_the_worktree_sandbox_path_for_preexisting_environments() {
    let yaml = session_policy_yaml(&preexisting_req());
    assert!(yaml.contains("/sandbox/worktree"), "{yaml}");
}

#[test]
fn policy_grants_no_worktree_path_for_fresh_environments() {
    let yaml = session_policy_yaml(&fresh_req());
    assert!(!yaml.contains("/sandbox/worktree"), "{yaml}");
}

// Local-first posture: deny-all egress — no `network_policies` entries at all (the
// enforcing proxy 403s every CONNECT that no policy allows; verified live 2026-07-07).
#[test]
fn policy_denies_all_egress_by_default() {
    let yaml = session_policy_yaml(&fresh_req());
    assert!(yaml.contains("network_policies: {}"), "{yaml}");
}

// The sandbox stays functional under our policy: the base filesystem grants match the
// built-in default (read-only system paths, writable /sandbox + /tmp), with Landlock in
// best-effort mode for kernels without full support.
#[test]
fn policy_keeps_the_baseline_filesystem_and_landlock_grants() {
    let yaml = session_policy_yaml(&fresh_req());
    // `version` is required — the CLI rejects the file without it ("missing field
    // `version`", verified live against 0.0.77).
    assert!(yaml.starts_with("version: 1\n"), "{yaml}");
    assert!(yaml.contains("filesystem_policy:"), "{yaml}");
    assert!(yaml.contains("- /sandbox"), "{yaml}");
    assert!(yaml.contains("- /tmp"), "{yaml}");
    assert!(yaml.contains("read_only:"), "{yaml}");
    assert!(yaml.contains("compatibility: best_effort"), "{yaml}");
}

// FR-031: the policy grants no provider/credential entries and no environment section —
// combined with the create-flag assertion above, nothing from the host env reaches the
// sandbox.
#[test]
fn policy_carries_no_credential_or_env_grants() {
    let yaml = session_policy_yaml(&preexisting_req());
    assert!(!yaml.contains("providers"), "{yaml}");
    assert!(!yaml.to_lowercase().contains("env"), "{yaml}");
}

// The acquired environment carries its generated policy (what `sandbox create --policy`
// was driven by), so operators can audit exactly what confines the agent.
#[tokio::test]
async fn acquire_records_the_session_policy() {
    let b = backend(ScriptedControl::ready());
    let env = b.acquire(preexisting_req()).await.unwrap();
    let policy = b.policy_for(&env.id).expect("policy recorded");
    assert!(policy.contains("/sandbox/worktree"));
    assert!(policy.contains("network_policies: {}"));
}
