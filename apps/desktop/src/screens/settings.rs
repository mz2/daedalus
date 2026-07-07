//! Settings screen (§6.8): tunnels, concurrency limit, notifications, theme (Light /
//! Dark / System — FR-009a), per-backend idle rates (FR-021b), and the visible
//! local-first posture.

use daedalus_app::App;
use daedalus_proto::BackendKind;

use crate::theme::{Density, Skin, ThemePreference};

/// The explanatory row shown under the theme segment when System is selected (prototype
/// `SettingsModal`).
pub const SYSTEM_THEME_NOTE: &str = "Following your operating system's Light / Dark appearance.";

/// The local-first security banner at the top of Settings (FR-032, SC-012) — prototype
/// `SettingsModal` copy verbatim; a reassurance, not a toggle.
pub const LOCAL_FIRST_BANNER: &str = "Local-first. Daedalus runs no open network listener \
     by default — nothing is reachable from outside this host unless you explicitly \
     enable a tunnel below. Secrets live only in your environments and are never displayed.";

/// The note under the Tunnels group label.
pub const TUNNELS_NOTE: &str = "Each active tunnel adds a remote Workshop backend and \
     makes its environments and sessions available — only while connected.";

/// The Add-tunnel modal's trust note (FR-032; prototype `TunnelModal`).
pub const OUTBOUND_ONLY_NOTE: &str = "Outbound only. Daedalus opens no inbound listener. \
     The tunnel authenticates with your Workshop credentials from the system keychain — \
     the token is never displayed or stored by Daedalus.";

/// The description under the concurrency slider (FR-026).
pub const CONCURRENCY_DESCRIPTION: &str =
    "New sessions beyond this limit are rejected with a clear message.";

/// The concurrency slider bounds (prototype `input type=range` 1–16).
pub const CONCURRENCY_RANGE: std::ops::RangeInclusive<usize> = 1..=16;

/// Parse + validate the concurrency-limit input (G2). An empty/blank string means
/// unlimited (`None`, intended). A non-empty value must parse as a whole number inside
/// [`CONCURRENCY_RANGE`] (1..=16); anything else — a non-numeric value like `"6x"` or an
/// out-of-range one like `"0"` — is REJECTED with a stated reason rather than silently
/// disabling the safety limit (which the old `parse::<usize>().ok()` did by turning both
/// into `None`).
///
/// # Errors
/// Returns a human-readable message when the input is non-empty but not a whole number in
/// range.
pub fn parse_concurrency(input: &str) -> Result<Option<usize>, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(None); // blank ⇒ unlimited (intended)
    }
    let bounds = format!(
        "between {} and {}",
        CONCURRENCY_RANGE.start(),
        CONCURRENCY_RANGE.end()
    );
    let n: usize = trimmed
        .parse()
        .map_err(|_| format!("Concurrency limit must be a whole number {bounds}."))?;
    if !CONCURRENCY_RANGE.contains(&n) {
        return Err(format!("Concurrency limit must be {bounds}."));
    }
    Ok(Some(n))
}

/// The notification toggle labels, in prototype order, with their default states
/// (Resource pressure ships off).
pub const NOTIFICATION_DEFAULTS: [(&str, bool); 5] = [
    ("Session stalled", true),
    ("Session failed", true),
    ("Session completed", true),
    ("Connection lost", true),
    ("Resource pressure", false),
];

/// One tunnel row in the Tunnels group (prototype `SettingsModal` tunnels list): label,
/// mono endpoint, and either the active chip or a Connect action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunnelRow {
    /// Operator label (e.g. "eu-fra-1").
    pub name: String,
    /// The Workshop endpoint Daedalus dials (outbound only).
    pub endpoint: String,
    /// Whether the tunnel is currently connected.
    pub active: bool,
}

impl TunnelRow {
    /// The row's trailing element: the "active" chip text, or the Connect action label.
    #[must_use]
    pub fn action_label(&self) -> &'static str {
        if self.active {
            "active"
        } else {
            "Connect"
        }
    }
}

/// One notification toggle row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationToggle {
    /// Event label (e.g. "Session stalled").
    pub label: &'static str,
    /// Whether notifications for it are enabled.
    pub enabled: bool,
}

