//! Backend registry + availability detection supporting multi-backend selection at start
//! (FR-027, FR-028).

use std::sync::Arc;

use daedalus_backend::Backend;
use daedalus_proto::{BackendId, BackendKind, BackendStatus};

/// A registry of the backends available to this Daedalus instance.
#[derive(Clone, Default)]
pub struct BackendRegistry {
    backends: Vec<Arc<dyn Backend>>,
}

impl BackendRegistry {
    /// Build a registry over the given backends.
    #[must_use]
    pub fn new(backends: Vec<Arc<dyn Backend>>) -> Self {
        Self { backends }
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

    /// Current availability of every backend (never errors — FR-028, C-B4).
    pub async fn statuses(&self) -> Vec<BackendStatus> {
        let mut out = Vec::with_capacity(self.backends.len());
        for backend in &self.backends {
            out.push(BackendStatus {
                id: backend.id(),
                kind: backend.kind(),
                availability: backend.availability().await,
            });
        }
        out
    }
}
