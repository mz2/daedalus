# Contract: Discovery + in-Workshop SDK

**Crates**: `daedalus-discovery`, `daedalus-sdk`. **Satisfies**: FR-010–FR-014, FR-017.

## Discovery

Daedalus discovers sessions from three source kinds and merges them into one de-duplicated list.

```rust
#[async_trait]
pub trait DiscoverySource: Send + Sync {
    fn kind(&self) -> SourceKind;                  // Local | Mdns | TunneledWorkshop
    async fn poll(&self) -> Vec<DiscoveredSession>; // current advertisements
    fn availability(&self) -> Availability;        // unreachable when host stops / tunnel drops (FR-014)
}

/// Merge across sources; collapse the same session (by stable identity) to one entry (FR-013).
pub fn deduplicate(all: Vec<DiscoveredSession>) -> Vec<DiscoveredSession>;
```

- **Local**: enumerate local zellij sessions.
- **mDNS**: browse `_daedalus._tcp`; TXT carries host + session identity (FR-010).
- **TunneledWorkshop**: query the in-Workshop SDK over an operator-established tunnel (FR-010, FR-033).

### Rules (test-first)
- **C-D1**: a session advertised via two sources appears once, keyed by stable identity (FR-013). *(test)*
- **C-D2**: when a source goes unavailable, its sessions are marked unreachable, not healthy (FR-014). *(test)*
- **C-D3**: a discovered session that cannot be attached via zellij is shown non-attachable with a reason
  (edge case), never silently dropped. *(test)*

## In-Workshop SDK (advertisement + task status)

Runs inside a Workshop environment; advertises a connectable service and exposes the session's SDD artifact
location so the task board can read status (FR-012, FR-017).

```rust
pub struct Advertisement {
    pub session_identity: SessionIdentity,   // stable identity for dedup
    pub connect: ConnectInfo,                // how Daedalus attaches (via zellij over the tunnel)
    pub artifacts: ArtifactRef,              // workspace location of SDD artifacts (e.g. tasks.md)
}

pub trait WorkshopSdk {
    fn advertise(&self) -> Advertisement;
    /// Task states parsed from SDD artifacts (e.g. SpecKit tasks.md checkbox state) — NOT inferred.
    fn task_states(&self) -> Vec<TrackedTask>;   // FR-017, Principle IV
}
```

### Rules (test-first)
- **C-S1**: `task_states` reflects `tasks.md` checkbox changes; the board updates ≤5s (SC-010). *(test)*
- **C-S2**: a Workshop not advertising its SDK service is not discoverable, and the operator is told why
  (edge case: SDK not advertised), not shown an unexplained empty list. *(test)*
