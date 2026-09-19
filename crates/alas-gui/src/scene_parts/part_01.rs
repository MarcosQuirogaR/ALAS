// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use alas_config::{design_variables::DesignVector, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_geom::wing_structure::WingStructureGeometry;
use alas_payload::{apply_cabin_preset, build::build_payload_layout};
use alas_perf::performance::build_vn_diagram;
use alas_report::families::aerodynamics::{
    figure_aero_panel, figure_airfoil_reynolds, figure_drag_breakdown, figure_model_comparison,
    figure_mses_convergence, figure_mses_cp_contours, figure_mses_mach_contours,
    figure_mses_pressure_distribution, figure_optimized_aircraft_comparison,
    figure_polar_comparison, figure_span_loading, figure_status_message, figure_vlm_flow,
    figure_vspaero_load_distribution, figure_vspaero_polar, figure_vspaero_wake_convergence,
};
use alas_report::families::geometry::{
    figure_airfoil_evolution, figure_design_evolution, figure_exterior_3d, figure_geometry,
    figure_openvsp_cad_preview, figure_planform_comparison, figure_threeview,
    figure_wireframe_empennage, figure_wireframe_fuselage, figure_wireframe_wing,
};
use alas_report::families::mass_balance::{
    figure_cg_envelope, figure_landing_gear_planform, figure_mass_breakdown,
    figure_mass_distribution,
};
use alas_report::families::mass_balance_layout::{
    figure_cabin_cross_section, figure_fuel_volume_check_for_loading,
};
use alas_report::families::mission::{
    figure_mission_aero_coefficients, figure_mission_aero_forces, figure_mission_drag_components,
    figure_mission_flight_path, figure_mission_profile, figure_mission_route_2d,
    figure_mission_route_3d, figure_mission_velocities,
};
use alas_report::families::optimization::figure_airfoil_comparison;
use alas_report::families::optimization::figure_optimization_history;
use alas_report::families::performance::{
    figure_lto_for_airport_at_masses, figure_matching_chart, figure_payload_range,
    figure_vn_diagram,
};
use alas_report::families::propulsion::{
    figure_propulsion_altitude_sweep, figure_propulsion_bpr_sensitivity,
    figure_propulsion_carpet_plot, figure_propulsion_cycle_summary,
    figure_propulsion_efficiency_decomposition,
};
use alas_report::families::screening::{
    fig_mses_verification, fig_ranking_bars, fig_rerank_2d_3d, fig_section_shapes, fig_trade_map,
};
use alas_report::families::stability::figure_stability_side_view;
use alas_report::families::stability::{figure_control_surfaces, figure_dynamic_modes};
use alas_report::families::structures::figure_structures_designer_preview;
use alas_report::families::structures::{figure_structures_loads, figure_structures_sizing};
use alas_report::families::structures_dynamics::figure_structures_stress;
use alas_report::families::structures_dynamics::{
    figure_structures_modes, figure_structures_patran, figure_structures_vibration,
};
use alas_report::scene::{Camera3D, Scene, SceneElement};
use alas_struct::sizing::size_wingbox;

use crate::state::{AppState, PreviewTab};
#[path = "../scene_solver.rs"]
mod scene_solver;
use scene_solver::{
    selected_avl_result, selected_optimization_result, selected_result_report, solutions_avl_result,
};

#[path = "../dispatch_ids.rs"]
mod dispatch_ids;
#[path = "../scene/localization.rs"]
mod localization;
pub use dispatch_ids::{PREVIEW_DISPATCH_IDS, RESULT_DISPATCH_IDS, SCREENING_DISPATCH_IDS};
use localization::localize_scene_text;

/// Translate a report scene for the active desktop language before display.
///
/// The report crate remains language-neutral so exported reports may select
/// their own language. The GUI applies this final display boundary to titles,
/// labels and supported dynamic footers.
pub fn localize_scene_for_display(mut scene: Scene) -> Scene {
    if let Some(title) = &mut scene.title {
        *title = localize_scene_text(title);
    }
    for element in &mut scene.elements {
        match element {
            SceneElement::Text { text, .. } | SceneElement::TextBlock { text, .. } => {
                *text = localize_scene_text(text);
            }
            _ => {}
        }
    }
    scene
}

