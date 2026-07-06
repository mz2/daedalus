//! Contract tests for discovery de-duplication and source-unavailable marking
//! (C-D1, C-D2, C-D3).

use std::sync::Arc;

use daedalus_discovery::{deduplicate, DiscoveryCoordinator};
use daedalus_proto::{Availability, SourceKind};
use daedalus_tests::TestDiscoverySource;

#[test]
fn c_d1_same_identity_from_two_sources_appears_once() {
    let local = TestDiscoverySource::new(SourceKind::Local);
    let mdns = TestDiscoverySource::new(SourceKind::Mdns);
    let from_local = local.make_session("session-7", true);
    let from_mdns = mdns.make_session("session-7", true);

    let merged = deduplicate(vec![from_local, from_mdns]);
    assert_eq!(merged.len(), 1, "one entry per stable identity");
    assert_eq!(merged[0].id.identity.0, "session-7");
}

#[tokio::test]
async fn c_d2_retains_last_seen_as_unreachable_when_source_drops() {
    let src = Arc::new(TestDiscoverySource::new(SourceKind::TunneledWorkshop));
    src.set_sessions(vec![src.make_session("s9", true)]);
    let coord = DiscoveryCoordinator::new(vec![Box::new(src.clone())]);

    // First pass: the session is healthy and attachable.
    let first = coord.discover().await;
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].source_availability, Availability::Available);

    // The tunnel drops: the session is retained but marked unreachable, never healthy.
    src.set_availability(Availability::Unavailable);
    let second = coord.discover().await;
    assert_eq!(second.len(), 1, "last-seen session is not silently dropped");
    assert_eq!(second[0].source_availability, Availability::Unavailable);
    assert!(!second[0].attachable);
    assert!(second[0].attach_reason.is_some());
}

#[test]
fn c_d3_non_attachable_session_is_kept_with_a_reason() {
    let src = TestDiscoverySource::new(SourceKind::Local);
    let session = src.make_session("dead", false);
    let merged = deduplicate(vec![session]);
    assert_eq!(merged.len(), 1, "never silently dropped");
    assert!(!merged[0].attachable);
    assert!(merged[0].attach_reason.is_some());
}
