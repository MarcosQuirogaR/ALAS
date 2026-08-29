// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/theme.py
// Reference: alas @ rust-port-baseline.

//! Shared color palette and theme definitions for ALAS figures and reports.
//!
//! Decoupled from any front-end UI framework so figure generators remain
//! headless-safe and produce consistent colors across SVG exports and GPU
//! rendering.

use serde::{Deserialize, Serialize};

/// Color palette configuration for plots and user interface chrome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Palette {
    /// Identifier of the palette (`"light"`, `"grey"`, or `"dark"`).
    pub name: &'static str,
    /// Main figure and axes background color in hex format.
    pub bg: &'static str,
    /// Front-end panel and input background color.
    pub panel: &'static str,
    /// Front-end widget border color.
    pub border: &'static str,
    /// Plot axes spine and boundary color.
    pub spine: &'static str,
    /// Axis tick and label text color.
    pub tick: &'static str,
    /// Plot title and primary text color.
    pub title: &'static str,
    /// Selection, focus, and highlight accent color.
    pub accent: &'static str,
}

/// Light palette with white background for print exports and light UI mode.
pub static PALETTE_LIGHT: Palette = Palette {
    name: "light",
    bg: "#ffffff",
    panel: "#f4f5f7",
    border: "#dfe2e8",
    spine: "#333333",
    tick: "#333333",
    title: "#000000",
    accent: "#2563eb",
};

/// Grey palette with medium-dark background.
pub static PALETTE_GREY: Palette = Palette {
    name: "grey",
    bg: "#3a3a3a",
    panel: "#41454c",
    border: "#565b63",
    spine: "#777777",
    tick: "#dddddd",
    title: "#ffffff",
    accent: "#6aa2ff",
};

/// Dark palette with high-contrast elements for desktop application theme.
pub static PALETTE_DARK: Palette = Palette {
    name: "dark",
    bg: "#1e1e1e",
    panel: "#262a31",
    border: "#363b44",
    spine: "#555555",
    tick: "#cccccc",
    title: "#ffffff",
    accent: "#4f8cff",
};

/// Dark desktop palette meeting WCAG 2.2 SC 1.4.11's 3:1 non-text contrast.
pub static PALETTE_DARK_ACCESSIBLE: Palette = Palette {
    name: "dark-accessible",
    bg: "#1e1e1e",
    panel: "#262a31",
    border: "#737881",
    spine: "#737881",
    tick: "#cccccc",
    title: "#ffffff",
    accent: "#4f8cff",
};

/// Grey desktop palette meeting WCAG 2.2 SC 1.4.11 against canvas and panels.
pub static PALETTE_GREY_ACCESSIBLE: Palette = Palette {
    name: "grey-accessible",
    bg: "#3a3a3a",
    panel: "#41454c",
    border: "#969696",
    spine: "#969696",
    tick: "#dddddd",
    title: "#ffffff",
    accent: "#6aa2ff",
};

/// Default theme used for the desktop interface.
pub const DEFAULT_THEME: &str = "dark";

/// Series color for baseline design traces across comparison figures.
pub const BASELINE_COLOR: &str = "tab:blue";

/// Series color for optimized design traces across comparison figures.
pub const OPTIMIZED_COLOR: &str = "tab:red";

/// Trace color for ghosted prior runs in history overlays.
pub const GHOST_COLOR: &str = "#999999";

/// Themes used for parity evidence; grey remains a diagnostic UI palette only.
pub const PARITY_THEMES: &[&str] = &["light", "dark"];

/// Return whether a theme is part of the approved render-parity set.
pub fn is_parity_theme(theme: &str) -> bool {
    PARITY_THEMES.contains(&theme)
}

/// Resolve an optional theme name to its matching static palette.
///
/// `None` or an unknown theme string falls back to the light palette,
/// preserving print-friendly headless rendering by default.
pub fn get_palette(theme: Option<&str>) -> &'static Palette {
    match theme {
        Some("dark-accessible") => &PALETTE_DARK_ACCESSIBLE,
        Some("grey-accessible") => &PALETTE_GREY_ACCESSIBLE,
        Some("dark") => &PALETTE_DARK,
        Some("grey") => &PALETTE_GREY,
        Some("light") | None => &PALETTE_LIGHT,
        Some(_) => &PALETTE_LIGHT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_resolution_selects_light_palette() {
        assert_eq!(get_palette(None).name, "light");
        assert_eq!(get_palette(Some("unknown")).name, "light");
    }

    #[test]
    fn explicit_theme_names_resolve_correctly() {
        assert_eq!(get_palette(Some("light")).name, "light");
        assert_eq!(get_palette(Some("grey")).name, "grey");
        assert_eq!(get_palette(Some("dark")).name, "dark");
        assert_eq!(get_palette(Some("dark-accessible")).name, "dark-accessible");
    }

    #[test]
    fn desktop_palette_boundaries_meet_non_text_contrast() {
        for palette in [&PALETTE_DARK_ACCESSIBLE, &PALETTE_GREY_ACCESSIBLE] {
            let background = crate::scene::Color::from_hex(palette.bg);
            let panel = crate::scene::Color::from_hex(palette.panel);
            assert!(
                crate::scene::Color::from_hex(palette.spine).contrast_against(background) >= 3.0
            );
            assert!(crate::scene::Color::from_hex(palette.border).contrast_against(panel) >= 3.0);
        }
    }

    #[test]
    fn only_light_and_dark_are_parity_themes() {
        assert!(is_parity_theme("light"));
        assert!(is_parity_theme("dark"));
        assert!(!is_parity_theme("grey"));
    }
}
