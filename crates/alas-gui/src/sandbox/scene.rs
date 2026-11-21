// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The sandbox viewport scene and the derived geometry metrics beside it.
//!
//! The scene is built from the committed configuration only: a camera
//! change re-projects the same model, a model edit rebuilds it. Isolation
//! is a view filter derived from the focused discipline.
//!
//! Metric conventions shown at the bottom left of the workspace:
//! * reference wing area is the builder's `s_ref`, the projected planform
//!   area of the main wing including the carry-through between the root
//!   stations, in square metres;
//! * span is the projected tip-to-tip distance in metres;
//! * mean aerodynamic chord is the builder's `c_ref` in metres;
//! * leading-edge sweep is the inboard leading-edge sweep design variable in
//!   degrees, positive aft; the quarter-chord sweep is the area-weighted
//!   mean over the lofted sections;
//! * aspect ratio is `span^2 / s_ref` and taper is tip over root chord.

use alas_config::{AlasConfig, DesignVector};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::builder::AircraftBuilder;
use alas_report::families::geometry::{
    SandboxSceneModel, SandboxSceneOptions, SceneComponent, SceneFraming,
};
use alas_report::scene::{Camera3D, Scene};

use crate::state::AppState;

use super::fields::Discipline;

#[path = "live_preview_cache.rs"]
mod live_preview_cache;
/// The dock's exterior model is built on worker threads (see the module).
pub(crate) use live_preview_cache::{
    background_pending as live_model_pending,
    build_in_background as build_live_models_in_background, poll_background as poll_live_models,
};

#[path = "framed_background.rs"]
mod framed_background;

#[path = "background_tone.rs"]
mod background_tone;
#[path = "front_view.rs"]
mod front_view;
pub use framed_background::{build_framed_background_side_view, build_framed_background_view};
pub use front_view::{build_framed_seat_layer, build_framed_side_view};
pub use front_view::{
    build_framed_view, build_front_view, build_front_view_from, point_in_polygon, WingMarker,
    BEHIND_AZIM_DEG, FRAMED_CANVAS_MARGIN,
};

/// Camera key of the sandbox viewport.
pub const SANDBOX_CAMERA_ID: &str = "sandbox_3d";
/// Viewport key of the sandbox canvas.
pub const SANDBOX_VIEW_KEY: &str = "sandbox::aircraft_3d";
/// Outline points per lofted section in the interactive preview.
pub const SECTION_POINTS: usize = 40;
/// The canvas drawn before the viewport has reported its size, in points.
pub const DEFAULT_CANVAS: (f64, f64) = (600.0, 500.0);

/// The component a discipline isolates in the preview.
pub fn discipline_component(discipline: Discipline) -> SceneComponent {
    match discipline {
        Discipline::Wing => SceneComponent::Wing,
        Discipline::HorizontalTail => SceneComponent::HorizontalTail,
        Discipline::VerticalTail => SceneComponent::VerticalTail,
        Discipline::Fuselage => SceneComponent::Fuselage,
        Discipline::Propulsion => SceneComponent::Nacelles,
    }
}

/// The interactive preview mesh: only the defining planform stations, so a
/// drag frame does not pay for solver-resolution subdivisions.
fn interactive_geometry(config: &AlasConfig) -> alas_config::GeometryConfig {
    interactive_geometry_of(config.geometry.clone())
}

/// [`interactive_geometry`] for a geometry already in hand.
fn interactive_geometry_of(
    mut geometry: alas_config::GeometryConfig,
) -> alas_config::GeometryConfig {
    geometry.wing.n_subdivisions = geometry.wing.n_subdivisions.clamp(1, 2);
    geometry.empennage.n_subdivisions = geometry.empennage.n_subdivisions.clamp(1, 2);
    geometry
}

/// Build the sandbox aircraft from the committed model, or `None` when it
/// does not build.
pub fn build_sandbox_airplane(state: &AppState) -> Option<(Airplane, DesignVector)> {
    try_build_sandbox_airplane(state).ok()
}

