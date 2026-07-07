//! Integration test for discovery across all three source kinds (US3): each session is
//! discovered once with its source/status and is connectable.

use daedalus_proto::SourceKind;
use daedalus_tests::{core_with_discovery, TestDiscoverySource};

#[tokio::test]
async fn discovers_across_sources_dedupes_and_connects() {
    let local = TestDiscoverySource::new(SourceKind::Local);
    let mdns = TestDiscoverySource::new(SourceKind::Mdns);
    let tunnel = TestDiscoverySource::new(SourceKind::TunneledWorkshop);

    // "shared" is advertised by both local and mDNS; "remote" only via the tunnel.
    local.set_sessions(vec![
        local.make_session("shared", true),
        local.make_session("local-only", true),
    ]);
    mdns.set_sessions(vec![mdns.make_session("shared", true)]);
    tunnel.set_sessions(vec![tunnel.make_session("remote", true)]);

    let (core, _dir) = core_with_discovery(vec![Box::new(local), Box::new(mdns), Box::new(tunnel)]);

    let discovered = core.discovered().await;
    // shared (deduped) + local-only + remote = 3 unique sessions.
    assert_eq!(discovered.len(), 3);
    let mut identities: Vec<String> = discovered.iter().map(|d| d.id.identity.0.clone()).collect();
    identities.sort();
    assert_eq!(identities, vec!["local-only", "remote", "shared"]);

    // Connect attaches via zellij (returns the resolved attach info).
    let target = discovered
        .iter()
        .find(|d| d.id.identity.0 == "remote")
        .unwrap();
    let connected = core.connect_discovered(target.id.clone()).await.unwrap();
    assert_eq!(connected.zellij_session, "daedalus-remote");
}

#[tokio::test]
async fn connecting_a_non_attachable_session_reports_a_reason() {
    let local = TestDiscoverySource::new(SourceKind::Local);
    local.set_sessions(vec![local.make_session("dead", false)]);
    let (core, _dir) = core_with_discovery(vec![Box::new(local)]);

    let discovered = core.discovered().await;
    let id = discovered[0].id.clone();
    let err = core.connect_discovered(id).await.unwrap_err();
    assert!(err.to_string().contains("not attachable"));
}
