// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Results-gallery and screening figure scenes.

use alas_config::AlasConfig;
use alas_geom::builder::AircraftBuilder;
use alas_report::families::aerodynamics::{
    figure_aero_panel, figure_airfoil_reynolds, figure_drag_breakdown, figure_model_comparison,
    figure_mses_convergence, figure_mses_cp_contours, figure_mses_mach_contours,
    figure_mses_pressure_distribution, figure_optimized_aircraft_comparison,
    figure_polar_comparison, figure_span_loading, figure_status_message, figure_vlm_flow,
    figure_vspaero_load_distribution, figure_vspaero_polar, figure_vspaero_wake_convergence,
};
use alas_report::families::geometry::{
    figure_airfoil_evolution, figure_design_evolution, figure_openvsp_cad_preview,
    figure_planform_comparison, figure_threeview, figure_wireframe_empennage,
    figure_wireframe_fuselage, figure_wireframe_wing,
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
use alas_report::families::structures::{figure_structures_loads, figure_structures_sizing};
use alas_report::families::structures_dynamics::figure_structures_stress;
use alas_report::families::structures_dynamics::{
    figure_structures_modes, figure_structures_patran, figure_structures_vibration,
};
use alas_report::scene::{Camera3D, Scene};

use super::localize_scene_for_display;
use super::solver::{
    selected_avl_result, selected_optimization_result, selected_result_report, solutions_avl_result,
};
use crate::state::AppState;

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
                // Unknown ids fall back to a useful chart, while registered ids stay
                // honest.
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
    if id == "openvsp_cad_preview" {
        return Some(Some(localize_scene_for_display(
            figure_openvsp_cad_preview(result.openvsp_export.as_ref(), Some(theme)),
        )));
    }
    // A route is useful even when the aerodynamic report was disabled or
    // failed, so only route data is required here.
    if id == "mission_route_2d" || id == "mission_route_3d" {
        let route = result.route.as_ref()?;
        let profiles = result
            .mission_result
            .as_ref()
            .filter(|mission| mission.figure_data_ready())
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
        "mses_cp_contours" => {
            let airfoil = report
                .airplane
                .wings
                .first()?
                .xsecs
                .first()?
                .airfoil
                .clone();
            figure_mses_cp_contours(result.mses_pressure.as_ref()?, Some(&airfoil), Some(theme))
        }
        "mses_convergence" => figure_mses_convergence(result.mses_result.as_ref()?, Some(theme)),
        "vspaero_polar" => figure_vspaero_polar(result.vspaero_result.as_ref()?, Some(theme)),
        "vspaero_wake_convergence" => {
            figure_vspaero_wake_convergence(result.vspaero_result.as_ref()?, Some(theme))
        }
        "vspaero_load_distribution" => {
            figure_vspaero_load_distribution(result.vspaero_result.as_ref()?, Some(theme))
        }
        "mass_breakdown" => figure_mass_breakdown(report, Some(theme)),
        "fuel_volume_check" => {
            figure_fuel_volume_check_for_loading(&result.feasibility.fuel_loading, Some(theme))
        }
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
        "lto_departure" | "lto_arrival" => {
            let departure = id == "lto_departure";
            let airport = result
                .route
                .as_ref()
                .and_then(|route| {
                    if departure {
                        route.origin_airport.as_ref()
                    } else {
                        route.dest_airport.as_ref()
                    }
                })
                .or_else(|| {
                    alas_config::airports::get(if departure {
                        &config.departure_airport
                    } else {
                        &config.arrival_airport
                    })
                    .ok()
                })?;
            let fuel = &result.feasibility.fuel_loading;
            let takeoff_mass_kg = fuel.analyzed_takeoff_mass_kg;
            let mlw_limit_kg = config.landing_mass_limit_kg(
                report.analysis_takeoff_mass_kg(config.requirements.mtow_kg),
            );
            let landing_mass_kg = fuel
                .analyzed_landing_mass_kg
                .unwrap_or(mlw_limit_kg.min(takeoff_mass_kg));
            figure_lto_for_airport_at_masses(
                report,
                config,
                airport,
                if departure { "Departure" } else { "Arrival" },
                takeoff_mass_kg,
                landing_mass_kg,
                Some(theme),
            )
        }
        "mission_profile" => figure_mission_profile(
            result
                .mission_result
                .as_ref()
                .filter(|mission| mission.figure_data_ready())?,
            Some(theme),
        ),
        "mission_velocities" => figure_mission_velocities(
            result
                .mission_result
                .as_ref()
                .filter(|mission| mission.figure_data_ready())?,
            Some(theme),
        ),
        "mission_flight_path" => figure_mission_flight_path(
            result
                .mission_result
                .as_ref()
                .filter(|mission| mission.figure_data_ready())?,
            Some(theme),
        ),
        "mission_aero_coefficients" => figure_mission_aero_coefficients(
            result
                .mission_result
                .as_ref()
                .filter(|mission| mission.figure_data_ready())?,
            Some(theme),
        ),
        "mission_aero_forces" => figure_mission_aero_forces(
            result
                .mission_result
                .as_ref()
                .filter(|mission| mission.figure_data_ready())?,
            Some(theme),
        ),
        "mission_drag_components" => figure_mission_drag_components(
            result
                .mission_result
                .as_ref()
                .filter(|mission| mission.figure_data_ready())?,
            Some(theme),
        ),
        "vn_diagram" => {
            let vn = alas_pipeline::design_vn_diagram(config, report);
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
                    result
                        .mission_result
                        .as_ref()
                        .filter(|mission| mission.figure_data_ready()),
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
/// unavailable slot, as the screening figure registry expects.
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
