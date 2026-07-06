//! The Daedalus design system, ported from `design/README.md` +
//! `design/Daedalus-Prototype-standalone.html` to typed Rust tokens (T017).
//!
//! These are pure data — skins, themes, density, the color-blind-safe status palette,
//! typography, radii, and surface colors — so they compile and are unit-tested headlessly.
//! The GPUI renderer (behind the `gpui` feature) consumes them; nothing here depends on
//! GPUI, keeping the design system verifiable without a GPU.

use daedalus_proto::{SessionStatus, TaskStatus};

/// An sRGB color with 8-bit channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
}

impl Color {
    /// Pure white (a candidate badge/foreground color).
    pub const WHITE: Color = Color {
        r: 0xFF,
        g: 0xFF,
        b: 0xFF,
    };
    /// Near-black used for foregrounds on light fills (matches the design ink).
    pub const INK: Color = Color {
        r: 0x10,
        g: 0x10,
        b: 0x10,
    };

    /// Construct from a `#RRGGBB` hex string (panics only on a malformed literal token —
    /// these are compile-time constants from the design system).
    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Parse `#RRGGBB`.
    #[must_use]
    pub fn from_hex(hex: &str) -> Option<Self> {
        let h = hex.strip_prefix('#')?;
        if h.len() != 6 {
            return None;
        }
        Some(Self {
            r: u8::from_str_radix(&h[0..2], 16).ok()?,
            g: u8::from_str_radix(&h[2..4], 16).ok()?,
            b: u8::from_str_radix(&h[4..6], 16).ok()?,
        })
    }

    /// Render back to `#RRGGBB`.
    #[must_use]
    pub fn to_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

    /// Relative luminance (WCAG) for contrast checks.
    #[must_use]
    pub fn luminance(self) -> f64 {
        fn lin(c: u8) -> f64 {
            let s = c as f64 / 255.0;
            if s <= 0.03928 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * lin(self.r) + 0.7152 * lin(self.g) + 0.0722 * lin(self.b)
    }

    /// WCAG contrast ratio against another color (1.0..=21.0).
    #[must_use]
    pub fn contrast_ratio(self, other: Color) -> f64 {
        let a = self.luminance();
        let b = other.luminance();
        let (hi, lo) = if a >= b { (a, b) } else { (b, a) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// The legible foreground (white or ink) to paint on top of this fill — whichever yields
    /// the higher WCAG contrast. Used for filled status pills so their label always reads.
    #[must_use]
    pub fn best_foreground(self) -> Color {
        if self.contrast_ratio(Color::WHITE) >= self.contrast_ratio(Color::INK) {
            Color::WHITE
        } else {
            Color::INK
        }
    }
}

/// The two platform-native skins (`design/README.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skin {
    /// Ubuntu / Yaru — Ubuntu orange accent, GNOME status palette. Default (Linux).
    Yaru,
    /// Cupertino — warm amber accent, traffic-light controls (macOS).
    Mac,
}

impl Skin {
    /// Pick the skin from the host OS (GPUI selects the host skin).
    #[must_use]
    pub fn from_host_os() -> Self {
        if cfg!(target_os = "macos") {
            Skin::Mac
        } else {
            Skin::Yaru
        }
    }

    /// The skin's signature accent.
    #[must_use]
    pub fn accent(self) -> Color {
        match self {
            Skin::Yaru => Color::rgb(0xE9, 0x54, 0x20), // Ubuntu orange
            Skin::Mac => Color::rgb(0xE0, 0x90, 0x1C),  // warm amber
        }
    }

    /// UI font family (system fallbacks applied by the renderer).
    #[must_use]
    pub fn ui_font(self) -> &'static str {
        match self {
            Skin::Yaru => "Ubuntu",
            Skin::Mac => "SF Pro",
        }
    }

    /// Monospace font for terminal, IDs, and metrics.
    #[must_use]
    pub fn mono_font(self) -> &'static str {
        "JetBrains Mono"
    }
}

/// Light/dark theme — both required, WCAG-AA, color-blind-safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    /// Default.
    Dark,
    /// Light.
    Light,
}

