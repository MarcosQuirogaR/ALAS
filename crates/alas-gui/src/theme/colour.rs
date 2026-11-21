// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Colour arithmetic and per-theme widget colours for the egui visuals.

use super::*;

pub(super) fn blend_color(from: Color32, to: Color32, amount: f32) -> Color32 {
    let mix = |start: u8, end: u8| {
        (f32::from(start) + (f32::from(end) - f32::from(start)) * amount).round() as u8
    };
    Color32::from_rgba_premultiplied(
        mix(from.r(), to.r()),
        mix(from.g(), to.g()),
        mix(from.b(), to.b()),
        mix(from.a(), to.a()),
    )
}

pub(super) fn selection_foreground(accent: Color32) -> Color32 {
    if contrast_ratio(Color32::BLACK, accent) >= contrast_ratio(Color32::WHITE, accent) {
        Color32::BLACK
    } else {
        Color32::WHITE
    }
}

/// How far a hovered control's tint sits from the selected fill, toward the
/// panel it floats on. Selected, hovered, pressed/focused and open were all
/// drawn with one saturated accent, so an open menu, a hovered tab and the
/// actually selected tab were indistinguishable.
pub(super) const HOVER_TINT_TOWARD_PANEL: f32 = 0.72;

/// The same blend for a pressed, keyboard-focused or open control: stronger
/// than hover, still clearly weaker than a selected fill, and additionally
/// carrying the wider `active` ring.
pub(super) const PRESSED_TINT_TOWARD_PANEL: f32 = 0.40;

/// The fill behind a *selected* control.
///
/// `apply_theme` sets `override_text_color` to the palette title colour, and
/// egui resolves a plain or strong widget label through that override before
/// any per-widget fallback (`RichText::get_text_color`,
/// `Visuals::strong_text_color`). The foreground on a selected control is
/// therefore the theme's own text colour - white in Dark and Grey, black in
/// Light - and the fill has to clear WCAG 2.1 AA 4.5:1 against *that*, not
/// against white everywhere. The palette accents did not: `#4f8cff` with white
/// is 3.22:1 and `#6aa2ff` with white is 2.55:1.
pub(super) fn accent_fill(theme: AppTheme) -> Color32 {
    match theme {
        // White label text: 5.17:1.
        AppTheme::Dark | AppTheme::Grey => Color32::from_rgb(0x25, 0x63, 0xEB),
        // Black label text: 6.50:1, and 3.23:1 against the white page, so the
        // filled control keeps a 1.4.11 boundary even without its stroke.
        AppTheme::Light => Color32::from_rgb(0x5B, 0x8D, 0xEF),
    }
}

/// The accent used for *text* and thin marks: section headings, the navigation
/// rail, sparkline strokes and editor guide lines.
///
/// egui's default `hyperlink_color` is `#009bff`, which is 2.94:1 on white, so
/// every accent section heading on the Light page failed AA. Each value below
/// clears 4.5:1 against both the page background and the card surface of its
/// own theme.
pub(super) fn accent_text(theme: AppTheme) -> Color32 {
    match theme {
        // 6.85:1 on #1e1e1e, 5.92:1 on #262a31.
        AppTheme::Dark => Color32::from_rgb(0x5A, 0xAA, 0xFF),
        // 5.67:1 on #3a3a3a, 4.80:1 on #41454c.
        AppTheme::Grey => Color32::from_rgb(0x8F, 0xB8, 0xFF),
        // 5.17:1 on #ffffff, 4.74:1 on #f4f5f7.
        AppTheme::Light => Color32::from_rgb(0x25, 0x63, 0xEB),
    }
}

/// Blend `accent` `amount` of the way into `surface`, then keep stepping
/// toward the surface until `foreground` clears the AA body-text threshold on
/// the result. Hover and pressed states are tints of the selected fill rather
/// than copies of it, which is what makes the interaction states separable;
/// the loop guarantees the label on them stays readable.
pub(super) fn accent_tint(
    accent: Color32,
    surface: Color32,
    foreground: Color32,
    amount: f32,
) -> Color32 {
    let mut step = amount.clamp(0.0, 1.0);
    let mut tint = blend_color(accent, surface, step);
    while contrast_ratio(foreground, tint) < 4.5 && step < 1.0 {
        step = (step + 0.02).min(1.0);
        tint = blend_color(accent, surface, step);
    }
    tint
}

pub(super) fn widget_border(theme: AppTheme, pal: &Palette) -> Color32 {
    match theme {
        // The print palette's pale rule is intentionally subtle. Interactive
        // controls need a 3:1 boundary against the desktop panel instead.
        AppTheme::Light => Color32::from_rgb(128, 128, 128),
        AppTheme::Dark | AppTheme::Grey => hex_to_color32(pal.border),
    }
}

pub(super) fn input_fill(theme: AppTheme) -> Color32 {
    match theme {
        // Keep numeric fields visibly separate from the panel without turning
        // the dense engineering forms into a collection of bright cards.
        AppTheme::Dark => Color32::from_rgb(48, 53, 62),
        AppTheme::Grey => Color32::from_rgb(54, 59, 68),
        AppTheme::Light => Color32::from_rgb(255, 255, 255),
    }
}

pub(super) fn input_border(theme: AppTheme) -> Color32 {
    match theme {
        AppTheme::Dark => Color32::from_rgb(158, 166, 179),
        AppTheme::Grey => Color32::from_rgb(202, 208, 218),
        AppTheme::Light => Color32::from_rgb(107, 114, 128),
    }
}

pub(super) fn disabled_text_color(theme: AppTheme) -> Color32 {
    match theme {
        // Disabled controls remain deliberately subdued, but still meet the
        // same text contrast threshold as small explanatory copy.
        AppTheme::Dark => Color32::from_rgb(190, 198, 210),
        AppTheme::Grey => Color32::from_rgb(222, 226, 232),
        AppTheme::Light => Color32::from_rgb(72, 81, 96),
    }
}

pub(super) fn contrast_ratio(a: Color32, b: Color32) -> f32 {
    fn luminance(color: Color32) -> f32 {
        let channel = |value: u8| {
            let value = f32::from(value) / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
    }
    let (lighter, darker) = if luminance(a) >= luminance(b) {
        (luminance(a), luminance(b))
    } else {
        (luminance(b), luminance(a))
    };
    (lighter + 0.05) / (darker + 0.05)
}
