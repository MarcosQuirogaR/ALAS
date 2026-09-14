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
use alas_report::scene::Scene;

use crate::state::AppState;

use super::fields::Discipline;

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
    let mut geometry = config.geometry.clone();
    geometry.wing.n_subdivisions = geometry.wing.n_subdivisions.clamp(1, 2);
    geometry.empennage.n_subdivisions = geometry.empennage.n_subdivisions.clamp(1, 2);
    geometry
}

/// Build the sandbox aircraft from the committed model, or `None` when it
/// does not build.
pub fn build_sandbox_airplane(state: &AppState) -> Option<(Airplane, DesignVector)> {
    let config = state.typed_config()?;
    let design = state.current_design()?;
    let plane = AircraftBuilder::new(Some(interactive_geometry(&config)))
        .build(Some(&design), true)
        .ok()?;
    Some((plane, design))
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
    model.render(
        Some(camera.into()),
        Some(state.theme.figure_theme_name()),
        &scene_options(state),
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
}
