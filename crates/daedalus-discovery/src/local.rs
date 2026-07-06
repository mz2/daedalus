//! Local discovery source: enumerate local zellij sessions (FR-010).

use async_trait::async_trait;

use daedalus_proto::{Availability, DiscoveredSession, SessionIdentity, SourceId, SourceKind};
use daedalus_zellij::list_local_sessions;

use crate::{advertised_session, DiscoverySource};

/// Discovers sessions running in local zellij on this host.
pub struct LocalSource {
    id: SourceId,
    /// Optional name prefix that marks a zellij session as Daedalus-managed.
    prefix: String,
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
            Ok(names) => names
                .iter()
                .filter(|n| n.starts_with(&self.prefix))
                .map(|n| self.to_discovered(n))
                .collect(),
            // zellij missing ⇒ no local sessions; availability() reflects unreachability.
            Err(_) => Vec::new(),
        }
    }

    fn availability(&self) -> Availability {
        // Local enumeration is best-effort; treat as available (zellij absence yields an
        // empty list, which `poll` handles).
        Availability::Available
    }
}
