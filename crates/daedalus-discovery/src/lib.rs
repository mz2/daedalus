//! Multi-source session discovery and de-duplication.
//!
//! Daedalus discovers sessions from three source kinds — local zellij, mDNS on the LAN, and
//! Workshops reachable over operator tunnels — and merges them into one de-duplicated list
//! keyed by stable identity (FR-010–FR-014; contract `contracts/discovery-and-sdk.md`).

pub mod dedupe;
pub mod local;
pub mod mdns;
pub mod tunnel;

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;

use daedalus_proto::{Availability, DiscoveredSession, SourceId, SourceKind};

pub use dedupe::deduplicate;
pub use local::LocalSource;
pub use mdns::MdnsSource;
pub use tunnel::{AdvertisementFetcher, TunnelSource, TunnelTarget};

/// A single source of discovered sessions (contract `discovery-and-sdk.md`).
#[async_trait]
pub trait DiscoverySource: Send + Sync {
    /// Which kind of source this is.
    fn kind(&self) -> SourceKind;

    /// Stable id used to attribute discovered sessions to this source.
    fn source_id(&self) -> SourceId;

    /// Current advertisements from this source.
    async fn poll(&self) -> Vec<DiscoveredSession>;

    /// Unreachable when the host stops advertising / the tunnel drops (FR-014).
    fn availability(&self) -> Availability;
}

/// Coordinates polling across sources, retaining last-seen sessions so that when a source
/// goes unavailable its sessions are marked **unreachable** rather than silently dropped
/// (FR-014, contract C-D2), then de-duplicates by stable identity (FR-013, C-D1).
pub struct DiscoveryCoordinator {
    sources: Vec<Box<dyn DiscoverySource>>,
    last_seen: Mutex<HashMap<SourceId, Vec<DiscoveredSession>>>,
}

impl DiscoveryCoordinator {
    /// Build a coordinator over the given sources.
    #[must_use]
    pub fn new(sources: Vec<Box<dyn DiscoverySource>>) -> Self {
        Self {
            sources,
            last_seen: Mutex::new(HashMap::new()),
        }
    }

    /// Poll every source and return one de-duplicated, availability-correct list.
    pub async fn discover(&self) -> Vec<DiscoveredSession> {
        self.discover_counted().await.0
    }

    /// Like [`Self::discover`], but also reports how many sessions were advertised from
    /// multiple sources and de-duplicated (FR-013 — the Discover footnote).
    pub async fn discover_counted(&self) -> (Vec<DiscoveredSession>, usize) {
        // Phase 1: poll all sources without holding the lock across awaits.
        let mut polled_all: Vec<(SourceId, Availability, Vec<DiscoveredSession>)> =
            Vec::with_capacity(self.sources.len());
        for source in &self.sources {
            let polled = source.poll().await;
            polled_all.push((source.source_id(), source.availability(), polled));
        }

        // Phase 2: reconcile with last-seen state.
        let mut last = self.last_seen.lock().expect("poisoned");
        let mut all = Vec::new();
        for (sid, availability, polled) in polled_all {
            if availability == Availability::Unavailable {
                if let Some(prev) = last.get(&sid) {
                    for mut session in prev.clone() {
                        session.source_availability = Availability::Unavailable;
                        session.attachable = false;
                        session.attach_reason = Some(
                            "source unreachable — host stopped advertising or tunnel dropped"
                                .into(),
                        );
                        all.push(session);
                    }
                }
            } else {
                let current: Vec<DiscoveredSession> = polled
                    .into_iter()
                    .map(|mut session| {
                        session.source_availability = availability;
                        session
                    })
                    .collect();
                last.insert(sid, current.clone());
                all.extend(current);
            }
        }
        drop(last);

        dedupe::deduplicate_counted(all)
    }
}