/// As [`build_sandbox_airplane`], with the reason it does not build.
pub fn try_build_sandbox_airplane(state: &AppState) -> Result<(Airplane, DesignVector), String> {
    let config = state
        .typed_config()
        .ok_or_else(|| "the configuration is not valid".to_owned())?;
    let design = state
        .current_design()
        .ok_or_else(|| "the design variables are not valid".to_owned())?;
    let plane = AircraftBuilder::new(Some(interactive_geometry(&config)))
        .build(Some(&design), true)
        .map_err(|error| error.to_string())?;
    Ok((plane, design))
}

/// Loft and partition an aircraft for drawing, once per geometry.
pub fn build_sandbox_model(plane: &Airplane) -> SandboxSceneModel {
    SandboxSceneModel::new(plane, SECTION_POINTS)
}

/// Build the sandbox scene for the current camera, focus, theme, viewport
/// and kept framing.
pub fn build_sandbox_scene(state: &AppState) -> Option<(Scene, SceneFraming)> {
    let (plane, _) = build_sandbox_airplane(state)?;
    Some(project_sandbox_model(state, &build_sandbox_model(&plane)))
}

/// Build the same high-detail lofted scene used by the sandbox for the
/// guided live-preview dock. The dock keeps its own camera and viewport
/// state, but shares the exact `SandboxSceneModel` renderer and its 40-point
/// section sampling so the two previews do not disagree about geometry.
pub fn build_live_preview_scene(state: &AppState, camera: Camera3D) -> Option<Scene> {
    let config = state.typed_config()?;
    let design = state.current_design()?;
    let model = live_preview_cache::model_for_dock(interactive_geometry(&config), design)?;
    let mut options = live_preview_options();
    if let Some(fit) = state.preview_fit {
        // The canvas takes the pane's shape, and the framing is the tight fit
        // of the aircraft for the camera the fit was made with. An orbit
        // keeps that framing, so the scale does not change while turning.
        options.canvas = fit.canvas;
        let orbit = dock_orbit_fit(&model, fit);
        let pan = (
            orbit.pan.0 + state.preview_pan.0,
            orbit.pan.1 + state.preview_pan.1,
        );
        options.reference = Some(orbit.tight.pivoted(&camera, orbit.canvas, orbit.pivot, pan));
    }
    Some(
        model
            .render(
                Some(camera),
                Some(state.theme.figure_theme_name()),
                &options,
            )
            .0,
    )
}

thread_local! {
    // The dock's last fit, with the model it was made for held so the model's
    // address cannot be reused by another while the entry lives. An orbit
    // frame reuses it; a resize, a reset or an edit makes a new one.
    static DOCK_FIT: std::cell::RefCell<Option<(std::rc::Rc<SandboxSceneModel>, LivePreviewFit, OrbitFit)>> =
        const { std::cell::RefCell::new(None) };
}

/// The dock's [`OrbitFit`] for `model` and `fit`, made once per pane size,
/// camera fit and geometry rather than once per drawn frame.
fn dock_orbit_fit(model: &std::rc::Rc<SandboxSceneModel>, fit: LivePreviewFit) -> OrbitFit {
    DOCK_FIT.with(|slot| {
        let mut slot = slot.borrow_mut();
        if let Some((cached_model, cached_fit, orbit)) = slot.as_ref() {
            if std::rc::Rc::ptr_eq(cached_model, model) && *cached_fit == fit {
                return *orbit;
            }
        }
        let orbit = OrbitFit::new(model, fit);
        *slot = Some((std::rc::Rc::clone(model), fit, orbit));
        orbit
    })
}

/// Where the dock's pivot sits at its default fit (canvas units from the pane
/// centre), so a zoom towards the cursor can add the user's pan to it.
pub fn dock_fit_pan() -> (f64, f64) {
    DOCK_FIT.with(|slot| {
        slot.borrow()
            .as_ref()
            .map_or((0.0, 0.0), |(_, _, orbit)| orbit.pan)
    })
}

/// Share of the pane's limiting dimension the aircraft fills when the live
/// preview is fitted.
pub const LIVE_PREVIEW_FILL: f64 = 0.92;
/// Margin of the scene viewport inside its canvas, in canvas units per side.
const CANVAS_MARGIN: f64 = 20.0;

