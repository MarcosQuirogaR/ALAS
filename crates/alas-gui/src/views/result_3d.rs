// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Orbit interaction shared by three-dimensional result cards and fullscreen views.

use alas_config::AlasConfig;
use alas_report::scene::Scene;
use alas_viz::ViewportTransform;
use egui::{vec2, Response, Ui, Vec2};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::state::{AppState, PreviewCamera};

pub(crate) const MISSION_ROUTE_3D: &str = "mission_route_3d";
/// Closest maximized-view zoom for the route globe.
///
/// The globe texture and every plotted object are now confined to the
/// figure's 720 x 450 point viewport, so the bound is set by image detail
/// rather than by contour bleed: the bundled Blue Marble raster is
/// 4096 x 2048, which is one texture pixel per scene point at a globe radius
/// of 4096 / (2 pi) ~= 652 points, and the route figure's radius is
/// 180.8 points per unit of zoom. 4.0 therefore reaches about 723 points,
/// roughly 1.1x the texture's own resolution.
pub(crate) const MAX_ROUTE_ZOOM: f64 = 4.0;

pub(crate) struct OrbitResponse {
    pub(crate) camera_changed: bool,
    pub(crate) double_clicked: bool,
}

pub(crate) fn is_orbitable_result(id: &str) -> bool {
    id == MISSION_ROUTE_3D
}

/// Whether a retained camera belongs to the route globe.
///
/// Callers pass a composed key (`result_camera::run=..;figure=..`, wrapped
/// again for the maximized overlay), so globe-specific behavior has to be
/// recognized by the figure it ends with rather than by the bare figure id.
fn is_route_camera_key(camera_key: &str) -> bool {
    camera_key == MISSION_ROUTE_3D || camera_key.ends_with(&format!("figure={MISSION_ROUTE_3D}"))
}

/// Camera identity deliberately omits theme so changing palettes does not
/// unexpectedly rotate the globe back to its initial projection.
pub(crate) fn result_camera_key(run_identity: u64, id: &str) -> String {
    format!("result_camera::run={run_identity};figure={id}")
}

/// Resolve the retained result camera, centering a new route-globe camera on
/// the route it is about to display. Other 3-D figures retain the shared
/// isometric fallback.
pub(crate) fn result_camera(
    state: &mut AppState,
    camera_key: &str,
) -> alas_report::scene::Camera3D {
    if !state.result_cameras.contains_key(camera_key) {
        let initial = state
            .pipeline_result
            .as_ref()
            .and_then(|result| result.route.as_ref())
            .map(alas_report::families::mission::route_focused_camera)
            .map(PreviewCamera::from)
            .unwrap_or_default();
        state.result_cameras.insert(camera_key.to_owned(), initial);
    }
    (*state.result_camera_mut(camera_key)).into()
}

pub(crate) fn show_orbit_view(
    state: &mut AppState,
    ui: &mut Ui,
    scene: &Scene,
    camera_key: &str,
    view_key: &str,
    desired_size: Vec2,
    maximized: bool,
) -> OrbitResponse {
    if maximized && is_route_camera_key(camera_key) {
        clamp_maximized_route_zoom(state, camera_key);
    }
    let remaining = ui.available_size();
    let desired_size = egui::vec2(
        desired_size.x.min(remaining.x).max(100.0),
        desired_size.y.min(remaining.y).max(100.0),
    );
    let camera = *state.result_camera_mut(camera_key);
    let cache_revision = camera_cache_revision(camera, maximized);
    let response = ui.add(
        alas_viz::SceneView::new(scene, state.view_state_mut(view_key.to_owned()))
            .desired_size(desired_size)
            .orbit_only()
            // Route zoom belongs to the 3-D camera. Canvas zoom scales the
            // cached PNG and moves the screen-anchored colorbar out of view.
            .wheel_zoom(false)
            .raster_scale(if maximized { 1.5 } else { 1.25 })
            // Only the globe texture is rasterized per camera frame; the
            // route, labels and colorbar are drawn as egui shapes. The SVG
            // round-trip of that overlay was the 80 ms that made orbiting
            // the globe run at 8 frames per second.
            .vector_overlay(true)
            .cache_key(view_key)
            // The view key already captures run/config/theme. Camera bits are
            // a cheap revision and avoid hashing the dense globe scene every frame.
            .cache_revision(cache_revision)
            .show_toolbar(false),
    );
    let orbit_changed = handle_camera_response(state, &response, scene, camera_key);
    let scroll_y = response.ctx.input(|input| input.smooth_scroll_delta.y);
    let zoom_changed = should_apply_camera_zoom(maximized, response.hovered(), scroll_y);
    if zoom_changed {
        let factor = f64::from((1.0 + scroll_y * 0.0015).clamp(0.5, 1.5));
        state
            .result_camera_mut(camera_key)
            .apply_zoom_factor(factor);
        if is_route_camera_key(camera_key) {
            clamp_maximized_route_zoom(state, camera_key);
        }
    }
    OrbitResponse {
        camera_changed: orbit_changed || zoom_changed,
        double_clicked: response.double_clicked(),
    }
}

