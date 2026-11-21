// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Tone of the airframe drawn as a quiet background behind a cutaway.

use alas_report::scene::Color;

/// How far the fuselage is pushed into the background of an airframe render
/// behind a cutaway: `(desaturation, fade towards the canvas)`, each 0..1.
pub(super) const FUSELAGE_BACKGROUND: (f64, f64) = (0.65, 0.28);

/// Opacity of the grey the wings, nacelles and tail surfaces are drawn with.
const BACKDROP_OPACITY: f64 = 0.40;

/// The flat, light, low-contrast grey of a backdrop surface: a mid grey
/// (light on a dark canvas, dark on a light one) at [`BACKDROP_OPACITY`]
/// over `canvas`, so it is never darker than the canvas on a dark theme and
/// no shading patch shows through the cabin.
pub(super) fn backdrop_grey(canvas: Color) -> Color {
    let luma =
        0.299 * f64::from(canvas.r) + 0.587 * f64::from(canvas.g) + 0.114 * f64::from(canvas.b);
    let target = if luma < 128.0 { 140.0 } else { 90.0 };
    let channel = |value: u8| {
        (f64::from(value) + (target - f64::from(value)) * BACKDROP_OPACITY)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Color::rgba(
        channel(canvas.r),
        channel(canvas.g),
        channel(canvas.b),
        canvas.a,
    )
}

/// Whether a component is a backdrop surface (everything but the fuselage).
pub(super) fn is_backdrop(component: alas_report::families::geometry::SceneComponent) -> bool {
    !matches!(
        component,
        alas_report::families::geometry::SceneComponent::Fuselage
    )
}

/// `colour` desaturated by `desaturate` and mixed `fade` of the way to
/// `canvas` (opaque, so overlapping surfaces never show through each other).
pub(super) fn pushed_back(colour: Color, canvas: Color, (desaturate, fade): (f64, f64)) -> Color {
    let luma =
        0.299 * f64::from(colour.r) + 0.587 * f64::from(colour.g) + 0.114 * f64::from(colour.b);
    let channel = |value: u8, target: u8| {
        let desaturated = f64::from(value) + (luma - f64::from(value)) * desaturate;
        (desaturated + (f64::from(target) - desaturated) * fade)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Color::rgba(
        channel(colour.r, canvas.r),
        channel(colour.g, canvas.g),
        channel(colour.b, canvas.b),
        colour.a,
    )
}