/// How the live-preview dock framed its aircraft: the pane's size in points
/// and the camera orientation the tight fit was made for. Set when the dock
/// opens, is resized or is reset; an orbit leaves it alone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LivePreviewFit {
    /// The pane size in points.
    pub canvas: (f64, f64),
    /// Elevation of the camera the fit was made for, degrees.
    pub elev_deg: f64,
    /// Azimuth of the camera the fit was made for, degrees.
    pub azim_deg: f64,
    /// Points kept free at the top and bottom of the pane for what the dock draws over it (the
    /// Interior mode's controls and legend card); the model is fitted between them.
    pub reserved: (f64, f64),
}

/// Share of extra extent the dock and the wing preview keep around the
/// aircraft at the default fit: 1.15 leaves a 0.80 fill of the pane.
pub const DOCK_FIT_MARGIN: f64 = 1.15;

/// The tight fit of a model to a pane for one camera, with the aircraft's
/// bounding-box centre as the point a camera turns about: made once per pane
/// size and geometry, then turned into a framing for any orbit camera.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrbitFit {
    /// The tight framing at the camera the fit was made for.
    pub tight: alas_report::families::geometry::FramingReference,
    /// The centre of the model's bounding box.
    pub pivot: [f64; 3],
    /// Where the pivot sits on the pane at the fit camera, canvas units from
    /// the pane centre.
    pub pan: (f64, f64),
    /// The pane the fit was made for.
    pub canvas: (f64, f64),
}

impl OrbitFit {
    /// Fit `model` to the pane and camera `fit` describes.
    pub fn new(model: &SandboxSceneModel, fit: LivePreviewFit) -> Self {
        let mut tight = fit_reference(model, fit);
        // A margin so the first notches of zoom do not clip a wing tip.
        tight.extent *= DOCK_FIT_MARGIN;
        let fit_camera = Camera3D {
            elev_deg: fit.elev_deg,
            azim_deg: fit.azim_deg,
            zoom: 1.0,
        };
        let pivot = model.framing_reference(None).center;
        let pan = tight.offset_of(&fit_camera, fit.canvas, pivot);
        Self {
            tight,
            pivot,
            pan,
            canvas: fit.canvas,
        }
    }

    /// The framing for `camera`: the pivot keeps its screen place, so an
    /// orbit or a zoom never carries the model out of the pane.
    pub fn reference(
        &self,
        camera: &Camera3D,
    ) -> alas_report::families::geometry::FramingReference {
        self.tight
            .pivoted(camera, self.canvas, self.pivot, self.pan)
    }
}

/// The framing that makes the aircraft fill [`LIVE_PREVIEW_FILL`] of the
/// pane's limiting dimension, centred, as seen from the fit's camera.
fn fit_reference(
    model: &SandboxSceneModel,
    fit: LivePreviewFit,
) -> alas_report::families::geometry::FramingReference {
    let points: Vec<[f64; 3]> = model
        .faces()
        .iter()
        .flat_map(|face| face.points.iter().copied())
        .collect();
    let camera = Camera3D {
        elev_deg: fit.elev_deg,
        azim_deg: fit.azim_deg,
        zoom: 1.0,
    };
    let (width, height) = fit.canvas;
    let target = (
        0.0,
        0.0,
        width * LIVE_PREVIEW_FILL,
        height * LIVE_PREVIEW_FILL,
    );
    let span = camera.fit_span_to_points(&points, target, 0.0);
    // `fit_span_to_points` scales by the smaller side of the viewport it is
    // given; the scene is drawn in its own, smaller, viewport.
    let viewport_min = (width.min(height) - 2.0 * CANVAS_MARGIN).max(1.0);
    let target_min = (width.min(height) * LIVE_PREVIEW_FILL).max(1.0);
    alas_report::families::geometry::FramingReference {
        center: camera.fit_center_to_points(&points, [0.0; 3]),
        extent: (span * viewport_min / target_min).max(1e-6),
    }
}

