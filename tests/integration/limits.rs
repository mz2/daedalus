//! Integration test for the resource-limit policy (T087, spec edge case "Resource
//! exhaustion"): a sandbox that exceeds its allotted CPU/memory/disk/time is surfaced to
//! the operator and the session is stopped per the configured limit policy, with the
//! outcome recording the reason. Other sessions are unaffected.

use daedalus_app::AppQuery;
use daedalus_core::{CoreConfig, LimitPolicy};
use daedalus_proto::{AppEvent, ResourceLimits, SessionStatus, StartSessionRequest};
use daedalus_tests::Fixture;

/// A start request whose environment carries the given resource limits.
fn limited_request(
    fx: &Fixture,
    tool: daedalus_proto::ToolId,
    limits: ResourceLimits,
) -> StartSessionRequest {
    StartSessionRequest {
        limits,
        ..fx.fresh_request(tool)
    }
}

/// The fake backend reports 256 MiB of memory used; a lower ceiling breaches.
const BELOW_USAGE: u64 = 128 * 1024 * 1024;
const ABOVE_USAGE: u64 = 512 * 1024 * 1024;

#[tokio::test]
async fn exceeding_a_memory_limit_stops_the_session_and_states_the_reason() {
    let fx = Fixture::new(); // default policy: stop on breach
    let tool = fx.register_sample_tool("claude");

    let over = fx
        .core
        .start_session(limited_request(
            &fx,
            tool,
            ResourceLimits {
                memory_bytes: Some(BELOW_USAGE),
                ..ResourceLimits::default()
            },
        ))
        .await
        .unwrap();
    // A generously-limited and an unlimited session share the fleet.
    let roomy = fx
        .core
        .start_session(limited_request(
            &fx,
            tool,
            ResourceLimits {
                memory_bytes: Some(ABOVE_USAGE),
                ..ResourceLimits::default()
            },
        ))
        .await
        .unwrap();
    let unlimited = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();

    let mut rx = fx.app.subscribe();

    // The usage sampler observes the breach…
    fx.core.record_resource_usage(over).await.unwrap();
    fx.core.record_resource_usage(roomy).await.unwrap();
    fx.core.record_resource_usage(unlimited).await.unwrap();

    // …the session is stopped with the reason recorded on its outcome…
    let detail = fx.app.session(over).unwrap();
    assert_eq!(detail.session.status, SessionStatus::Stopped);
    let reason = detail
        .session
        .terminal_outcome
        .and_then(|o| o.reason)
        .expect("outcome carries the stated reason");
    assert!(
        reason.contains("resource limit exceeded: memory"),
        "reason states which limit was exceeded, got {reason:?}"
    );
    // …the agent is actually halted in its environment…
    assert!(!fx.backend.agent_running(&detail.environment.id));

    // …and the breach is surfaced to the operator (event + notification).
    let events: Vec<AppEvent> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
    assert!(
        events.iter().any(|e| matches!(
            e,
            AppEvent::SessionStatusChanged { id, status: SessionStatus::Stopped } if *id == over
        )),
        "the stop is announced"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            AppEvent::Notification(n)
                if n.session == over
                    && n.note.as_deref().is_some_and(|s| s.contains("resource limit exceeded"))
        )),
        "the operator is notified with the stated reason"
    );

    // Other sessions are unaffected.
    for id in [roomy, unlimited] {
        assert_eq!(
            fx.app.session(id).unwrap().session.status,
            SessionStatus::Running,
            "sessions within their limits keep running"
        );
    }
}

#[tokio::test]
async fn notify_only_policy_surfaces_the_breach_without_stopping() {
    // The configured limit policy governs the response: NotifyOnly surfaces the breach to
    // the operator but leaves the session running.
    let fx = Fixture::with_config(CoreConfig {
        limit_policy: LimitPolicy::NotifyOnly,
        ..CoreConfig::default()
    });
    let tool = fx.register_sample_tool("claude");
    let over = fx
        .core
        .start_session(limited_request(
            &fx,
            tool,
            ResourceLimits {
                memory_bytes: Some(BELOW_USAGE),
                ..ResourceLimits::default()
            },
        ))
        .await
        .unwrap();

    let mut rx = fx.app.subscribe();
    fx.core.record_resource_usage(over).await.unwrap();

    assert_eq!(
        fx.app.session(over).unwrap().session.status,
        SessionStatus::Running,
        "notify-only leaves the session running"
    );
    let surfaced = std::iter::from_fn(|| rx.try_recv().ok()).any(|e| {
        matches!(
            e,
            AppEvent::Notification(n)
                if n.session == over
                    && n.note.as_deref().is_some_and(|s| s.contains("resource limit exceeded: memory"))
        )
    });
    assert!(surfaced, "the breach is still surfaced to the operator");
}