pub(crate) fn rebuild_scene(
    state: &mut AppState,
    cache_key: &str,
    camera_key: &str,
    config: &AlasConfig,
    theme: &str,
) {
    let camera = (*state.result_camera_mut(camera_key)).into();
    let _ =
        state.rebuild_result_figure_with_camera(cache_key, MISSION_ROUTE_3D, config, theme, camera);
}

fn handle_camera_response(
    state: &mut AppState,
    response: &Response,
    scene: &Scene,
    camera_key: &str,
) -> bool {
    // A double-click changes only the containing view.  Its pointer sequence
    // must not also be interpreted as a final orbit gesture, otherwise a
    // tiny click displacement changes the camera as fullscreen opens.
    if !should_apply_orbit(
        response.double_clicked(),
        response.dragged_by(egui::PointerButton::Primary)
            || response.dragged_by(egui::PointerButton::Middle),
    ) {
        return false;
    }
    let delta = response.drag_motion();
    if is_route_camera_key(camera_key)
        && apply_anchored_globe_drag(state, response, scene, camera_key, delta)
    {
        return true;
    }
    state
        .result_camera_mut(camera_key)
        .apply_orbit_motion(delta);
    delta.is_finite() && delta != vec2(0.0, 0.0)
}

/// Drag the globe by its surface instead of by a fixed number of degrees per
/// pixel, so the grabbed location follows the pointer at any zoom.
///
/// The anchor is re-picked from the previous pointer position each frame
/// rather than stored, which keeps the gesture exact while the pointer is on
/// the globe and lets the clamped horizon, pole and outside-pointer cases
/// recover on the following frame. The route view disables canvas pan and
/// wheel zoom, so the widget's fit transform is the whole scene-to-screen
/// mapping. Returns false when the gesture has no anchored solution and the
/// caller should fall back to the angular sweep.
fn apply_anchored_globe_drag(
    state: &mut AppState,
    response: &Response,
    scene: &Scene,
    camera_key: &str,
    delta: Vec2,
) -> bool {
    if !delta.is_finite() || delta == vec2(0.0, 0.0) {
        return false;
    }
    let Some(pointer) = response.interact_pointer_pos() else {
        return false;
    };
    let transform = ViewportTransform::fit(scene.width, scene.height, response.rect);
    let to = transform.to_canvas(pointer);
    let from = transform.to_canvas(pointer - delta);
    let camera = *state.result_camera_mut(camera_key);
    let (center, radius) = alas_report::families::mission::route_globe_disk(&camera.into());
    state
        .result_camera_mut(camera_key)
        .apply_surface_anchored_drag(from, to, center, radius)
}

fn should_apply_orbit(double_clicked: bool, dragged: bool) -> bool {
    dragged && !double_clicked
}

fn should_apply_camera_zoom(maximized: bool, hovered: bool, scroll_y: f32) -> bool {
    maximized && hovered && scroll_y.is_finite() && scroll_y.abs() > f32::EPSILON
}

