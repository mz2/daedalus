//! Contract tests for discovery de-duplication and source-unavailable marking
//! (C-D1, C-D2, C-D3).

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use daedalus_discovery::{
    deduplicate, AdvertisementFetcher, DiscoveryCoordinator, TunnelSource, TunnelTarget,
};
use daedalus_proto::{ArtifactRef, Availability, SessionIdentity, SourceKind};
use daedalus_sdk::{Advertisement, ConnectInfo};
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

/// A fetcher over named targets; a target in `dropped` returns an error so the
/// `TunnelSource` reports `Degraded` while still serving its live targets (tunnel.rs).
#[derive(Clone, Default)]
struct FakeFetcher {
    ads: Arc<Mutex<Vec<(String, Advertisement)>>>,
    dropped: Arc<Mutex<HashSet<String>>>,
}

impl FakeFetcher {
    fn advertise(&self, target: &str, identity: &str) {
        self.ads.lock().unwrap().push((
            target.to_string(),
            Advertisement {
                session_identity: SessionIdentity::new(identity),
                connect: ConnectInfo {
                    zellij_session: format!("daedalus-{identity}"),
                    host_label: "workshop".to_string(),
                },
                artifacts: ArtifactRef {
                    root: "/tmp".to_string(),
                    tasks_file: "/tmp/tasks.md".to_string(),
                },
            },
        ));
    }

    fn drop_target(&self, target: &str) {
        self.dropped.lock().unwrap().insert(target.to_string());
    }
}

#[async_trait]
impl AdvertisementFetcher for FakeFetcher {
    async fn fetch(&self, target: &TunnelTarget) -> Result<Vec<Advertisement>, String> {
        if self.dropped.lock().unwrap().contains(&target.name) {
            return Err(format!("tunnel '{}' dropped", target.name));
        }
        let ads = self
            .ads
            .lock()
            .unwrap()
            .iter()
            .filter(|(t, _)| t == &target.name)
            .map(|(_, ad)| ad.clone())
            .collect();
        Ok(ads)
    }
}

fn tunnel_target(name: &str) -> TunnelTarget {
    TunnelTarget {
        name: name.to_string(),
        local_endpoint: format!("127.0.0.1:0/{name}"),
    }
}

/// FR-014 / C-D2: when one of several tunnel targets drops the source is `Degraded`
/// (not `Unavailable`), and the dropped target's previously-seen session must be
/// retained and marked unreachable — not silently dropped — while the surviving
/// target's session stays healthy and attachable.
#[tokio::test]
async fn c_d2_degraded_source_retains_dropped_targets_session_as_unreachable() {
    let fetcher = FakeFetcher::default();
    fetcher.advertise("alpha", "s-alpha");
    fetcher.advertise("beta", "s-beta");
    let source = TunnelSource::new(
        vec![tunnel_target("alpha"), tunnel_target("beta")],
        Box::new(fetcher.clone()),
    );
    let coord = DiscoveryCoordinator::new(vec![Box::new(source)]);

    // First pass: both targets healthy → both sessions available.
    let first = coord.discover().await;
    assert_eq!(first.len(), 2);
    assert!(first
        .iter()
        .all(|s| s.source_availability == Availability::Available));

    // The beta tunnel drops: the source goes Degraded (alpha still serves).
    fetcher.drop_target("beta");
    let second = coord.discover().await;

    assert_eq!(
        second.len(),
        2,
        "the dropped target's session is retained, not silently dropped"
    );
    let beta = second
        .iter()
        .find(|s| s.id.identity.0 == "s-beta")
        .expect("dropped session retained");
    assert_eq!(beta.source_availability, Availability::Unavailable);
    assert!(!beta.attachable);
    assert!(beta.attach_reason.is_some());

    let alpha = second
        .iter()
        .find(|s| s.id.identity.0 == "s-alpha")
        .expect("surviving session present");
    assert!(
        alpha.attachable,
        "the live target's session stays attachable"
    );
}

/// The retain-on-degrade behaviour must not become retain-forever: when the source is
/// fully `Available` and a session is legitimately gone, it is removed.
#[tokio::test]
async fn fully_available_source_drops_a_legitimately_ended_session() {
    let src = Arc::new(TestDiscoverySource::new(SourceKind::Local));
    src.set_sessions(vec![
        src.make_session("a", true),
        src.make_session("b", true),
    ]);
    let coord = DiscoveryCoordinator::new(vec![Box::new(src.clone())]);

    let first = coord.discover().await;
    assert_eq!(first.len(), 2);

    // "b" ends; the source stays fully Available (never went Degraded/Unavailable).
    src.set_sessions(vec![src.make_session("a", true)]);
    let second = coord.discover().await;
    assert_eq!(
        second.len(),
        1,
        "ended session removed under a healthy source"
    );
    assert_eq!(second[0].id.identity.0, "a");
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
