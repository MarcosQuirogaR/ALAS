// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The framed airframe renders with the wings, nacelles and tail surfaces
//! pushed into the background, behind a cutaway or an overlay.

use alas_report::scene::{Camera3D, Color, Scene};

use super::background_tone as tone;
use super::front_view::{BEHIND_AZIM_DEG, FRAMED_CANVAS_MARGIN, SIDE_AZIM_DEG};
use super::{live_preview_cache, live_preview_options};

/// [`build_framed_view`] / [`build_framed_side_view`] with the wings, nacelles
/// and tail surfaces pushed into the background (`side` picks the camera).
fn build_framed_background(
    geometry: alas_config::GeometryConfig,
    design: alas_config::DesignVector,
    theme: &str,
    viewport_px: (f64, f64),
    centre: (f64, f64),
    px_per_m: f64,
    side: bool,
) -> Option<(Scene, alas_report::families::geometry::SceneFraming)> {
    let model = live_preview_cache::model(super::interactive_geometry_of(geometry), design)?;
    let mut options = live_preview_options();
    options.canvas = (
        viewport_px.0 + 2.0 * FRAMED_CANVAS_MARGIN,
        viewport_px.1 + 2.0 * FRAMED_CANVAS_MARGIN,
    );
    let extent = viewport_px.0.min(viewport_px.1) * 0.45 / px_per_m.max(1.0e-9);
    options.reference = Some(alas_report::families::geometry::FramingReference {
        center: if side {
            [centre.0, 0.0, centre.1]
        } else {
            [0.0, centre.0, centre.1]
        },
        extent,
    });
    let camera = Camera3D {
        elev_deg: 0.0,
        azim_deg: if side { SIDE_AZIM_DEG } else { BEHIND_AZIM_DEG },
        zoom: 1.0,
    };
    let canvas = Color::from_hex(alas_report::theme::get_palette(Some(theme)).bg);
    let grey = tone::backdrop_grey(canvas);
    Some(model.render_toned(
        Some(camera),
        Some(theme),
        &options,
        &|component, colour| {
            if tone::is_backdrop(component) {
                grey
            } else {
                tone::pushed_back(colour, canvas, tone::FUSELAGE_BACKGROUND)
            }
        },
        &tone::is_backdrop,
    ))
}

/// [`build_framed_view`] with the airframe as a quiet background.
pub fn build_framed_background_view(
    geometry: alas_config::GeometryConfig,
    design: alas_config::DesignVector,
    theme: &str,
    viewport_px: (f64, f64),
    centre_yz: (f64, f64),
    px_per_m: f64,
) -> Option<(Scene, alas_report::families::geometry::SceneFraming)> {
    build_framed_background(
        geometry,
        design,
        theme,
        viewport_px,
        centre_yz,
        px_per_m,
        false,
    )
}

/// [`build_framed_side_view`] with the airframe as a quiet background.
pub fn build_framed_background_side_view(
    geometry: alas_config::GeometryConfig,
    design: alas_config::DesignVector,
    theme: &str,
    viewport_px: (f64, f64),
    centre_xz: (f64, f64),
    px_per_m: f64,
) -> Option<(Scene, alas_report::families::geometry::SceneFraming)> {
    build_framed_background(
        geometry,
        design,
        theme,
        viewport_px,
        centre_xz,
        px_per_m,
        true,
    )
}
