// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


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
    // failed; the Python result factory only requires route data here.
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
            let mlw_limit_kg = config.landing_mass_limit_kg(config.requirements.mtow_kg);
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
        "mission_velocities" => {
            figure_mission_velocities(
                result
                    .mission_result
                    .as_ref()
                    .filter(|mission| mission.figure_data_ready())?,
                Some(theme),
            )
        }
        "mission_flight_path" => {
            figure_mission_flight_path(
                result
                    .mission_result
                    .as_ref()
                    .filter(|mission| mission.figure_data_ready())?,
                Some(theme),
            )
        }
        "mission_aero_coefficients" => {
            figure_mission_aero_coefficients(
                result
                    .mission_result
                    .as_ref()
                    .filter(|mission| mission.figure_data_ready())?,
                Some(theme),
            )
        }
        "mission_aero_forces" => {
            figure_mission_aero_forces(
                result
                    .mission_result
                    .as_ref()
                    .filter(|mission| mission.figure_data_ready())?,
                Some(theme),
            )
        }
        "mission_drag_components" => {
            figure_mission_drag_components(
                result
                    .mission_result
                    .as_ref()
                    .filter(|mission| mission.figure_data_ready())?,
                Some(theme),
            )
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
                "5 waypoints | 5525 km | flown altitude and mass profile | orthographic globe; drag to orbit; wheel zoom in fullscreen"
            ),
            "5 puntos de ruta | 5525 km | perfil de altitud y masa volado | globo ortogr\u{00e1}fico; arrastre para orbitar; use la rueda para ampliar en pantalla completa"
        );
    }

    #[test]
    fn route_mass_scale_keeps_english_when_english_is_selected() {
        alas_i18n::es::install();
        alas_i18n::set_language(Some("en"));
        assert_eq!(localize_scene_text("Total Mass (t)"), "Total Mass (t)");
    }
}
