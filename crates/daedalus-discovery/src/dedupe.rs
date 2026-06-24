//! Stable-identity de-duplication across discovery sources (FR-013, contract C-D1).

use std::collections::BTreeMap;

use daedalus_proto::{Availability, DiscoveredSession};

/// Collapse the same session (by stable identity) seen via multiple sources into one entry.
///
/// When the same identity appears more than once, the **most useful** advertisement wins:
/// an attachable entry beats a non-attachable one, and an entry from an `Available` source
/// beats one from a `Degraded`/`Unavailable` source (FR-014 — never present a
/// known-unreachable session as healthy).
#[must_use]
pub fn deduplicate(all: Vec<DiscoveredSession>) -> Vec<DiscoveredSession> {
    let mut best: BTreeMap<String, DiscoveredSession> = BTreeMap::new();

    for candidate in all {
        let key = candidate.id.identity.0.clone();
        match best.get(&key) {
            Some(existing) if !is_better(&candidate, existing) => {}
            _ => {
                best.insert(key, candidate);
            }
        }
    }

    best.into_values().collect()
}

/// Rank for choosing the surviving duplicate: higher is better.
fn rank(s: &DiscoveredSession) -> u8 {
    let avail = match s.source_availability {
        Availability::Available => 2,
        Availability::Degraded => 1,
        Availability::Unavailable => 0,
    };
    let attach = u8::from(s.attachable);
    avail * 2 + attach
}

fn is_better(candidate: &DiscoveredSession, existing: &DiscoveredSession) -> bool {
    rank(candidate) > rank(existing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use daedalus_proto::{DiscoveredSessionId, SessionIdentity, SourceId, SourceKind};

    fn sample(attachable: bool, avail: Availability) -> DiscoveredSession {
        DiscoveredSession {
            id: DiscoveredSessionId {
                source: SourceId::new(),
                identity: SessionIdentity::new("abc"),
            },
            kind: SourceKind::Local,
            source_availability: avail,
            zellij_session: "daedalus-abc".into(),
            host_label: "host".into(),
            status: None,
            attachable,
            attach_reason: None,
            artifacts: None,
        }
    }

    #[test]
    fn same_identity_collapses_to_one() {
        let a = sample(false, Availability::Available);
        let b = sample(true, Availability::Available);
        let out = deduplicate(vec![a, b]);
        assert_eq!(out.len(), 1);
        assert!(out[0].attachable, "the attachable duplicate should win");
    }

    #[test]
    fn available_source_beats_unreachable() {
        let healthy = sample(true, Availability::Available);
        let gone = sample(true, Availability::Unavailable);
        let out = deduplicate(vec![gone, healthy]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source_availability, Availability::Available);
    }
}