/// One optional per-backend idle-rate row (FR-021b): backend, and the operator-configured
/// currency-per-hour rate — clearable, and `None` by default (no cost estimate shown).
#[derive(Debug, Clone, PartialEq)]
pub struct IdleRateRow {
    /// Which backend kind the rate applies to.
    pub backend: BackendKind,
    /// The configured rate (per hour), if any.
    pub rate: Option<f64>,
}

impl IdleRateRow {
    /// The formatted rate value shown in the row (e.g. "$0.60/hr"); `None` when unset.
    #[must_use]
    pub fn rate_text(&self) -> Option<String> {
        self.rate.map(|r| format!("${r:.2}/hr"))
    }
}

/// The settings view-model (mirrors what the modal edits).
#[derive(Debug, Clone)]
pub struct SettingsView {
    /// Concurrency limit (`None` = unlimited).
    pub concurrency_limit: Option<usize>,
    /// Stall interval in seconds.
    pub stall_interval_secs: u64,
    /// Operator-configured tunnels (loopback endpoints Daedalus dials, FR-032).
    pub tunnels: Vec<TunnelRow>,
    /// Per-event notification toggles (prototype order; Resource pressure off).
    pub notifications: Vec<NotificationToggle>,
    /// Active skin.
    pub skin: Skin,
    /// Theme choice — the three-way segment (System | Light | Dark), System default
    /// (FR-009a, T084).
    pub theme: ThemePreference,
    /// Active density.
    pub density: Density,
    /// Always true: no open listener by default; remote reach only via operator tunnels
    /// (SC-012, FR-032/033). Shown to the operator as a reassurance, not a toggle.
    pub local_first_no_listener: bool,
}

impl SettingsView {
    /// The explanatory row under the theme segment — shown only when System is selected.
    #[must_use]
    pub fn theme_note(&self) -> Option<&'static str> {
        (self.theme == ThemePreference::System).then_some(SYSTEM_THEME_NOTE)
    }

    /// The per-backend idle-rate rows (FR-021b): one per registered backend, carrying the
    /// currently configured rate. Edits flow through
    /// [`daedalus_app::Command::SetIdleRate`].
    #[must_use]
    pub fn idle_rate_rows(app: &App) -> Vec<IdleRateRow> {
        let registry = app.core().backend_registry();
        registry
            .all()
            .iter()
            .map(|b| IdleRateRow {
                backend: b.kind(),
                rate: registry.idle_rate(b.kind()),
            })
            .collect()
    }
}

