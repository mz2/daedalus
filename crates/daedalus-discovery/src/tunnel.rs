//! Tunneled-Workshop discovery: query the in-Workshop SDK over an operator-established
//! tunnel (FR-010, FR-033). Daedalus never opens a listener — it only dials the local
//! endpoint the operator's tunnel exposes (research R4).

use std::sync::Mutex;

use async_trait::async_trait;

use daedalus_proto::{Availability, DiscoveredSession, SourceId, SourceKind};
use daedalus_sdk::Advertisement;

use crate::{advertised_session, DiscoverySource};

/// One operator-configured tunnel entry. Daedalus dials `local_endpoint` only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunnelTarget {
    /// Operator-facing name.
    pub name: String,
    /// The loopback endpoint the operator's tunnel forwards (e.g. `127.0.0.1:7733`).
    pub local_endpoint: String,
}

/// Obtains advertisements from a tunnel endpoint. The real implementation dials the
/// endpoint and speaks the SDK protocol; tests inject canned advertisements.
#[async_trait]
pub trait AdvertisementFetcher: Send + Sync {
    /// Fetch the current advertisements behind a tunnel target, or a reason it failed.
    async fn fetch(&self, target: &TunnelTarget) -> Result<Vec<Advertisement>, String>;
}

/// Discovers sessions advertised by Workshops reachable over operator tunnels.
pub struct TunnelSource {
    id: SourceId,
    targets: Vec<TunnelTarget>,
    fetcher: Box<dyn AdvertisementFetcher>,
    availability: Mutex<Availability>,
}

impl TunnelSource {
    /// Build a tunnel source over the given targets and fetcher.
    #[must_use]
    pub fn new(targets: Vec<TunnelTarget>, fetcher: Box<dyn AdvertisementFetcher>) -> Self {
        Self {
            id: SourceId::new(),
            targets,
            fetcher,
            availability: Mutex::new(Availability::Available),
        }
    }

    fn to_discovered(&self, target: &TunnelTarget, ad: Advertisement) -> DiscoveredSession {
        advertised_session(
            self.id,
            ad.session_identity,
            SourceKind::TunneledWorkshop,
            ad.connect.zellij_session,
            format!("{} ({})", ad.connect.host_label, target.name),
            Some(ad.artifacts),
        )
    }
}

#[async_trait]
impl DiscoverySource for TunnelSource {
    fn kind(&self) -> SourceKind {
        SourceKind::TunneledWorkshop
    }

    fn source_id(&self) -> SourceId {
        self.id
    }

    async fn poll(&self) -> Vec<DiscoveredSession> {
        let mut out = Vec::new();
        let mut any_failure = false;
        let mut any_success = false;

        for target in &self.targets {
            match self.fetcher.fetch(target).await {
                Ok(ads) => {
                    any_success = true;
                    for ad in ads {
                        out.push(self.to_discovered(target, ad));
                    }
                }
                Err(reason) => {
                    any_failure = true;
                    tracing::warn!("tunnel '{}' unreachable: {reason}", target.name);
                }
            }
        }

        // FR-014: a dropped tunnel makes the source degraded/unavailable, not silently empty.
        let availability = match (any_success, any_failure) {
            (true, false) => Availability::Available,
            (true, true) => Availability::Degraded,
            (false, _) if self.targets.is_empty() => Availability::Available,
            (false, _) => Availability::Unavailable,
        };
        *self.availability.lock().expect("poisoned") = availability;

        out
    }

    fn availability(&self) -> Availability {
        *self.availability.lock().expect("poisoned")
    }
}
