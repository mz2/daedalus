//! Settings screen (§6.8): tunnels, concurrency limit, notifications, theme, and the
//! visible local-first posture.

use crate::theme::{Density, Skin, ThemeMode};

/// The settings view-model (mirrors what the modal edits).
#[derive(Debug, Clone)]
pub struct SettingsView {
    /// Concurrency limit (`None` = unlimited).
    pub concurrency_limit: Option<usize>,
    /// Stall interval in seconds.
    pub stall_interval_secs: u64,
    /// Operator-configured tunnel names (loopback endpoints Daedalus dials).
    pub tunnels: Vec<String>,
    /// Whether terminal/abnormal notifications are enabled.
    pub notifications_enabled: bool,
    /// Active skin.
    pub skin: Skin,
    /// Active theme mode.
    pub mode: ThemeMode,
    /// Active density.
    pub density: Density,
    /// Always true: no open listener by default; remote reach only via operator tunnels
    /// (SC-012, FR-032/033). Shown to the operator as a reassurance, not a toggle.
    pub local_first_no_listener: bool,
}

impl Default for SettingsView {
    fn default() -> Self {
        Self {
            concurrency_limit: None,
            stall_interval_secs: daedalus_core::DEFAULT_STALL_INTERVAL_SECS,
            tunnels: Vec::new(),
            notifications_enabled: true,
            skin: Skin::from_host_os(),
            mode: ThemeMode::Dark,
            density: Density::Regular,
            local_first_no_listener: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_first_posture_is_always_on() {
        assert!(SettingsView::default().local_first_no_listener);
    }
}
