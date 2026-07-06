//! Backend registry + availability detection supporting multi-backend selection at start
//! (FR-027, FR-028), plus the operator-configured idle rates used for waiting-cost
//! estimates (FR-021b).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use daedalus_backend::Backend;
use daedalus_proto::{BackendId, BackendKind, BackendStatus};

/// A registry of the backends available to this Daedalus instance. Cloning shares the
/// registered backends and their idle-rate configuration.
#[derive(Clone, Default)]
pub struct BackendRegistry {
    backends: Vec<Arc<dyn Backend>>,
    /// Optional operator-configured idle rate (per hour) per backend kind (FR-021b);
    /// absent ⇒ no cost estimate is shown.
    idle_rates: Arc<Mutex<HashMap<BackendKind, f64>>>,
}

impl BackendRegistry {
    /// Build a registry over the given backends.
    #[must_use]
    pub fn new(backends: Vec<Arc<dyn Backend>>) -> Self {
        Self {
            backends,
            idle_rates: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// First backend of the given kind, if registered.
    #[must_use]
    pub fn by_kind(&self, kind: BackendKind) -> Option<Arc<dyn Backend>> {
        self.backends.iter().find(|b| b.kind() == kind).cloned()
    }

    /// Backend with the given id, if registered.
    #[must_use]
    pub fn by_id(&self, id: BackendId) -> Option<Arc<dyn Backend>> {
        self.backends.iter().find(|b| b.id() == id).cloned()
    }

    /// All registered backends.
    #[must_use]
    pub fn all(&self) -> &[Arc<dyn Backend>] {
        &self.backends
    }

    /// Set (or clear) the operator-configured idle rate (per hour) for a backend kind,
    /// used for waiting-cost estimates (FR-021b).
    pub fn set_idle_rate(&self, kind: BackendKind, rate: Option<f64>) {
        let mut rates = self.idle_rates.lock().expect("poisoned");
        match rate {
            Some(rate) => {
                rates.insert(kind, rate);
            }
            None => {
                rates.remove(&kind);
            }
        }
    }

    /// The configured idle rate (per hour) for a backend kind, if any (FR-021b).
    #[must_use]
    pub fn idle_rate(&self, kind: BackendKind) -> Option<f64> {
        self.idle_rates
            .lock()
            .expect("poisoned")
            .get(&kind)
            .copied()
    }

    /// The configured idle rate for the backend with the given id, if any.
    #[must_use]
    pub fn idle_rate_by_id(&self, id: BackendId) -> Option<f64> {
        self.by_id(id).and_then(|b| self.idle_rate(b.kind()))
    }

    /// Current availability of every backend, each with its stated degraded/unavailable
    /// reason (never errors — FR-028, C-B4). One row per backend, so a surface can flag a
    /// down host while stating that the others keep working.
    pub async fn statuses(&self) -> Vec<BackendStatus> {
        let mut out = Vec::with_capacity(self.backends.len());
        for backend in &self.backends {
            out.push(BackendStatus {
                id: backend.id(),
                kind: backend.kind(),
                availability: backend.availability().await,
                availability_reason: backend.availability_reason().await,
            });
        }
        out
    }
}