/// Drawing options for the live-preview dock.
///
/// The dock shares the sandbox's committed aircraft and renderer, but it is
/// its own view.  In particular, a discipline focus or a fitted framing in
/// Sandbox must never make a supposedly full-aircraft live preview disappear
/// or change its camera framing.  Its camera is supplied by the dock and its
/// viewport uses a stable bootstrap canvas until the dock reports its size.
fn live_preview_options() -> SandboxSceneOptions {
    SandboxSceneOptions {
        isolate: None,
        max_section_points: SECTION_POINTS,
        canvas: DEFAULT_CANVAS,
        reference: None,
    }
}

/// The drawing options the current state asks for: the isolated component,
/// the viewport-sized canvas and the framing kept across camera motion and
/// edits (fitted to the shown components when none is kept).
pub fn scene_options(state: &AppState) -> SandboxSceneOptions {
    SandboxSceneOptions {
        isolate: state.sandbox.focus().map(discipline_component),
        max_section_points: SECTION_POINTS,
        canvas: state
            .sandbox
            .viewport_size
            .map_or(DEFAULT_CANVAS, |(w, h)| (f64::from(w), f64::from(h))),
        reference: state.sandbox.framing,
    }
}

/// Draw an already partitioned aircraft for the current camera and options.
pub fn project_sandbox_model(state: &AppState, model: &SandboxSceneModel) -> (Scene, SceneFraming) {
    let camera = state
        .preview_cameras
        .get(SANDBOX_CAMERA_ID)
        .copied()
        .unwrap_or_default();
    let camera: Camera3D = camera.into();
    let mut options = scene_options(state);
    // The camera turns about the pivot, which stays where the pan puts it.
    if let (Some(pivot), Some(reference)) = (state.sandbox.pivot, options.reference) {
        options.reference =
            Some(reference.pivoted(&camera, options.canvas, pivot, state.sandbox.pan));
    }
    model.render(
        Some(camera),
        Some(state.theme.figure_theme_name()),
        &options,
    )
}

/// Derived geometry quantities of the committed aircraft.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeometryMetrics {
    /// Reference wing area in square metres.
    pub reference_area_m2: f64,
    /// Projected span in metres.
    pub span_m: f64,
    /// Mean aerodynamic chord in metres.
    pub mean_aerodynamic_chord_m: f64,
    /// Inboard leading-edge sweep in degrees.
    pub leading_edge_sweep_deg: f64,
    /// Area-weighted mean quarter-chord sweep in degrees.
    pub quarter_chord_sweep_deg: f64,
    /// Aspect ratio.
    pub aspect_ratio: f64,
    /// Tip over root chord.
    pub taper_ratio: f64,
    /// Overall fuselage length in metres.
    pub fuselage_length_m: f64,
}

