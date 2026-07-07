//! Contract test: backend availability never errors and a down backend is marked
//! unavailable while others keep working (C-B4, FR-028), including the stated
//! degraded/unavailable reason carried end-to-end to the surfaces (T083).

use std::sync::Arc;

use daedalus_app::{App, AppQueryAsync};
use daedalus_backend::Backend;
use daedalus_backend_fake::FakeBackend;
use daedalus_core::{BackendRegistry, Core, CoreConfig, Store};
use daedalus_proto::Availability;
use daedalus_zellij::InMemoryTerminal;

#[tokio::test]
async fn availability_is_a_value_and_down_backend_is_isolated() {
    let a = Arc::new(FakeBackend::new());
    let b = Arc::new(FakeBackend::new());
    let registry = BackendRegistry::new(vec![
        a.clone() as Arc<dyn Backend>,
        b.clone() as Arc<dyn Backend>,
    ]);

    // All available initially.
    let statuses = registry.statuses().await;
    assert_eq!(statuses.len(), 2);
    assert!(statuses
        .iter()
        .all(|s| s.availability == Availability::Available));

    // Take A down — querying never errors; B is unaffected.
    a.set_availability(Availability::Unavailable);
    let statuses = registry.statuses().await;
    let down = statuses.iter().find(|s| s.id == a.id()).unwrap();
    let up = statuses.iter().find(|s| s.id == b.id()).unwrap();
    assert_eq!(down.availability, Availability::Unavailable);
    assert_eq!(up.availability, Availability::Available);
}

#[tokio::test]
async fn degraded_availability_carries_a_stated_reason() {
    // FR-028: degraded is flagged *with* a stated reason (e.g. resource pressure), while a
    // healthy backend carries none.
    let degraded = Arc::new(FakeBackend::new());
    let healthy = Arc::new(FakeBackend::new());
    let registry = BackendRegistry::new(vec![
        degraded.clone() as Arc<dyn Backend>,
        healthy.clone() as Arc<dyn Backend>,
    ]);

    degraded.set_availability_with_reason(
        Availability::Degraded,
        Some("high memory pressure — provisioning may be slow"),
    );

    let statuses = registry.statuses().await;
    let row = statuses.iter().find(|s| s.id == degraded.id()).unwrap();
    assert_eq!(row.availability, Availability::Degraded);
    assert_eq!(
        row.availability_reason.as_deref(),
        Some("high memory pressure — provisioning may be slow")
    );
    let ok = statuses.iter().find(|s| s.id == healthy.id()).unwrap();
    assert_eq!(ok.availability, Availability::Available);
    assert_eq!(ok.availability_reason, None);
}

#[tokio::test]
async fn app_query_lists_every_backend_with_reasons_so_continuity_is_explicit() {
    // FR-028: the surface-facing query returns one row per backend, each with its
    // availability + stated reason — so a surface can show a down host AND state that
    // "other hosts keep working" from the same response.
    let dir = tempfile::TempDir::new().unwrap();
    let store = Arc::new(Store::open(dir.path()).unwrap());
    let down = Arc::new(FakeBackend::new());
    let up = Arc::new(FakeBackend::new());
    let registry = BackendRegistry::new(vec![
        down.clone() as Arc<dyn Backend>,
        up.clone() as Arc<dyn Backend>,
    ]);
    let core = Arc::new(Core::new(
        store,
        registry,
        Arc::new(InMemoryTerminal::new()),
        None,
        CoreConfig::default(),
    ));
    let app = App::new(core);

    down.set_availability_with_reason(
        Availability::Unavailable,
        Some("tunnel dropped — reconnect to resume"),
    );

    let rows = app.backends().await;
    assert_eq!(rows.len(), 2);
    let down_row = rows.iter().find(|s| s.id == down.id()).unwrap();
    assert_eq!(down_row.availability, Availability::Unavailable);
    assert_eq!(
        down_row.availability_reason.as_deref(),
        Some("tunnel dropped — reconnect to resume")
    );
    let up_row = rows.iter().find(|s| s.id == up.id()).unwrap();
    assert_eq!(up_row.availability, Availability::Available);
    assert_eq!(up_row.availability_reason, None);
}
