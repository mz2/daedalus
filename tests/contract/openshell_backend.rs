//! Contract tests for the NVIDIA OpenShell backend (issue #9): C-B1/C-B2/C-B4/C-B5 against
//! a scripted control, plus the per-session policy guarantees — worktree mount (FR-002a),
//! default-deny outbound network (local-first posture), no credential passthrough (FR-031),
//! and GPU gated on host readiness (Linux hosts only).

use daedalus_backend::{AcquireRequest, Backend, BackendError};
use daedalus_backend_openshell::{session_policy_yaml, OpenShellBackend, OpenShellControl};
use daedalus_proto::{Availability, BackendKind, Origin, ResourceLimits, WorktreeRef};

/// A control whose host state is scripted, so every availability tier and failure path is
/// exercisable without an `openshell` CLI or Docker on the test host.
struct ScriptedControl {
    binary: Option<String>,
    runtime_ready: bool,
    gpu_ready: bool,
}

impl ScriptedControl {
    /// A fully healthy OpenShell host (CLI + container runtime, no GPU).
    fn ready() -> Self {
        Self {
            binary: Some("/bin/true".into()),
            runtime_ready: true,
            gpu_ready: false,
        }
    }
}

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
}

fn backend(control: ScriptedControl) -> OpenShellBackend {
    OpenShellBackend::new(Box::new(control))
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
    let b = backend(ScriptedControl {
        binary: None,
        runtime_ready: false,
        gpu_ready: false,
    });
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
        binary: Some("/bin/true".into()),
        runtime_ready: false,
        gpu_ready: false,
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
        binary: Some("/bin/true".into()),
        runtime_ready: false,
        gpu_ready: false,
    });
    let err = b.acquire(fresh_req()).await.unwrap_err();
    assert!(matches!(err, BackendError::ProvisionFailed(_)));
    assert_eq!(b.live_environment_count(), 0);
}

// C-B2: a failed agent start tears the environment down — no orphan (FR-005). The
// scripted control points at a binary that cannot be spawned.
#[tokio::test]
async fn c_b2_failed_start_tears_the_environment_down() {
    let b = backend(ScriptedControl {
        binary: Some("/nonexistent/openshell-cli".into()),
        runtime_ready: true,
        gpu_ready: false,
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
}

// C-B5: stop and teardown are idempotent and isolated to one environment (FR-024).
#[tokio::test]
async fn c_b5_stop_and_teardown_are_idempotent_and_isolated() {
    let b = backend(ScriptedControl::ready());
    let a = b.acquire(fresh_req()).await.unwrap();
    let other = b.acquire(preexisting_req()).await.unwrap();

    b.stop(&a.id).await.unwrap();
    b.stop(&a.id).await.unwrap();
    b.teardown(&a.id).await.unwrap();
    b.teardown(&a.id).await.unwrap();

    assert_eq!(b.live_environment_count(), 1, "the other env is untouched");
    assert!(b.policy_for(&other.id).is_some());
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

// --- Per-session policy (generated YAML) -----------------------------------------------

// FR-002a: a pre-existing environment mounts exactly the isolated worktree, read-write.
#[test]
fn policy_mounts_the_worktree_for_preexisting_environments() {
    let yaml = session_policy_yaml(&preexisting_req(), false);
    assert!(yaml.contains("/work/repo/.worktrees/feat"), "{yaml}");
    assert!(yaml.contains("rw"), "worktree is writable: {yaml}");
}

#[test]
fn policy_mounts_nothing_for_fresh_environments() {
    let yaml = session_policy_yaml(&fresh_req(), false);
    assert!(yaml.contains("mounts: []"), "no host mounts: {yaml}");
}

// Local-first posture: outbound network is denied by default.
#[test]
fn policy_denies_outbound_network_by_default() {
    let yaml = session_policy_yaml(&fresh_req(), false);
    assert!(yaml.contains("outbound: deny"), "{yaml}");
}

// FR-031: no host environment/credential passthrough, ever.
#[test]
fn policy_passes_no_host_environment_through() {
    let yaml = session_policy_yaml(&fresh_req(), false);
    assert!(yaml.contains("passthrough: []"), "{yaml}");
}

// GPU only appears in the policy when the host is GPU-ready (Linux + Container Toolkit).
#[test]
fn policy_gates_gpu_on_host_readiness() {
    assert!(!session_policy_yaml(&fresh_req(), false).contains("gpu"));
    assert!(session_policy_yaml(&fresh_req(), true).contains("gpu:\n    enabled: true"));
}

// Operator resource limits map onto the policy (FR-025-adjacent; backend defaults apply
// when unset — absent keys, not zeros).
#[test]
fn policy_maps_resource_limits_when_set() {
    let mut req = fresh_req();
    req.limits = ResourceLimits {
        cpu_cores: Some(2.5),
        memory_bytes: Some(1_073_741_824),
        disk_bytes: None,
        time_secs: Some(3600),
    };
    let yaml = session_policy_yaml(&req, false);
    assert!(yaml.contains("cpu_cores: 2.5"), "{yaml}");
    assert!(yaml.contains("memory_bytes: 1073741824"), "{yaml}");
    assert!(
        !yaml.contains("disk_bytes"),
        "unset limit stays absent: {yaml}"
    );
    assert!(yaml.contains("time_secs: 3600"), "{yaml}");
}

// The acquired environment carries its generated policy (what a real `sandbox create`
// call is driven by), so operators can audit exactly what confines the agent.
#[tokio::test]
async fn acquire_records_the_session_policy() {
    let b = backend(ScriptedControl::ready());
    let env = b.acquire(preexisting_req()).await.unwrap();
    let policy = b.policy_for(&env.id).expect("policy recorded");
    assert!(policy.contains("/work/repo/.worktrees/feat"));
    assert!(policy.contains("outbound: deny"));
}
