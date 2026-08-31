// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Figure-scene construction for the live preview, the per-page side previews,
//! and the results gallery.
//!
//! Kept out of [`crate::state`] so that module stays the data hub: these
//! functions read a whole [`AppState`] and return the [`Scene`] a viewport then
//! draws. Everything here degrades to `None` rather than panicking -- a figure
//! that needs a completed run, or a geometry that will not build mid-edit, is a
//! blank slot, not a crash.

use alas_config::{design_variables::DesignVector, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_geom::wing_structure::WingStructureGeometry;
use alas_payload::{apply_cabin_preset, build::build_payload_layout};
use alas_perf::performance::build_vn_diagram;
use alas_report::families::aerodynamics::{
    figure_aero_panel, figure_airfoil_reynolds, figure_drag_breakdown, figure_model_comparison,
    figure_mses_mach_contours, figure_mses_pressure_distribution,
    figure_optimized_aircraft_comparison, figure_polar_comparison, figure_span_loading,
    figure_status_message, figure_vlm_flow,
};
use alas_report::families::geometry::{
    figure_airfoil_evolution, figure_design_evolution, figure_exterior_3d, figure_geometry,
    figure_planform_comparison, figure_threeview, figure_wireframe_empennage,
    figure_wireframe_fuselage, figure_wireframe_wing,
};
use alas_report::families::mass_balance::{
    figure_cg_envelope, figure_landing_gear_planform, figure_mass_breakdown,
    figure_mass_distribution,
};
use alas_report::families::mass_balance_layout::{
    figure_cabin_cross_section, figure_fuel_volume_check,
};
use alas_report::families::mission::{
    figure_mission_aero_coefficients, figure_mission_aero_forces, figure_mission_drag_components,
    figure_mission_flight_path, figure_mission_profile, figure_mission_route_2d,
    figure_mission_route_3d, figure_mission_velocities,
};
use alas_report::families::optimization::figure_airfoil_comparison;
use alas_report::families::optimization::figure_optimization_history;
use alas_report::families::performance::{
    figure_lto_arrival, figure_lto_departure, figure_lto_for_airport, figure_matching_chart,
    figure_payload_range, figure_vn_diagram,
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
#[path = "scene_solver.rs"]
mod scene_solver;
use scene_solver::{
    selected_avl_result, selected_optimization_result, selected_result_report, solutions_avl_result,
};

#[path = "dispatch_ids.rs"]
mod dispatch_ids;
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
        if let SceneElement::Text { text, .. } = element {
            *text = localize_scene_text(text);
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
                        "Mass and balance preview unavailable",
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

/// Build one result figure with an optional projection override for a 3-D scene.
///
/// The ordinary report and export paths pass no override. The GUI uses this
/// only for result figures whose registry contract explicitly supports orbit.
pub fn build_result_figure_with_camera(
    state: &AppState,
    id: &str,
    config: &AlasConfig,
    theme: &str,
    camera: Option<Camera3D>,
) -> Option<Option<Scene>> {
    let result = state.pipeline_result.as_ref()?;
    // These figures depend only on configuration and remain useful before a
    // full aerodynamic report exists.
    let config_scene = match id {
        "propulsion_cycle_summary" => Some(figure_propulsion_cycle_summary(config, Some(theme))),
        "propulsion_carpet_plot" => Some(figure_propulsion_carpet_plot(config, Some(theme))),
        "propulsion_efficiency_decomposition" => Some(figure_propulsion_efficiency_decomposition(
            config,
            Some(theme),
        )),
        "propulsion_bpr_sensitivity" => {
            Some(figure_propulsion_bpr_sensitivity(config, Some(theme)))
        }
        "propulsion_altitude_sweep" => Some(figure_propulsion_altitude_sweep(config, Some(theme))),
        _ => None,
    };
    if let Some(scene) = config_scene {
        return Some(Some(localize_scene_for_display(scene)));
    }
    // A route is useful even when the aerodynamic report was disabled or
    // failed; the Python result factory only requires route data here.
    if id == "mission_route_2d" || id == "mission_route_3d" {
        let route = result.route.as_ref()?;
        let profiles = result
            .mission_result
            .as_ref()
            .map(|mission| alas_report::route_geometry::sync_mass_to_route(route, mission))
            .filter(|(mass, altitude)| !mass.is_empty() && !altitude.is_empty());
        let (mass, altitude) = profiles
            .as_ref()
            .map(|(mass, altitude)| (Some(mass.as_slice()), Some(altitude.as_slice())))
            .unwrap_or((None, None));
        return Some(Some(localize_scene_for_display(
            if id == "mission_route_3d" {
                figure_mission_route_3d(route, mass, altitude, camera, Some(theme))
            } else {
                figure_mission_route_2d(route, mass, altitude, Some(theme))
            },
        )));
    }
    let report = selected_result_report(state, result)?;
    let baseline = result.baseline_analysis.as_ref();
    let scene = match id {
        "matching_chart" => figure_matching_chart(report, config, Some(theme)),
        "optimization_history" => {
            let history = selected_optimization_result(state, result)?.history.clone();
            figure_optimization_history(&history, Some(theme))
        }
        "design_evolution" => {
            let history = &selected_optimization_result(state, result)?.history;
            let builder = AircraftBuilder::new(Some(config.geometry.clone()));
            figure_design_evolution(history, &builder, 12, Some(theme)).unwrap_or_else(|| {
                figure_status_message(
                    "Design evolution unavailable",
                    "This run produced no physically valid candidate planforms to plot.",
                    false,
                    Some(theme),
                )
            })
        }
        "airfoil_comparison" => {
            let baseline = baseline?;
            let initial = baseline
                .airplane
                .wings
                .first()?
                .xsecs
                .first()?
                .airfoil
                .coordinates
                .as_slice();
            let optimized = report
                .airplane
                .wings
                .first()?
                .xsecs
                .first()?
                .airfoil
                .coordinates
                .as_slice();
            figure_airfoil_comparison(initial, optimized, Some(theme))
        }
        "airfoil_evolution" => figure_airfoil_evolution(&report.airplane, Some(theme)),
        "polar_comparison" => {
            let baseline = baseline?;
            figure_polar_comparison(
                baseline,
                report,
                ("Baseline", "Optimized"),
                None,
                Some(theme),
            )
        }
        "planform_comparison" => {
            let baseline = baseline?;
            figure_planform_comparison(
                &baseline.airplane,
                &report.airplane,
                ("Baseline", "Optimized"),
                Some(theme),
            )
        }
        "wireframe_wing" => figure_wireframe_wing(&report.airplane, Some(theme)),
        "wireframe_fuselage" => figure_wireframe_fuselage(&report.airplane, Some(theme)),
        "wireframe_empennage" => figure_wireframe_empennage(&report.airplane, Some(theme)),
        "threeview" | "asb_threeview" => figure_threeview(&report.airplane, Some(theme)),
        "aero_panel" => figure_aero_panel(report, Some(theme)),
        "span_loading" => figure_span_loading(report, Some(theme)),
        "vlm_flow" => figure_vlm_flow(report, Some(theme)),
        "drag_breakdown" => figure_drag_breakdown(report, Some(theme)),
        "airfoil_reynolds" => {
            let airfoil = report
                .airplane
                .wings
                .first()?
                .xsecs
                .first()?
                .airfoil
                .clone();
            figure_airfoil_reynolds(&airfoil, Some(theme))
        }
        "mses_pressure" => {
            figure_mses_pressure_distribution(result.mses_pressure.as_ref()?, Some(theme))
        }
        "mses_mach_contours" => {
            let airfoil = report
                .airplane
                .wings
                .first()?
                .xsecs
                .first()?
                .airfoil
                .clone();
            figure_mses_mach_contours(result.mses_pressure.as_ref()?, Some(&airfoil), Some(theme))
        }
        "mass_breakdown" => figure_mass_breakdown(report, Some(theme)),
        "fuel_volume_check" => figure_fuel_volume_check(report, config, Some(theme)),
        "mass_distribution" => figure_mass_distribution(report, Some(theme)),
        "dynamic_modes" => figure_dynamic_modes(report, config, Some(theme)),
        "cg_envelope" => figure_cg_envelope(report, config, Some(theme)),
        "landing_gear_planform" => figure_landing_gear_planform(report, config, Some(theme)),
        "control_surfaces" => figure_control_surfaces(report, config, Some(theme)),
        "stability_side_view" => figure_stability_side_view(report, Some(theme)),
        "cabin_payload" => {
            let layout = report.payload_layout.as_ref()?;
            alas_report::families::geometry::figure_cabin_payload(
                layout,
                &report.airplane,
                config,
                Some(theme),
            )
        }
        "cabin_section" => {
            let layout = report.payload_layout.as_ref()?;
            figure_cabin_cross_section(layout, &report.airplane, config, Some(theme))
        }
        "payload_range" => figure_payload_range(report, config, Some(theme)),
        "lto_departure" => result
            .route
            .as_ref()
            .and_then(|route| route.origin_airport.as_ref())
            .map(|airport| {
                figure_lto_for_airport(report, config, airport, "Departure", Some(theme))
            })
            .unwrap_or_else(|| figure_lto_departure(report, config, Some(theme))),
        "lto_arrival" => result
            .route
            .as_ref()
            .and_then(|route| route.dest_airport.as_ref())
            .map(|airport| figure_lto_for_airport(report, config, airport, "Arrival", Some(theme)))
            .unwrap_or_else(|| figure_lto_arrival(report, config, Some(theme))),
        "mission_profile" => figure_mission_profile(result.mission_result.as_ref()?, Some(theme)),
        "mission_velocities" => {
            figure_mission_velocities(result.mission_result.as_ref()?, Some(theme))
        }
        "mission_flight_path" => {
            figure_mission_flight_path(result.mission_result.as_ref()?, Some(theme))
        }
        "mission_aero_coefficients" => {
            figure_mission_aero_coefficients(result.mission_result.as_ref()?, Some(theme))
        }
        "mission_aero_forces" => {
            figure_mission_aero_forces(result.mission_result.as_ref()?, Some(theme))
        }
        "mission_drag_components" => {
            figure_mission_drag_components(result.mission_result.as_ref()?, Some(theme))
        }
        "vn_diagram" => {
            let vn = build_vn_diagram(
                report.airplane.s_ref,
                &config.requirements,
                &config.performance,
                config.requirements.cruise_altitude_m,
            );
            figure_vn_diagram(&vn, Some(theme))
        }
        "structures_sizing" => {
            figure_structures_sizing(result.structural_result.as_ref(), Some(theme))
        }
        "structures_loads" => {
            figure_structures_loads(result.structural_result.as_ref(), Some(theme))
        }
        "structures_stress" => {
            figure_structures_stress(result.structural_result.as_ref(), Some(theme))
        }
        "structures_modes" => {
            figure_structures_modes(result.structural_result.as_ref(), Some(theme))
        }
        "structures_vibration" => {
            figure_structures_vibration(result.structural_result.as_ref(), Some(theme))
        }
        "structures_patran" => {
            figure_structures_patran(result.structural_result.as_ref(), Some(theme))
        }
        "model_comparison" => {
            let dual = result.solver_optimizations.as_ref().and_then(|solutions| {
                Some((
                    solutions.vlm.report.as_ref()?,
                    solutions.avl.report.as_ref()?,
                ))
            });
            if let Some((vlm_report, avl_report)) = dual {
                figure_optimized_aircraft_comparison(
                    vlm_report,
                    avl_report,
                    result.avl_result.as_ref(),
                    solutions_avl_result(result),
                    Some(theme),
                )
            } else {
                figure_model_comparison(
                    report,
                    result.mission_result.as_ref(),
                    result.mses_result.as_ref(),
                    result.vspaero_result.as_ref(),
                    selected_avl_result(state, result),
                    Some(theme),
                )
            }
        }
        _ => return Some(None),
    };
    Some(Some(localize_scene_for_display(scene)))
}

/// Build one airfoil-screening figure from the result owned by the screening
/// page. A missing sweep or a stage with no usable candidates is an ordinary
/// unavailable slot, matching the Python `SWEEP_FIGURES` contract.
pub fn build_screening_figure(state: &AppState, id: &str, theme: &str) -> Option<Scene> {
    let result = state.screening.result.as_ref()?;
    match id {
        "trade_map" => fig_trade_map(result, Some(theme)),
        "rerank_2d_3d" => fig_rerank_2d_3d(result, Some(theme)),
        "ranking_bars" => fig_ranking_bars(result, Some(theme)),
        "mses_verification" => fig_mses_verification(result, Some(theme)),
        "section_shapes" => fig_section_shapes(result, Some(theme)),
        _ => None,
    }
    .map(localize_scene_for_display)
}

#[cfg(test)]
mod tests {
    use super::{cap_interactive_preview_mesh, localize_scene_text};
    use alas_config::AlasConfig;

    #[test]
    fn interactive_preview_caps_mesh_without_changing_solver_configuration() {
        let original = AlasConfig::default();
        let mut preview = original.clone();

        cap_interactive_preview_mesh(&mut preview);

        assert_eq!(preview.geometry.wing.n_subdivisions, 2);
        assert_eq!(preview.geometry.empennage.n_subdivisions, 2);
        assert_eq!(original.geometry.wing.n_subdivisions, 8);
        assert_eq!(original.geometry.empennage.n_subdivisions, 6);
    }

    #[test]
    fn layout_counts_keep_values_while_translating_their_units() {
        alas_i18n::es::install();
        alas_i18n::set_language(Some("es"));
        assert_eq!(
            localize_scene_text("349 seats, 65.0 t"),
            "349 asientos, 65.0 t"
        );
        assert_eq!(localize_scene_text("12 ULD, 30.5 t"), "12 ULD, 30.5 t");
    }

    #[test]
    fn dynamic_mass_and_mode_labels_keep_values_while_translating_names() {
        alas_i18n::es::install();
        alas_i18n::set_language(Some("es"));
        assert_eq!(
            localize_scene_text("Wing\n41.9 t  (12%)"),
            "Ala\n41.9 t  (12%)"
        );
        assert_eq!(
            localize_scene_text("Physical CG  x=35.7 m"),
            "CG f\u{00ed}sico  x=35.7 m"
        );
        assert_eq!(localize_scene_text("Mode 3: 15.92 Hz"), "Modo 3: 15.92 Hz");
    }

    #[test]
    fn report_figure_labels_translate_dynamic_prefixes_without_changing_values() {
        alas_i18n::es::install();
        alas_i18n::set_language(Some("es"));
        assert_eq!(
            localize_scene_text("Wingbox planform  (39 ribs)"),
            "Planta del caj\u{f3}n alar  (39 ribs)"
        );
        assert_eq!(
            localize_scene_text("FEM vs Torenbeek wing mass  (delta = -3%)"),
            "Masa alar: FEM frente a Torenbeek  (delta = -3%)"
        );
        assert_eq!(
            localize_scene_text("Per-engine thrust, this cruise pt :   242.4 kN"),
            "Empuje por motor, en este punto de crucero :   242.4 kN"
        );
        assert_eq!(
            localize_scene_text("Torenbeek\nestimate"),
            "Torenbeek\nestimaci\u{00f3}n"
        );
    }

    #[test]
    fn report_annotations_translate_generated_prefixes_without_changing_measurements() {
        alas_i18n::es::install();
        alas_i18n::set_language(Some("es"));
        assert_eq!(
            localize_scene_text("Governing load case: pull-up"),
            "Caso de carga determinante: pull-up"
        );
        assert_eq!(
            localize_scene_text("Semi-wing mass: 20,291 kg"),
            "Masa de semiala: 20,291 kg"
        );
        assert_eq!(
            localize_scene_text("  Spar caps: 5,470 kg"),
            "  Tapas de larguero: 5,470 kg"
        );
        assert_eq!(
            localize_scene_text("Landing Gear Planform -- NLG: 2xHeavy"),
            "Planta del tren de aterrizaje: NLG: 2xHeavy"
        );
    }

    #[test]
    fn cg_steering_constraint_uses_control_in_spanish() {
        alas_i18n::es::install();
        alas_i18n::set_language(Some("es"));
        assert_eq!(
            localize_scene_text("Min nose load (steering)"),
            "Carga m\u{00ed}nima en morro (control)"
        );
        assert_eq!(
            localize_scene_text("Min nose load\n(steering)"),
            "Carga m\u{00ed}nima en morro\n(control)"
        );
    }

    #[test]
    fn route_footers_preserve_measurements_while_translating_the_prose() {
        alas_i18n::es::install();
        alas_i18n::set_language(Some("es"));
        assert_eq!(localize_scene_text("Origin"), "Origen");
        assert_eq!(localize_scene_text("Destination"), "Destino");
        assert_eq!(localize_scene_text("Total Mass (t)"), "Masa total (t)");
        assert_eq!(
            localize_scene_text(
                "5 waypoints | 5525 km | flown profile: 254000 kg to 215000 kg, 0 to 0 m"
            ),
            "5 puntos de ruta | 5525 km | perfil volado: de 254000 kg a 215000 kg, de 0 a 0 m"
        );
        assert_eq!(
            localize_scene_text(
                "5 waypoints | 5525 km | flown altitude and mass profile | orthographic globe; drag to orbit; scroll to zoom"
            ),
            "5 puntos de ruta | 5525 km | perfil de altitud y masa volado | globo ortogr\u{00e1}fico; arrastre para orbitar; desplaza para ampliar"
        );
    }

    #[test]
    fn route_mass_scale_keeps_english_when_english_is_selected() {
        alas_i18n::es::install();
        alas_i18n::set_language(Some("en"));
        assert_eq!(localize_scene_text("Total Mass (t)"), "Total Mass (t)");
    }
}
