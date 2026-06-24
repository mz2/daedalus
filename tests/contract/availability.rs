//! Contract test: backend availability never errors and a down backend is marked
//! unavailable while others keep working (C-B4, FR-028).

use std::sync::Arc;

use daedalus_backend::Backend;
use daedalus_backend_fake::FakeBackend;
use daedalus_core::BackendRegistry;
use daedalus_proto::Availability;

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