/// The operating system's current appearance, as reported by the platform layer (the GPUI
/// window supplies it; headless tests inject it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsAppearance {
    /// The OS is in light appearance.
    Light,
    /// The OS is in dark appearance.
    Dark,
}

/// The operator's theme choice (FR-009a, T084): Light, Dark, or System — where System
/// follows the OS appearance and is the **default** (prototype `app.jsx`
/// `TWEAK_DEFAULTS.theme = "system"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemePreference {
    /// Follow the operating system's Light / Dark appearance (default).
    #[default]
    System,
    /// Always light.
    Light,
    /// Always dark.
    Dark,
}

impl ThemePreference {
    /// The segment options in prototype order (System | Light | Dark).
    #[must_use]
    pub fn all() -> [ThemePreference; 3] {
        [
            ThemePreference::System,
            ThemePreference::Light,
            ThemePreference::Dark,
        ]
    }

    /// The segment label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            ThemePreference::System => "System",
            ThemePreference::Light => "Light",
            ThemePreference::Dark => "Dark",
        }
    }

    /// Resolve the effective [`ThemeMode`]: System adopts the injected OS appearance;
    /// explicit choices ignore it.
    #[must_use]
    pub fn resolve(self, os_appearance: OsAppearance) -> ThemeMode {
        match self {
            ThemePreference::Light => ThemeMode::Light,
            ThemePreference::Dark => ThemeMode::Dark,
            ThemePreference::System => match os_appearance {
                OsAppearance::Light => ThemeMode::Light,
                OsAppearance::Dark => ThemeMode::Dark,
            },
        }
    }
}

/// Density steps → row height / card pad / gap (design/README.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Density {
    /// 26 / 10 / 9.
    Compact,
    /// 30 / 14 / 12 — default.
    Regular,
    /// 36 / 19 / 17.
    Comfy,
}

impl Density {
    /// Row height in px.
    #[must_use]
    pub fn row_height(self) -> u32 {
        match self {
            Density::Compact => 26,
            Density::Regular => 30,
            Density::Comfy => 36,
        }
    }
    /// Card padding in px.
    #[must_use]
    pub fn card_pad(self) -> u32 {
        match self {
            Density::Compact => 10,
            Density::Regular => 14,
            Density::Comfy => 19,
        }
    }
    /// Gap in px.
    #[must_use]
    pub fn gap(self) -> u32 {
        match self {
            Density::Compact => 9,
            Density::Regular => 12,
            Density::Comfy => 17,
        }
    }
}

/// The status palette key — maps 1:1 to `SessionStatus`/`TaskStatus` plus the discovery
/// `unreachable` state, always paired with an icon + label (never color alone).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusTone {
    /// Provisioning/launching.
    Starting,
    /// Live.
    Running,
    /// No progress — attention.
    Stalled,
    /// Crash/abnormal exit.
    Failed,
    /// Awaiting operator confirmation (FR-015a).
    Awaiting,
    /// All tasks done / confirmed.
    Completed,
    /// Stopped by operator.
    Stopped,
    /// Indeterminate.
    Unknown,
    /// Source gone (mDNS/tunnel).
    Unreachable,
}

impl StatusTone {
    /// The tone for a session status.
    #[must_use]
    pub fn from_session(status: SessionStatus) -> Self {
        match status {
            SessionStatus::Starting => StatusTone::Starting,
            SessionStatus::Running => StatusTone::Running,
            SessionStatus::Stalled => StatusTone::Stalled,
            SessionStatus::Failed => StatusTone::Failed,
            // Waiting-for-input and awaiting-confirmation deliberately share the purple
            // family: purple means "blocked on the operator" (design/README.md §status).
            SessionStatus::WaitingForInput => StatusTone::Awaiting,
            SessionStatus::AwaitingConfirmation => StatusTone::Awaiting,
            SessionStatus::Completed => StatusTone::Completed,
            SessionStatus::Stopped => StatusTone::Stopped,
            SessionStatus::Unknown => StatusTone::Unknown,
        }
    }

