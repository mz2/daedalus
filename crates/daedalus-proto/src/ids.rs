//! Strongly-typed identifiers shared across the core and every surface.
//!
//! Each id is a thin newtype over a UUID so the type system prevents mixing, e.g., a
//! [`SessionId`] with a [`ToolId`]. They are `serde`-serializable for persistence and the
//! (future) wire surfaces.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! uuid_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        pub struct $name(pub Uuid);

        impl $name {
            /// Generate a fresh random identifier.
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            /// Construct from an existing UUID (e.g. when re-hydrating from persistence).
            #[must_use]
            pub fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl core::fmt::Display for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                core::fmt::Display::fmt(&self.0, f)
            }
        }
    };
}

uuid_id!(
    /// Identifies a [`crate::entities::Session`]; stable across discovery sources (FR-013).
    SessionId
);
uuid_id!(
    /// Identifies a registered [`crate::entities::AgenticTool`].
    ToolId
);
uuid_id!(
    /// Identifies an [`crate::entities::Objective`].
    ObjectiveId
);
uuid_id!(
    /// Identifies a [`crate::entities::SandboxEnvironment`].
    EnvironmentId
);
uuid_id!(
    /// Identifies an [`crate::entities::EnvironmentBackend`].
    BackendId
);
uuid_id!(
    /// Identifies a [`crate::entities::Source`] (host where a session is discovered).
    SourceId
);
uuid_id!(
    /// Identifies an [`crate::entities::EventRecord`].
    EventId
);

/// Identifier of a tracked task, sourced verbatim from the SDD artifact (e.g. `T001`).
///
/// Unlike the UUID ids this is the human identifier from `tasks.md`, so it is a `String`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TaskId(pub String);

impl TaskId {
    /// Construct from anything string-like.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl core::fmt::Display for TaskId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Stable cross-source identity for a discovered session (host + zellij session, or an
/// SDK-issued UUID). Two advertisements with the same identity are the same session (FR-013).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SessionIdentity(pub String);

impl SessionIdentity {
    /// Construct from anything string-like.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl core::fmt::Display for SessionIdentity {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Identity of a discovered session as presented to the operator (source + stable identity).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DiscoveredSessionId {
    /// Which source advertised it.
    pub source: SourceId,
    /// The stable cross-source identity.
    pub identity: SessionIdentity,
}
