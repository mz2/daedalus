//! Local discovery source: enumerate local zellij sessions (FR-010).

use std::sync::Mutex;

use async_trait::async_trait;

use daedalus_proto::{Availability, DiscoveredSession, SessionIdentity, SourceId, SourceKind};
use daedalus_zellij::list_local_sessions;

use crate::{advertised_session, DiscoverySource};

/// Discovers sessions running in local zellij on this host.
pub struct LocalSource {
    id: SourceId,
    /// Optional name prefix that marks a zellij session as Daedalus-managed.
    prefix: String,
    /// Availability recorded by the most recent [`poll`](DiscoverySource::poll): the trait
    /// exposes `availability()` separately, so we cache the last poll outcome here rather
    /// than reporting a hardcoded `Available` while an enumeration error was swallowed into
    /// an empty list (FR-014 — a failed listing must not drop local sessions as "available
    /// but empty").
    availability: Mutex<Availability>,
}

impl Default for LocalSource {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalSource {
    /// A local source that recognises Daedalus sessions by the
    /// [`daedalus_zellij::SESSION_PREFIX`] name prefix.
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: SourceId::new(),
            prefix: daedalus_zellij::SESSION_PREFIX.to_string(),
            availability: Mutex::new(Availability::Available),
        }
    }

    fn to_discovered(&self, name: &str) -> DiscoveredSession {
        let identity = name.strip_prefix(&self.prefix).unwrap_or(name).to_string();
        advertised_session(
            self.id,
            SessionIdentity::new(identity),
            SourceKind::Local,
            name.to_string(),
            "localhost".to_string(),
            None,
        )
    }
}

#[async_trait]
impl DiscoverySource for LocalSource {
    fn kind(&self) -> SourceKind {
        SourceKind::Local
    }

    fn source_id(&self) -> SourceId {
        self.id
    }

    async fn poll(&self) -> Vec<DiscoveredSession> {
        match list_local_sessions().await {
            Ok(names) => {
                *self.availability.lock().expect("poisoned") = Availability::Available;
                names
                    .iter()
                    .filter(|n| n.starts_with(&self.prefix))
                    .map(|n| self.to_discovered(n))
                    .collect()
            }
            // Enumeration failed: report unreachable so the coordinator retains previously
            // seen local sessions and marks them unreachable, rather than dropping them as
            // "available but empty" (FR-014).
            Err(_) => {
                *self.availability.lock().expect("poisoned") = Availability::Unavailable;
                Vec::new()
            }
        }
    }

    fn availability(&self) -> Availability {
        // Reflect the most recent poll: the coordinator always polls before reading this,
        // so a swallowed enumeration error surfaces here as Unavailable.
        *self.availability.lock().expect("poisoned")
    }
}
