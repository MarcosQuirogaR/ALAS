// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Isolated-component wireframes drawn from the built section lists.

use super::*;
use crate::families::geometry::SceneComponent;

/// Chord below which an end section is a degenerate sliver, as a share of the
/// widest chord.
const DEGENERATE_CHORD_SHARE: f64 = 0.02;

/// The wing with degenerate end sections dropped.
///
/// A section whose chord has collapsed at the root or the tip draws as a
/// sliver or a spike. The span is therefore clamped to the sections that carry
/// a real chord; nothing between them is touched.
pub(super) fn trimmed_wing(wing: &Wing) -> Wing {
    let mut trimmed = wing.clone();
    let widest = trimmed
        .xsecs
        .iter()
        .map(|section| section.chord)
        .fold(0.0_f64, f64::max);
    let floor = DEGENERATE_CHORD_SHARE * widest;
    while trimmed.xsecs.len() > 2 && trimmed.xsecs.last().is_some_and(|s| s.chord < floor) {
        trimmed.xsecs.pop();
    }
    while trimmed.xsecs.len() > 2 && trimmed.xsecs.first().is_some_and(|s| s.chord < floor) {
        trimmed.xsecs.remove(0);
    }
    trimmed
}

/// The fuselage as its own station profile: the bulkhead outlines, longerons
/// and centreline the three-view draws, seen from the shared isolated camera.
///
/// The three-view takes its shape from the built stations without a spline
/// between them, so a profile cannot overshoot them and the aft station closes
/// the tail. A shape that shows here is the built geometry.
pub(super) fn fuselage_wire_scene(plane: &Airplane, theme: Option<&str>) -> Scene {
    let palette = get_palette(theme);
    let (width, height) = (600.0, 450.0);
    let viewport = (
        20.0,
        HEADER_HEIGHT,
        width - 40.0,
        height - HEADER_HEIGHT - 20.0,
    );
    let camera = ISOLATED_CAMERA;
    let points = airplane_fit_points(plane);
    let center = camera.fit_center_to_points(&points, [0.0; 3]);
    let extent = camera.fit_span_to_points(&points, viewport, WIREFRAME_PADDING);
    let mut scene = Scene::new(width, height, Some(Color::from_hex(palette.bg)));
    for (index, fus) in plane.fuselages.iter().enumerate() {
        draw_fuselage_wireframe(
            &mut scene,
            &camera,
            center,
            extent,
            viewport,
            fus,
            SceneComponent::of_fuselage(index, fus).color(),
        );
    }
    title_text(
        &mut scene,
        "Fuselage Wireframe",
        Color::from_hex(palette.title),
    );
    scene.render_title = true;
    scene
}