impl Default for SettingsView {
    fn default() -> Self {
        Self {
            concurrency_limit: None,
            stall_interval_secs: daedalus_core::DEFAULT_STALL_INTERVAL_SECS,
            tunnels: Vec::new(),
            notifications: NOTIFICATION_DEFAULTS
                .into_iter()
                .map(|(label, enabled)| NotificationToggle { label, enabled })
                .collect(),
            skin: Skin::from_host_os(),
            theme: ThemePreference::default(),
            density: Density::Regular,
            local_first_no_listener: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use daedalus_app::Command;
    use daedalus_proto::BackendKind;

    #[test]
    fn local_first_posture_is_always_on() {
        assert!(SettingsView::default().local_first_no_listener);
    }

    #[test]
    fn security_banners_carry_the_prototype_copy() {
        // FR-032/SC-012 (T085): the Settings local-first banner and the Add-tunnel
        // outbound-only note, verbatim.
        assert_eq!(
            LOCAL_FIRST_BANNER,
            "Local-first. Daedalus runs no open network listener by default — nothing is \
             reachable from outside this host unless you explicitly enable a tunnel \
             below. Secrets live only in your environments and are never displayed."
        );
        assert_eq!(
            OUTBOUND_ONLY_NOTE,
            "Outbound only. Daedalus opens no inbound listener. The tunnel authenticates \
             with your Workshop credentials from the system keychain — the token is never \
             displayed or stored by Daedalus."
        );
        assert_eq!(
            TUNNELS_NOTE,
            "Each active tunnel adds a remote Workshop backend and makes its environments \
             and sessions available — only while connected."
        );
    }

    #[test]
    fn tunnel_rows_offer_connect_only_while_inactive() {
        // Prototype tunnels list (T085): active rows show the chip, idle rows the
        // Connect action.
        let active = TunnelRow {
            name: "eu-fra-1".into(),
            endpoint: "wss://workshop.eu-fra-1.internal".into(),
            active: true,
        };
        assert_eq!(active.action_label(), "active");
        let idle = TunnelRow {
            active: false,
            ..active
        };
        assert_eq!(idle.action_label(), "Connect");
    }

    #[test]
    fn notification_toggles_default_to_the_prototype_set() {
        // Prototype notifications group (T085): five per-event toggles, resource
        // pressure off by default.
        let view = SettingsView::default();
        let rows: Vec<(&str, bool)> = view
            .notifications
            .iter()
            .map(|t| (t.label, t.enabled))
            .collect();
        assert_eq!(
            rows,
            [
                ("Session stalled", true),
                ("Session failed", true),
                ("Session completed", true),
                ("Connection lost", true),
                ("Resource pressure", false),
            ]
        );
    }

    #[test]
    fn concurrency_slider_carries_the_prototype_bounds_and_description() {
        assert_eq!(CONCURRENCY_RANGE, 1..=16);
        assert_eq!(
            CONCURRENCY_DESCRIPTION,
            "New sessions beyond this limit are rejected with a clear message."
        );
    }

    #[test]
    fn parse_concurrency_validates_range_and_treats_blank_as_unlimited() {
        // G2: blank ⇒ unlimited (intended); anything else must be a whole number inside
        // CONCURRENCY_RANGE, otherwise it is REJECTED rather than silently disabling the
        // safety limit.
        assert_eq!(parse_concurrency(""), Ok(None));
        assert_eq!(parse_concurrency("   "), Ok(None));
        assert_eq!(parse_concurrency("6"), Ok(Some(6)));
        assert_eq!(parse_concurrency("1"), Ok(Some(1)));
        assert_eq!(parse_concurrency("16"), Ok(Some(16)));
        // "6x" must NOT parse to None (unlimited) — it is rejected.
        assert!(parse_concurrency("6x").is_err());
        // "0" is a hard block outside 1..=16 — rejected, not accepted.
        assert!(parse_concurrency("0").is_err());
        assert!(parse_concurrency("17").is_err());
        assert!(parse_concurrency("-1").is_err());
    }

    #[test]
    fn theme_defaults_to_system_with_the_explanatory_row() {
        // FR-009a (T084): the Settings theme segment defaults to System, and only then
        // shows the prototype's explanatory row copy verbatim.
        let mut view = SettingsView::default();
        assert_eq!(view.theme, ThemePreference::System);
        assert_eq!(
            view.theme_note(),
            Some("Following your operating system's Light / Dark appearance.")
        );
        view.theme = ThemePreference::Dark;
        assert_eq!(view.theme_note(), None, "the row only shows for System");
        view.theme = ThemePreference::Light;
        assert_eq!(view.theme_note(), None);
    }

    #[tokio::test]
    async fn idle_rate_rows_list_backends_and_follow_setting_edits() {
        // FR-021b (T086): one optional idle-rate row per registered backend; edits flow
        // through Command::SetIdleRate and the row is clearable.
        let fx = daedalus_tests::Fixture::new();

        let rows = SettingsView::idle_rate_rows(&fx.app);
        assert_eq!(rows.len(), 1, "one row per registered backend");
        assert_eq!(rows[0].backend, BackendKind::Fake);
        assert_eq!(rows[0].rate, None, "no rate configured by default");
        assert_eq!(rows[0].rate_text(), None);

        fx.app
            .execute(Command::SetIdleRate {
                backend: BackendKind::Fake,
                rate: Some(0.60),
            })
            .await
            .unwrap();
        let rows = SettingsView::idle_rate_rows(&fx.app);
        assert_eq!(rows[0].rate, Some(0.60));
        assert_eq!(rows[0].rate_text().as_deref(), Some("$0.60/hr"));

        // Clearable: None removes the value (and thus any cost estimate downstream).
        fx.app
            .execute(Command::SetIdleRate {
                backend: BackendKind::Fake,
                rate: None,
            })
            .await
            .unwrap();
        assert_eq!(SettingsView::idle_rate_rows(&fx.app)[0].rate, None);
    }
}
