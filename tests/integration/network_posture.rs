//! Integration test for the local-first posture (SC-012, FR-032/033): no open network
//! listener by default; remote reach only over operator-established tunnels.

use daedalus_discovery::{
    tunnel::AdvertisementFetcher, DiscoverySource, TunnelSource, TunnelTarget,
};
use daedalus_proto::Availability;
use daedalus_tests::Fixture;

#[tokio::test]
async fn no_discovery_configured_means_no_implicit_remote_reach() {
    // A core with no discovery coordinator reaches nothing remote on its own.
    let fx = Fixture::new();
    assert!(fx.core.discovered().await.is_empty());
}

struct NoTargetsFetcher;

#[async_trait::async_trait]
impl AdvertisementFetcher for NoTargetsFetcher {
    async fn fetch(&self, _t: &TunnelTarget) -> Result<Vec<daedalus_sdk::Advertisement>, String> {
        // Should never be called: there are no operator targets.
        Err("no tunnel".into())
    }
}

#[tokio::test]
async fn remote_reach_requires_explicit_operator_tunnels() {
    // With no operator-configured targets, the tunnel source advertises nothing and opens
    // no connection — Daedalus only ever dials operator-provided loopback endpoints.
    let source = TunnelSource::new(Vec::new(), Box::new(NoTargetsFetcher));
    assert!(source.poll().await.is_empty());
    assert_eq!(source.availability(), Availability::Available);
}