fn clamp_maximized_route_zoom(state: &mut AppState, camera_key: &str) {
    let camera = state.result_camera_mut(camera_key);
    camera.zoom = if camera.zoom.is_finite() {
        camera.zoom.clamp(0.15, MAX_ROUTE_ZOOM)
    } else {
        1.0
    };
}

fn camera_cache_revision(camera: PreviewCamera, maximized: bool) -> u64 {
    let mut hasher = DefaultHasher::new();
    camera.pitch_deg.to_bits().hash(&mut hasher);
    camera.yaw_deg.to_bits().hash(&mut hasher);
    camera.zoom.to_bits().hash(&mut hasher);
    maximized.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::{
        camera_cache_revision, is_orbitable_result, is_route_camera_key, result_camera,
        result_camera_key, should_apply_camera_zoom, should_apply_orbit, MAX_ROUTE_ZOOM,
        MISSION_ROUTE_3D,
    };
    use crate::state::{AppState, PreviewCamera};

    #[test]
    fn only_the_route_globe_uses_result_orbit_interaction() {
        assert!(is_orbitable_result(MISSION_ROUTE_3D));
        assert!(!is_orbitable_result("mission_route_2d"));
        assert!(!is_orbitable_result("mass_breakdown"));
    }

    #[test]
    fn result_camera_identity_is_independent_by_run_and_figure() {
        assert_ne!(
            result_camera_key(7, MISSION_ROUTE_3D),
            result_camera_key(8, MISSION_ROUTE_3D)
        );
        assert_ne!(
            result_camera_key(7, MISSION_ROUTE_3D),
            result_camera_key(7, "future_3d_result")
        );
    }

    #[test]
    fn a_result_without_a_route_uses_the_isometric_camera_fallback() {
        let mut state = AppState::default();
        let camera = result_camera(&mut state, "route");
        assert_eq!(camera, PreviewCamera::isometric().into());
    }

    #[test]
    fn fullscreen_double_click_does_not_also_orbit_the_camera() {
        assert!(!should_apply_orbit(true, true));
        assert!(!should_apply_orbit(true, false));
        assert!(should_apply_orbit(false, true));
    }

    #[test]
    fn route_wheel_zoom_is_reserved_for_the_maximized_view() {
        assert!(!should_apply_camera_zoom(false, true, 20.0));
        assert!(!should_apply_camera_zoom(true, false, 20.0));
        assert!(!should_apply_camera_zoom(true, true, 0.0));
        assert!(should_apply_camera_zoom(true, true, 20.0));
    }

    #[test]
    fn route_texture_revision_tracks_camera_and_quality_tier() {
        let camera = PreviewCamera::top();
        let mut zoomed = camera;
        zoomed.apply_zoom_factor(1.2);
        assert_ne!(
            camera_cache_revision(camera, false),
            camera_cache_revision(zoomed, false)
        );
        assert_ne!(
            camera_cache_revision(camera, false),
            camera_cache_revision(camera, true)
        );
    }

    #[test]
    fn the_globe_camera_is_recognized_through_the_composed_and_maximized_keys() {
        let embedded = result_camera_key(7, MISSION_ROUTE_3D);
        let maximized = format!("fullscreen_camera::{embedded}");
        assert!(is_route_camera_key(&embedded));
        assert!(is_route_camera_key(&maximized));
        assert!(is_route_camera_key(MISSION_ROUTE_3D));
        assert!(!is_route_camera_key(&result_camera_key(
            7,
            "mass_breakdown"
        )));
    }

    #[test]
    fn maximized_route_zoom_is_capped_at_the_globe_textures_own_resolution() {
        let mut state = AppState::default();
        state.result_cameras.insert(
            "route".to_owned(),
            PreviewCamera {
                zoom: 8.0,
                ..PreviewCamera::top()
            },
        );
        super::clamp_maximized_route_zoom(&mut state, "route");
        assert_eq!(state.result_cameras["route"].zoom, MAX_ROUTE_ZOOM);
    }
}
