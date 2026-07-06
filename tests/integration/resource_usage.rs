//! Integration test: resource usage surfaces to the query API for a running session
//! (FR-019, T034), including recent per-metric history for the telemetry rail's
//! sparklines (FR-019, T082).

use daedalus_app::AppQuery;
use daedalus_proto::{MetricKind, ResourceUsageMetric, Timestamp};
use daedalus_tests::Fixture;

#[tokio::test]
async fn resource_usage_is_sampled_and_queryable() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");
    let id = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();

    // Before sampling, nothing is recorded.
    assert!(fx.app.resource_usage(id).is_empty());

    fx.core.record_resource_usage(id).await.unwrap();

    let usage = fx.app.resource_usage(id);
    assert!(!usage.is_empty(), "usage surfaces to AppQuery");
    let kinds: Vec<MetricKind> = usage.iter().map(|m| m.metric).collect();
    assert!(kinds.contains(&MetricKind::Cpu));
    assert!(kinds.contains(&MetricKind::Memory));
}

#[tokio::test]
async fn recent_metric_history_is_retained_and_queryable() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");
    let id = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();

    // A deterministic CPU history, oldest → newest.
    for (i, value) in [10.0, 20.0, 30.0].into_iter().enumerate() {
        fx.core
            .store()
            .insert_metric(&ResourceUsageMetric {
                session_id: id,
                metric: MetricKind::Cpu,
                value,
                timestamp: Timestamp::from_millis(i as i64 + 1),
            })
            .unwrap();
    }

    // Full recent window, oldest → newest (sparkline order).
    let values: Vec<f64> = fx
        .app
        .resource_history(id, MetricKind::Cpu, 8)
        .iter()
        .map(|m| m.value)
        .collect();
    assert_eq!(values, vec![10.0, 20.0, 30.0]);

    // The window is bounded: only the most recent N samples.
    let values: Vec<f64> = fx
        .app
        .resource_history(id, MetricKind::Cpu, 2)
        .iter()
        .map(|m| m.value)
        .collect();
    assert_eq!(values, vec![20.0, 30.0]);

    // Other metrics are untouched by the CPU history.
    assert!(fx.app.resource_history(id, MetricKind::Disk, 8).is_empty());

    // Repeated backend sampling accumulates history too (FR-019 trends).
    fx.core.record_resource_usage(id).await.unwrap();
    fx.core.record_resource_usage(id).await.unwrap();
    assert_eq!(fx.app.resource_history(id, MetricKind::Memory, 8).len(), 2);
}
