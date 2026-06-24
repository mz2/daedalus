//! Integration test: resource usage surfaces to the query API for a running session
//! (FR-019, T034).

use daedalus_app::AppQuery;
use daedalus_proto::MetricKind;
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