    /// The design-table label for a session status. The two purple states are
    /// distinguished by label ("Waiting for input" vs. "Awaiting confirmation"), and
    /// `Unknown` reads as "Connection lost" — never as healthy (design/README.md §status).
    #[must_use]
    pub fn session_label(status: SessionStatus) -> &'static str {
        match status {
            SessionStatus::WaitingForInput => "Waiting for input",
            SessionStatus::AwaitingConfirmation => "Awaiting confirmation",
            SessionStatus::Unknown => "Connection lost",
            other => StatusTone::from_session(other).label(),
        }
    }

    /// The tone for a tracked-task status.
    #[must_use]
    pub fn from_task(status: TaskStatus) -> Self {
        match status {
            TaskStatus::Todo => StatusTone::Unknown,
            TaskStatus::InProgress => StatusTone::Running,
            TaskStatus::Done => StatusTone::Completed,
            TaskStatus::Blocked => StatusTone::Failed,
        }
    }

    /// A short label always shown alongside the color (accessibility).
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            StatusTone::Starting => "Starting",
            StatusTone::Running => "Running",
            StatusTone::Stalled => "Stalled",
            StatusTone::Failed => "Failed",
            StatusTone::Awaiting => "Awaiting",
            StatusTone::Completed => "Completed",
            StatusTone::Stopped => "Stopped",
            StatusTone::Unknown => "Unknown",
            StatusTone::Unreachable => "Unreachable",
        }
    }

    /// A screen-reader label for AccessKit (e.g. "Status: Running"). Pairs with the glyph
    /// and color so the status is conveyed without relying on color (WCAG 1.4.1).
    #[must_use]
    pub fn accessible_label(self) -> String {
        format!("Status: {}", self.label())
    }

    /// A glyph hint paired with color+label so status is never color-only (design §status).
    #[must_use]
    pub fn glyph(self) -> &'static str {
        match self {
            StatusTone::Starting => "◐",
            StatusTone::Running => "●",
            StatusTone::Stalled => "▲",
            StatusTone::Failed => "✕",
            StatusTone::Awaiting => "?",
            StatusTone::Completed => "✓",
            StatusTone::Stopped => "■",
            StatusTone::Unknown => "·",
            StatusTone::Unreachable => "⦸",
        }
    }

    /// The palette color for this tone under a skin (`design/README.md` status table).
    #[must_use]
    pub fn color(self, skin: Skin) -> Color {
        match (skin, self) {
            (Skin::Yaru, StatusTone::Starting) => Color::rgb(0x35, 0x84, 0xE4),
            (Skin::Mac, StatusTone::Starting) => Color::rgb(0x0A, 0x84, 0xFF),
            (Skin::Yaru, StatusTone::Running) => Color::rgb(0x2E, 0xC2, 0x7E),
            (Skin::Mac, StatusTone::Running) => Color::rgb(0x2D, 0xC6, 0x53),
            (Skin::Yaru, StatusTone::Stalled) => Color::rgb(0xE5, 0xA5, 0x0A),
            (Skin::Mac, StatusTone::Stalled) => Color::rgb(0xFF, 0x9F, 0x0A),
            (Skin::Yaru, StatusTone::Failed) => Color::rgb(0xE0, 0x1B, 0x24),
            (Skin::Mac, StatusTone::Failed) => Color::rgb(0xFF, 0x45, 0x3A),
            (Skin::Yaru, StatusTone::Awaiting) => Color::rgb(0xC0, 0x61, 0xCB),
            (Skin::Mac, StatusTone::Awaiting) => Color::rgb(0xBF, 0x5A, 0xF2),
            (Skin::Yaru, StatusTone::Completed) => Color::rgb(0x1C, 0x9B, 0x8E),
            (Skin::Mac, StatusTone::Completed) => Color::rgb(0x26, 0xB5, 0xA8),
            (Skin::Yaru, StatusTone::Stopped) => Color::rgb(0x77, 0x76, 0x7B),
            (Skin::Mac, StatusTone::Stopped) => Color::rgb(0x98, 0x98, 0x9F),
            (Skin::Yaru, StatusTone::Unknown) => Color::rgb(0x9A, 0x99, 0x96),
            (Skin::Mac, StatusTone::Unknown) => Color::rgb(0xB0, 0xB0, 0xB8),
            (Skin::Yaru, StatusTone::Unreachable) => Color::rgb(0x77, 0x76, 0x7B),
            (Skin::Mac, StatusTone::Unreachable) => Color::rgb(0x8E, 0x8E, 0x96),
        }
    }
}

