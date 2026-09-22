// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Bundled display typefaces shared by every desktop rendering backend.
//!
//! The GUI's text atlas and the SVG rasterizer receive these same immutable
//! byte slices. Keeping the fonts in one crate prevents engineering symbols
//! from silently falling back to a host-dependent font in figure previews.

/// Stable key used when registering the proportional face with `egui`.
pub const PROPORTIONAL_FONT_KEY: &str = "alas-noto-sans";

/// Stable key used when registering the monospaced face with `egui`.
pub const MONOSPACE_FONT_KEY: &str = "alas-noto-sans-mono";

/// Stable key used when registering the mathematical fallback with `egui`.
pub const MATH_FONT_KEY: &str = "alas-noto-sans-math";

/// Family name encoded inside the bundled Noto Sans font.
pub const PROPORTIONAL_FAMILY: &str = "Noto Sans";

/// Family name encoded inside the bundled Noto Sans Mono font.
pub const MONOSPACE_FAMILY: &str = "Noto Sans Mono";

/// Family name encoded inside the bundled Noto Sans Math font.
pub const MATH_FAMILY: &str = "Noto Sans Math";

/// CSS family stack written into report SVG text elements.
///
/// The first two entries match the GUI's proportional stack; `sans-serif`
/// preserves readable text for an SVG opened outside ALAS without its bundled
/// assets installed.
pub const SVG_FONT_FAMILY: &str = "'Noto Sans', 'Noto Sans Math', sans-serif";

/// Noto Sans variable font for proportional user-interface text.
pub const PROPORTIONAL_FONT_BYTES: &[u8] = include_bytes!("../assets/NotoSans-Variable.ttf");

/// Noto Sans Mono variable font for structured values and console text.
pub const MONOSPACE_FONT_BYTES: &[u8] = include_bytes!("../assets/NotoSansMono-Variable.ttf");

/// Noto Sans Math font used after the primary text face for engineering glyphs.
pub const MATH_FONT_BYTES: &[u8] = include_bytes!("../assets/NotoSansMath-Regular.ttf");

/// Immutable Google Fonts source revision used for the bundled font files.
pub const GOOGLE_FONTS_SOURCE_REVISION: &str = "e44c4b011a820c2cbe2fd2cfa8052037d7edb571";
