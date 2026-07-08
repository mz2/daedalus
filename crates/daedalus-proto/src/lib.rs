//! Shared serde contract types for Daedalus.
//!
//! These types are consumed verbatim by `daedalus-core`, every backend, the discovery and
//! SDK crates, the app-API, and the surfaces. They carry no behaviour beyond construction
//! and display helpers — orchestration lives in `daedalus-core`.

pub mod api;
pub mod entities;
pub mod ids;
pub mod status;

pub use api::*;
pub use entities::*;
pub use ids::*;
pub use status::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_status_terminality() {
        assert!(SessionStatus::Completed.is_terminal());
        assert!(SessionStatus::Failed.is_terminal());
        assert!(SessionStatus::Stopped.is_terminal());
        assert!(!SessionStatus::Running.is_terminal());
        assert!(SessionStatus::Stalled.is_attention());
        assert!(SessionStatus::AwaitingConfirmation.is_attention());
        assert!(SessionStatus::WaitingForInput.is_attention());
        assert!(SessionStatus::Unknown.is_attention());
    }

    #[test]
    fn ids_are_distinct_types_but_roundtrip_through_serde() {
        let s = SessionId::new();
        let json = serde_json::to_string(&s).unwrap();
        let back: SessionId = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn status_tokens_are_stable() {
        // Design-prototype keys per the data-model.md terminology mapping.
        assert_eq!(SessionStatus::WaitingForInput.as_str(), "awaiting");
        assert_eq!(SessionStatus::AwaitingConfirmation.as_str(), "confirm");
        assert_eq!(SessionStatus::Unknown.as_str(), "unknown");
        assert_eq!(TaskStatus::InProgress.as_str(), "in_progress");
        assert_eq!(BackendKind::Workshop.as_str(), "workshop");
        // The persisted serde token matches the `DAEDALUS_BACKEND` token (idle-rate keys
        // in the settings store round-trip through serde — persist::text_to_enum).
        assert_eq!(BackendKind::OpenShell.as_str(), "openshell");
        assert_eq!(
            serde_json::to_value(BackendKind::OpenShell).unwrap(),
            serde_json::json!("openshell")
        );
    }
}