/// Build the identity of a rendered figure cache entry.
///
/// A figure is not identified by its display id alone: a new run can publish
/// different data under the same id, and changing configuration or theme must
/// invalidate the prior scene.
pub fn figure_cache_key(
    run_identity: u64,
    config: &AlasConfig,
    theme: &str,
    figure_id: &str,
) -> String {
    let configuration = match serde_json::to_string(config) {
        Ok(value) => value,
        Err(_) => "<unserializable-config>".to_owned(),
    };
    let language = alas_i18n::get_language();
    format!(
        "run={run_identity};config={configuration};theme={theme};language={language};figure={figure_id}"
    )
}

/// The design vector the previews are built at.
fn preview_design(state: &AppState) -> DesignVector {
    state.current_design().unwrap_or_default()
}

/// Build the structural live preview using the same already-portable sizing
/// factory as the report figure. This is dispatch glue; the renderer remains
/// owned by `alas-report::families::structures`.
fn build_structures_preview(
    airplane: &alas_geom::aircraft::airplane::Airplane,
    config: &AlasConfig,
    design: &DesignVector,
    theme: &str,
) -> Option<Scene> {
    let wing = airplane.wings.first()?;
    let root_airfoil = wing.xsecs.first()?.airfoil.clone();
    let tip_airfoil = wing.xsecs.last()?.airfoil.clone();
    let (spar_fracs, spar_full_span) = config.structures.resolved_spars();
    let geometry = WingStructureGeometry::new(
        design,
        &config.geometry.wing,
        &root_airfoil,
        &tip_airfoil,
        &spar_fracs,
        Some(&spar_full_span),
    )
    .ok()?;
    let sizing = size_wingbox(
        &geometry,
        &config.structures,
        &config.requirements,
        alas_config::materials::get(&config.structures.skin_material).ok()?,
        alas_config::materials::get(&config.structures.spar_web_material).ok()?,
        alas_config::materials::get(&config.structures.spar_cap_material).ok()?,
        alas_config::materials::get(&config.structures.rib_material).ok()?,
    );
    Some(figure_structures_designer_preview(
        &geometry,
        &sizing,
        Some(theme),
    ))
}

/// The live-preview scene for the unified aircraft viewer's current mode.
pub fn build_preview_scene(state: &AppState) -> Option<Scene> {
    let id = match state.preview_tab {
        PreviewTab::Cabin => "cabin_3d",
        PreviewTab::Exterior => state.selected_preview_id.as_str(),
    };
    build_page_preview_with_camera(state, id, Some(state.active_preview_camera().into()))
}

/// A preview figure built live from the current configuration, by id.
///
/// Only the figures that need geometry alone are buildable before a run; the
/// report-fed ones (CG envelope, control surfaces, ...) return `None` here and
/// the page shows a "run to preview" note instead.
pub fn build_page_preview(state: &AppState, id: &str) -> Option<Scene> {
    build_page_preview_with_camera(state, id, None)
}

fn cap_interactive_preview_mesh(config: &mut AlasConfig) {
    config.geometry.wing.n_subdivisions = config.geometry.wing.n_subdivisions.clamp(1, 2);
    config.geometry.empennage.n_subdivisions = config.geometry.empennage.n_subdivisions.clamp(1, 2);
}