/// Corner radii (`design/README.md` — yaru tightens md/lg/xl).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Radii {
    /// xs.
    pub xs: u32,
    /// sm (control radius).
    pub sm: u32,
    /// md.
    pub md: u32,
    /// lg.
    pub lg: u32,
    /// xl.
    pub xl: u32,
    /// window.
    pub window: u32,
}

/// Background/surface colors for a theme×skin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Surfaces {
    /// App backdrop.
    pub app: Color,
    /// Window chrome.
    pub window: Color,
    /// Content area.
    pub content: Color,
    /// Elevated surface (cards/popovers).
    pub elevated: Color,
    /// Sidebar.
    pub sidebar: Color,
    /// Title bar.
    pub titlebar: Color,
    /// Primary text.
    pub text_primary: Color,
}

/// A fully-resolved theme: skin + mode + density + the derived tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    /// Active skin.
    pub skin: Skin,
    /// Light/dark.
    pub mode: ThemeMode,
    /// Density step.
    pub density: Density,
    /// Radii.
    pub radii: Radii,
    /// Surface colors.
    pub surfaces: Surfaces,
}

impl Theme {
    /// Resolve a theme for the host OS in dark mode at regular density.
    #[must_use]
    pub fn host_default() -> Self {
        Self::resolve(Skin::from_host_os(), ThemeMode::Dark, Density::Regular)
    }

    /// Resolve all tokens for a skin/mode/density combination.
    #[must_use]
    pub fn resolve(skin: Skin, mode: ThemeMode, density: Density) -> Self {
        let radii = match skin {
            Skin::Yaru => Radii {
                xs: 4,
                sm: 6,
                md: 6,
                lg: 8,
                xl: 10,
                window: 11,
            },
            Skin::Mac => Radii {
                xs: 4,
                sm: 6,
                md: 9,
                lg: 13,
                xl: 18,
                window: 12,
            },
        };
        let surfaces = match mode {
            ThemeMode::Dark => Surfaces {
                app: Color::rgb(0x0D, 0x0D, 0x0D),
                window: Color::rgb(0x24, 0x24, 0x24),
                content: Color::rgb(0x1E, 0x1E, 0x1E),
                elevated: Color::rgb(0x2F, 0x2F, 0x2F),
                sidebar: Color::rgb(0x2A, 0x2A, 0x2A),
                titlebar: Color::rgb(0x30, 0x30, 0x30),
                text_primary: Color::rgb(0xF2, 0xF2, 0xF2),
            },
            ThemeMode::Light => Surfaces {
                app: Color::rgb(0xF4, 0xF4, 0xF3),
                window: Color::rgb(0xFA, 0xFA, 0xFA),
                content: Color::rgb(0xFF, 0xFF, 0xFF),
                elevated: Color::rgb(0xFF, 0xFF, 0xFF),
                sidebar: Color::rgb(0xEE, 0xEE, 0xEC),
                titlebar: Color::rgb(0xE6, 0xE6, 0xE4),
                text_primary: Color::rgb(0x1A, 0x1A, 0x1A),
            },
        };
        Self {
            skin,
            mode,
            density,
            radii,
            surfaces,
        }
    }

