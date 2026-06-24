//! Per-screen view-models that turn `App` queries into render-ready data for the GPUI
//! layer (design/README.md screen map). They hold no orchestration — only presentation
//! shaping — and are unit-testable without a GPU.

pub mod backends;
pub mod discover;
pub mod fleet;
pub mod session;
pub mod settings;
pub mod start;
pub mod tasks;
pub mod tools;