/// Build one preview with an explicit camera, without changing the dock's
/// retained camera. Fullscreen views use this so their orbit gestures remain
/// local to the overlay instead of moving the preview behind it.
pub fn build_page_preview_with_camera(
    state: &AppState,
    id: &str,
    camera_override: Option<Camera3D>,
) -> Option<Scene> {
    let mut config = state.typed_config()?;
    let design = preview_design(state);
    // A named cabin preset is normally materialized by the pipeline. The live
    // preview has no pipeline pass, so apply it to this throwaway copy first
    // and keep Custom untouched for direct class-by-class editing.
    if config.requirements.cabin_preset != "Custom" {
        apply_cabin_preset(&mut config, Some(&design)).ok()?;
    }
    // Camera drags rebuild this scene repeatedly. The report and solvers keep
    // the configured discretization; the interactive preview only needs the
    // defining planform panels, so cap its throwaway mesh before building.
    cap_interactive_preview_mesh(&mut config);
    let theme = state.theme.figure_theme_name().to_owned();
    let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&design), true)
        .ok()?;
    let camera = camera_override.or_else(|| {
        let preview_camera = state.preview_cameras.get(id).copied().unwrap_or_default();
        Some(Camera3D {
            elev_deg: preview_camera.pitch_deg,
            azim_deg: preview_camera.yaw_deg,
            zoom: preview_camera.zoom,
        })
    });

    let scene = match id {
        "cabin_3d" => match build_payload_layout(&airplane, &config, 0.0, 0.0) {
            Ok(layout) => alas_report::families::geometry::figure_cabin_payload_3d(
                &layout,
                &airplane,
                &config,
                camera,
                Some(&theme),
            ),
            Err(_) => figure_geometry(&airplane, Some(&theme)),
        },
        // Python's `_preview_geometry` asks for the four-panel three-view and
        // falls back to the planform only when that builder returns `None`.
        // Keep the legacy `3view` alias for callers that used the early Rust
        // preview id, while the registry/menu use the Python `geometry` id.
        "geometry" | "3view" | "threeview" => figure_threeview(&airplane, Some(&theme)),
        "exterior_3d" => figure_exterior_3d(&airplane, camera, Some(&theme)),
        "engine" => {
            alas_report::families::propulsion::figure_engine_designer_preview(&config, Some(&theme))
        }
        "structures" => build_structures_preview(&airplane, &config, &design, &theme)?,
        // These previews need a completed report. Returning None keeps the
        // form honest instead of silently showing an unrelated exterior view.
        "drag" => alas_report::families::aerodynamics::figure_drag_preview(
            &airplane,
            &config,
            &design,
            Some(&theme),
        ),
        "mass_cg" | "landing_gear" | "control_surfaces" => {
            let report = match alas_report::families::mass_balance::quick_preview_report(
                airplane, &config, design,
            ) {
                Ok(report) => report,
                Err(error) => {
                    return Some(localize_scene_for_display(figure_status_message(
                        &preview_unavailable_title(id),
                        &format!("Invalid structural mass-coordinate model: {error}"),
                        false,
                        Some(&theme),
                    )));
                }
            };
            match id {
                "mass_cg" => figure_cg_envelope(&report, &config, Some(&theme)),
                "landing_gear" => figure_landing_gear_planform(&report, &config, Some(&theme)),
                _ => figure_control_surfaces(&report, &config, Some(&theme)),
            }
        }
        _ => return None,
    };
    Some(localize_scene_for_display(scene))
}

/// The title a status figure carries when it stands in for preview `id`.
///
/// One `Err` arm serves the CG envelope, the landing-gear planform and the
/// control-surface layout, and it used to hard-code
/// "Mass and balance preview unavailable" for all three, so the Landing Gear
/// page displayed an error titled for a different artefact. The panel title is
/// already declared once, in [`crate::nav`], so the status figure takes it
/// from there instead of restating it.
pub(crate) fn preview_unavailable_title(id: &str) -> String {
    let figure = crate::nav::all_pages()
        .find(|page| page.preview == Some(id))
        .and_then(|page| page.preview_title)
        .unwrap_or("Preview");
    crate::views::tr_fields(
        "{figure} unavailable",
        &[("figure", crate::views::tr(figure))],
    )
}

/// The results-gallery scene for the current result selection.
pub fn build_result_scene(state: &AppState) -> Option<Scene> {
    let result = state.pipeline_result.as_ref()?;
    let report = selected_result_report(state, result)?;
    let theme = state.theme.figure_theme_name().to_owned();
    match build_result_figure(state, &state.selected_result_id, &result.config, &theme) {
        Some(Some(scene)) => Some(scene),
        Some(None) | None => {
            if let Some(descriptor) = alas_report::find_figure(&state.selected_result_id) {
                Some(localize_scene_for_display(alas_report::families::aerodynamics::figure_status_message(
                    descriptor.title,
                    "Unavailable for this run: the required analysis stage did not produce data.",
                    false,
                    Some(&theme),
                )))
            } else {
                // Unknown ids can arise from an older saved UI state. Keep a
                // useful chart in that case, while registered ids stay honest.
                Some(localize_scene_for_display(figure_polar_comparison(
                    result.baseline_analysis.as_ref().unwrap_or(report),
                    report,
                    ("Baseline", "Optimized"),
                    None,
                    Some(&theme),
                )))
            }
        }
    }
}

/// Build one result figure by id, or `None` when this run has no data for it.
pub fn build_result_figure(
    state: &AppState,
    id: &str,
    config: &AlasConfig,
    theme: &str,
) -> Option<Option<Scene>> {
    build_result_figure_with_camera(state, id, config, theme, None)
}