    /// The status color under this theme's skin.
    #[must_use]
    pub fn status_color(&self, tone: StatusTone) -> Color {
        tone.color(self.skin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrips() {
        assert_eq!(Color::from_hex("#E95420").unwrap(), Skin::Yaru.accent());
        assert_eq!(Skin::Yaru.accent().to_hex(), "#E95420");
    }

    #[test]
    fn status_palette_matches_design_table() {
        assert_eq!(
            StatusTone::Awaiting.color(Skin::Yaru),
            Color::from_hex("#C061CB").unwrap()
        );
        assert_eq!(
            StatusTone::Running.color(Skin::Mac),
            Color::from_hex("#2DC653").unwrap()
        );
    }

    #[test]
    fn every_session_status_maps_to_a_tone_with_label_and_glyph() {
        for status in [
            SessionStatus::Starting,
            SessionStatus::Running,
            SessionStatus::Completed,
            SessionStatus::Failed,
            SessionStatus::Stalled,
            SessionStatus::Stopped,
            SessionStatus::AwaitingConfirmation,
            SessionStatus::WaitingForInput,
            SessionStatus::Unknown,
        ] {
            let tone = StatusTone::from_session(status);
            assert!(!tone.label().is_empty());
            assert!(!tone.glyph().is_empty());
            assert!(!StatusTone::session_label(status).is_empty());
        }
    }

    const ALL_TONES: [StatusTone; 9] = [
        StatusTone::Starting,
        StatusTone::Running,
        StatusTone::Stalled,
        StatusTone::Failed,
        StatusTone::Awaiting,
        StatusTone::Completed,
        StatusTone::Stopped,
        StatusTone::Unknown,
        StatusTone::Unreachable,
    ];

    #[test]
    fn body_text_meets_aa_normal_in_both_themes_and_skins() {
        // The labels that carry meaning are rendered in the theme's primary text color on
        // its surfaces — these must clear WCAG-AA for normal text (4.5:1).
        for skin in [Skin::Yaru, Skin::Mac] {
            for mode in [ThemeMode::Dark, ThemeMode::Light] {
                let t = Theme::resolve(skin, mode, Density::Regular);
                for surface in [t.surfaces.content, t.surfaces.sidebar, t.surfaces.elevated] {
                    let ratio = t.surfaces.text_primary.contrast_ratio(surface);
                    assert!(ratio >= 4.5, "{skin:?}/{mode:?} body text {ratio:.2} < 4.5");
                }
            }
        }
    }

    #[test]
    fn filled_status_pills_are_legible_in_every_theme_and_skin() {
        // A filled pill paints its label in best_foreground() over the status color; the
        // label must clear the AA large-text/UI bar (3:1) for every tone, skin, and theme.
        for skin in [Skin::Yaru, Skin::Mac] {
            for tone in ALL_TONES {
                let fill = tone.color(skin);
                let ratio = fill.best_foreground().contrast_ratio(fill);
                assert!(ratio >= 3.0, "{skin:?} {tone:?} pill text {ratio:.2} < 3.0");
            }
        }
    }

    #[test]
    fn status_is_never_conveyed_by_color_alone() {
        // WCAG 1.4.1: every tone carries a glyph + label + accessible label in addition to
        // its hue.
        for tone in ALL_TONES {
            assert!(!tone.glyph().is_empty());
            assert!(!tone.label().is_empty());
            assert!(tone.accessible_label().contains(tone.label()));
        }
    }

    #[test]
    fn system_theme_preference_is_the_default_and_follows_the_os() {
        // FR-009a (T084): "System" follows the OS appearance and is the DEFAULT
        // (prototype `app.jsx` TWEAK_DEFAULTS.theme = "system").
        assert_eq!(ThemePreference::default(), ThemePreference::System);
        assert_eq!(
            ThemePreference::System.resolve(OsAppearance::Light),
            ThemeMode::Light
        );
        assert_eq!(
            ThemePreference::System.resolve(OsAppearance::Dark),
            ThemeMode::Dark
        );
        // Explicit choices ignore the OS appearance.
        assert_eq!(
            ThemePreference::Light.resolve(OsAppearance::Dark),
            ThemeMode::Light
        );
        assert_eq!(
            ThemePreference::Dark.resolve(OsAppearance::Light),
            ThemeMode::Dark
        );
    }

    #[test]
    fn theme_preference_segment_matches_the_prototype() {
        // Segment order + labels per the prototype Settings modal (System | Light | Dark).
        let labels: Vec<&str> = ThemePreference::all().iter().map(|p| p.label()).collect();
        assert_eq!(labels, ["System", "Light", "Dark"]);
    }

    #[test]
    fn density_tokens_match_spec() {
        assert_eq!(Density::Compact.row_height(), 26);
        assert_eq!(Density::Regular.card_pad(), 14);
        assert_eq!(Density::Comfy.gap(), 17);
    }
}