/// Compute the metrics from an already built aircraft.
pub fn geometry_metrics(plane: &Airplane, design: &DesignVector) -> GeometryMetrics {
    let wing = plane.wings.first();
    GeometryMetrics {
        reference_area_m2: plane.s_ref,
        span_m: plane.b_ref,
        mean_aerodynamic_chord_m: plane.c_ref,
        leading_edge_sweep_deg: design.sweep_deg,
        quarter_chord_sweep_deg: wing.map(|w| w.mean_sweep_angle(0.25)).unwrap_or(f64::NAN),
        aspect_ratio: if plane.s_ref > 0.0 {
            plane.b_ref * plane.b_ref / plane.s_ref
        } else {
            f64::NAN
        },
        taper_ratio: wing.map(|w| w.taper_ratio()).unwrap_or(f64::NAN),
        fuselage_length_m: design.fuselage_length_m,
    }
}

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_and_scene_read_the_same_committed_aircraft() {
        let state = AppState::default();
        let (plane, design) = build_sandbox_airplane(&state).expect("AVE builds");
        let metrics = geometry_metrics(&plane, &design);
        assert!((metrics.span_m - design.span_m).abs() < 1e-6);
        assert!(metrics.reference_area_m2 > 300.0);
        assert!(metrics.aspect_ratio > 5.0 && metrics.aspect_ratio < 15.0);
        assert!((metrics.leading_edge_sweep_deg - 34.0).abs() < 1e-9);
        assert!(metrics.quarter_chord_sweep_deg.is_finite());
    }

    #[test]
    fn focusing_a_discipline_isolates_its_component_without_changing_the_model() {
        let mut state = AppState::default();
        let before = state.config_values.clone();
        let whole = build_sandbox_scene(&state).expect("scene");
        state.sandbox.set_focus(Some(Discipline::Wing));
        let wing = build_sandbox_scene(&state).expect("wing scene");
        assert!(wing.0.elements.len() < whole.0.elements.len());
        assert!(wing.1.max_span < whole.1.max_span);
        assert_eq!(state.config_values, before);
        state.sandbox.set_focus(None);
        let again = build_sandbox_scene(&state).expect("scene");
        assert_eq!(again.0.elements.len(), whole.0.elements.len());
    }

    /// The live-preview dock turns the aircraft about its bounding-box centre:
    /// that point keeps one screen place for every orbit and zoom, and the
    /// whole aircraft is inside the pane at the camera the fit was made for,
    /// whatever the pane's shape.
    #[test]
    fn the_dock_orbits_about_the_bounding_box_centre_and_fits_every_pane() {
        let state = AppState::default();
        let (plane, _) = build_sandbox_airplane(&state).expect("AVE builds");
        let model = build_sandbox_model(&plane);
        let centre = model.framing_reference(None).center;
        for canvas in [
            (430.0, 760.0),
            (300.0, 900.0),
            (900.0, 500.0),
            (1200.0, 1200.0),
        ] {
            let fit = LivePreviewFit {
                canvas,
                elev_deg: 22.0,
                azim_deg: -125.0,
                reserved: (0.0, 0.0),
            };
            let at_fit = Camera3D {
                elev_deg: 22.0,
                azim_deg: -125.0,
                zoom: 1.0,
            };
            let reference = OrbitFit::new(&model, fit).reference(&at_fit);
            let framing = SceneFraming {
                camera: at_fit,
                center: reference.center,
                max_span: reference.extent,
                viewport: (20.0, 20.0, canvas.0 - 40.0, canvas.1 - 40.0),
                canvas,
            };
            for face in model.faces() {
                for p in &face.points {
                    let [x, y] = framing.project(*p);
                    assert!(
                        x >= -0.5 && x <= canvas.0 + 0.5 && y >= -0.5 && y <= canvas.1 + 0.5,
                        "{canvas:?}: ({x:.0}, {y:.0}) is outside the pane"
                    );
                }
            }
            let anchor = framing.project(centre);
            for (elev, azim, zoom) in [(22.0, -60.0, 1.0), (60.0, 140.0, 2.5), (0.0, -90.0, 0.4)] {
                let camera = Camera3D {
                    elev_deg: elev,
                    azim_deg: azim,
                    zoom,
                };
                let reference = OrbitFit::new(&model, fit).reference(&camera);
                let turned = SceneFraming {
                    camera,
                    center: reference.center,
                    max_span: reference.extent,
                    viewport: framing.viewport,
                    canvas,
                };
                let p = turned.project(centre);
                assert!(
                    (p[0] - anchor[0]).abs() < 1e-6 && (p[1] - anchor[1]).abs() < 1e-6,
                    "{canvas:?}: the centre moved from {anchor:?} to {p:?}"
                );
            }
        }
    }
    #[test]
    fn live_preview_ignores_sandbox_focus_and_framing() {
        let mut state = AppState::default();
        let camera = Camera3D::default();
        let whole = build_live_preview_scene(&state, camera).expect("live scene");

        state.sandbox.set_focus(Some(Discipline::Wing));
        state.sandbox.framing = Some(alas_report::families::geometry::FramingReference {
            center: [0.0, 0.0, 0.0],
            extent: 1.0,
        });
        let focused = build_live_preview_scene(&state, camera).expect("live scene");

        assert_eq!(focused, whole);
    }
}
