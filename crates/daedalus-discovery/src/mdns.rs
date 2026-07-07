//! mDNS discovery source: browse `_daedalus._tcp`, identity carried in TXT records (FR-010).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use mdns_sd::{ServiceDaemon, ServiceEvent};

use daedalus_proto::{Availability, DiscoveredSession, SessionIdentity, SourceId, SourceKind};

use crate::{advertised_session, DiscoverySource};

/// The Daedalus mDNS service type.
pub const SERVICE_TYPE: &str = "_daedalus._tcp.local.";

type Map = HashMap<String, DiscoveredSession>;

/// Browses the LAN for advertised Daedalus sessions over mDNS-SD.
pub struct MdnsSource {
    id: SourceId,
    seen: Arc<Mutex<Map>>,
    available: Arc<Mutex<Availability>>,
    // Held to keep the daemon alive for the source's lifetime.
    _daemon: Option<ServiceDaemon>,
}

impl Default for MdnsSource {
    fn default() -> Self {
        Self::new()
    }
}

impl MdnsSource {
    /// Start browsing. If the mDNS daemon cannot start, the source is created in an
    /// `Unavailable` state rather than failing (FR-014 — surfaced, not crashed).
    #[must_use]
    pub fn new() -> Self {
        let id = SourceId::new();
        let seen: Arc<Mutex<Map>> = Arc::new(Mutex::new(HashMap::new()));
        let available = Arc::new(Mutex::new(Availability::Available));

        let daemon = match ServiceDaemon::new() {
            Ok(daemon) => daemon,
            Err(e) => {
                tracing::warn!("mDNS daemon unavailable: {e}");
                *available.lock().expect("poisoned") = Availability::Unavailable;
                return Self {
                    id,
                    seen,
                    available,
                    _daemon: None,
                };
            }
        };

        match daemon.browse(SERVICE_TYPE) {
            Ok(receiver) => {
                let seen_bg = Arc::clone(&seen);
                let available_bg = Arc::clone(&available);
                let source_id = id;
                std::thread::spawn(move || {
                    while let Ok(event) = receiver.recv() {
                        match event {
                            ServiceEvent::ServiceResolved(info) => {
                                if let Some(session) = resolve(source_id, &info) {
                                    seen_bg
                                        .lock()
                                        .expect("poisoned")
                                        .insert(info.get_fullname().to_string(), session);
                                }
                            }
                            ServiceEvent::ServiceRemoved(_, fullname) => {
                                seen_bg.lock().expect("poisoned").remove(&fullname);
                            }
                            ServiceEvent::SearchStarted(_) | ServiceEvent::SearchStopped(_) => {}
                            _ => {}
                        }
                    }
                    *available_bg.lock().expect("poisoned") = Availability::Unavailable;
                });
            }
            Err(e) => {
                tracing::warn!("mDNS browse failed: {e}");
                *available.lock().expect("poisoned") = Availability::Unavailable;
            }
        }

        Self {
            id,
            seen,
            available,
            _daemon: Some(daemon),
        }
    }
}

fn resolve(source: SourceId, info: &mdns_sd::ServiceInfo) -> Option<DiscoveredSession> {
    let props = info.get_properties();
    let identity = props.get_property_val_str("identity")?.to_string();
    let zellij_session = props
        .get_property_val_str("zellij")
        .unwrap_or("")
        .to_string();
    let host_label = props
        .get_property_val_str("host")
        .unwrap_or_else(|| info.get_hostname())
        .to_string();

    Some(advertised_session(
        source,
        SessionIdentity::new(identity),
        SourceKind::Mdns,
        zellij_session,
        host_label,
        None,
    ))
}

#[async_trait]
impl DiscoverySource for MdnsSource {
    fn kind(&self) -> SourceKind {
        SourceKind::Mdns
    }

    fn source_id(&self) -> SourceId {
        self.id
    }

    async fn poll(&self) -> Vec<DiscoveredSession> {
        self.seen
            .lock()
            .expect("poisoned")
            .values()
            .cloned()
            .collect()
    }

    fn availability(&self) -> Availability {
        *self.available.lock().expect("poisoned")
    }
}
