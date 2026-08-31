// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from the legacy mission-builder boundary.
// Reference: alas @ rust-port-baseline.

//! Native mission orchestration for the design pipeline.
//!
//! The old desktop path stopped after route planning and left
//! `PipelineResult::mission_result` permanently empty. This module is the
//! dissolved external runner boundary: it builds the same segment schedule and
//! the same four analysis inputs from the already-built Rust airplane, then
//! evaluates [`alas_mission::Mission`] in-process.

use std::f64::consts::PI;

use alas_aero::drag_buildup::{DragSettings, FuselageParams, NacelleParams, WingParams};
use alas_aero::lift_surrogate::{LiftSurrogate, TrainingGrid};
use alas_aero::vorlax::{VlmGeometry, VlmSettings, VlmWing};
use alas_config::airports::Airport;
use alas_config::{ActiveEngineModel, AlasConfig};
#[cfg(test)]
use alas_mission::segments::SegmentKind;
use alas_mission::segments::{LegacyTurbofanCompatibility, MissionAnalyses};
use alas_mission::{build_mission_request, Mission, MissionResult};
use alas_prop::empirical_turbofan::{EmpiricalTurbofanDeck, EmpiricalTurbofanModel};
use alas_prop::mission_turbofan::{
    size_turbofan, size_turbofan_to_static_rating, PartPowerModel, TurbofanInputs,
    VehicleBuilderParams,
};
use alas_prop::system::{
    LegacyTurbofanModel, ModelIdentity, ModelProvenance, PropulsionInstallation,
    PropulsionOrchestrator,
};
use alas_prop::turboprop::{Atr72TurbopropSystem, Pw127m568fModel};

use crate::feasibility::plan_fuel_loading;
use crate::full_analysis::AnalysisReport;

mod guidance;
mod product;
mod schedule;

use guidance::{adapt_failed_climb, adapt_failed_descent};
#[cfg(test)]
use schedule::schedule_horizontal_distance;
use schedule::{build_schedule, close_schedule_distance};

const METRES_PER_SECOND_TO_FEET_PER_MINUTE: f64 = 3.28084 * 60.0;

/// Run the native mission for the selected report and route.
pub(crate) fn evaluate(
    config: &AlasConfig,
    report: &AnalysisReport,
    origin: &Airport,
    destination: &Airport,
    route_distance_m: f64,
) -> Result<MissionResult, String> {
    if config.mission.model == alas_config::MissionModel::TotalEnergy {
        let request = build_mission_request(config, origin, destination, route_distance_m);
        let analyses = build_analyses(config, report)?;
        return evaluate_product_with_trip_fuel_closure(config, &request, analyses);
    }
    let request = build_mission_request(config, origin, destination, route_distance_m);
    let mut schedule = build_schedule(&request)?;
    let analyses = build_analyses(config, report)?;
    const MAX_GUIDANCE_REVISIONS: usize = 12;
    for revision in 0..=MAX_GUIDANCE_REVISIONS {
        let result = Mission {
            schedule: schedule.clone(),
        }
        .evaluate(&analyses)
        .map_err(|error| format!("native mission failed: {error}"))?;
        if result.completed_summary().is_some() || revision == MAX_GUIDANCE_REVISIONS {
            return Ok(result);
        }

        let Some(index) = result.segments.len().checked_sub(1) else {
            return Ok(result);
        };
        let segment_tag = schedule[index].tag.clone();
        let solution = &result.solutions[index];
        let adapted = if solution.throttle_limited {
            adapt_failed_climb(&mut schedule, index, &result.segments[index]).map(|change| {
                tracing::info!(
                    segment = %segment_tag,
                    old_rate_m_s = change.old_rate_m_s,
                    new_rate_m_s = change.new_rate_m_s,
                    old_end_altitude_m = change.old_end_altitude_m,
                    new_end_altitude_m = change.new_end_altitude_m,
                    "replanned throttle-limited climb from available excess power"
                );
            })
        } else if !solution.converged {
            adapt_failed_descent(&mut schedule, index, &result.segments[index]).map(|change| {
                tracing::info!(
                    segment = %segment_tag,
                    old_rate_m_s = change.old_rate_m_s,
                    new_rate_m_s = change.new_rate_m_s,
                    "replanned descent from idle-thrust excess drag"
                );
            })
        } else {
            None
        };
        if adapted.is_none() {
            return Ok(result);
        }
        close_schedule_distance(&mut schedule, &request)?;
    }
    unreachable!("bounded guidance revision loop always returns")
}

/// Close the conceptual trip-only takeoff fuel against the mission burn.
///
/// The former product path always loaded the maximum fuel admitted by MTOW,
/// making short sectors fly at an artificial near-MTOW state. Search for the
/// first propagatable carried-fuel state, then solve
/// `m_TO = m_ZF + 1.02 m_trip`. The explicit two-percent arrival remainder
/// prevents integration roundoff from crossing the zero-fuel-mass floor; it is
/// carried and reported as destination fuel, not treated as a hidden tolerance.
/// Policy reserves remain outside this conceptual closure and are not invented.
fn evaluate_product_with_trip_fuel_closure(
    config: &AlasConfig,
    request: &alas_mission::MissionRequest,
    mut analyses: MissionAnalyses,
) -> Result<MissionResult, String> {
    let Some(zero_fuel_mass_kg) = analyses.minimum_mass_kg else {
        return product::evaluate(config, request, &analyses);
    };
    let maximum_takeoff_mass_kg = analyses.takeoff_mass_kg;
    let maximum_carried_fuel_kg = maximum_takeoff_mass_kg - zero_fuel_mass_kg;
    if !zero_fuel_mass_kg.is_finite()
        || !maximum_carried_fuel_kg.is_finite()
        || maximum_carried_fuel_kg <= 0.0
    {
        return product::evaluate(config, request, &analyses);
    }

    const ARRIVAL_REMAINDER_FRACTION: f64 = 0.02;
    const MASS_RESIDUAL_TOLERANCE_KG: f64 = 5.0;
    let mut upper_solution = None;
    let mut last_error = None;
    // Start from the established maximum load: it is the cheapest way to
    // prove a positive residual on long sectors and avoids propagating a
    // ladder of predictably under-fueled trajectories.
    for fraction in [1.0, 0.85, 0.70, 0.50, 0.35, 0.20, 0.10] {
        analyses.takeoff_mass_kg = zero_fuel_mass_kg + fraction * maximum_carried_fuel_kg;
        match product::evaluate(config, request, &analyses) {
            Ok(result) => {
                let summary = result
                    .completed_summary()
                    .ok_or_else(|| "trip-fuel closure received an incomplete mission".to_owned())?;
                let residual_kg = analyses.takeoff_mass_kg
                    - zero_fuel_mass_kg
                    - (1.0 + ARRIVAL_REMAINDER_FRACTION) * summary.trip_fuel_kg;
                if residual_kg >= 0.0 {
                    upper_solution = Some((analyses.takeoff_mass_kg, result, residual_kg));
                    break;
                }
            }
            Err(error) => last_error = Some(error),
        }
    }
    let Some((mut upper_mass_kg, mut upper_result, mut upper_residual_kg)) = upper_solution else {
        return Err(format!(
            "trip-fuel closure found no load with the required 2% arrival remainder between ZFW {zero_fuel_mass_kg:.1} kg and analyzed maximum {maximum_takeoff_mass_kg:.1} kg: {}",
            last_error.unwrap_or_else(|| "no propagated diagnostic".to_owned())
        ));
    };

    if upper_residual_kg <= MASS_RESIDUAL_TOLERANCE_KG {
        return Ok(upper_result);
    }
    let mut lower_mass_kg = zero_fuel_mass_kg;
    for _ in 0..40 {
        // A fixed-point estimate from the feasible upper trajectory normally
        // lands close to the root. Fall back to bisection whenever that
        // estimate is already on a known bound.
        let upper_summary = upper_result
            .completed_summary()
            .ok_or_else(|| "trip-fuel closure lost its complete upper bracket".to_owned())?;
        let fixed_point_mass_kg =
            zero_fuel_mass_kg + (1.0 + ARRIVAL_REMAINDER_FRACTION) * upper_summary.trip_fuel_kg;
        let candidate_mass_kg = if fixed_point_mass_kg > lower_mass_kg + MASS_RESIDUAL_TOLERANCE_KG
            && fixed_point_mass_kg < upper_mass_kg - MASS_RESIDUAL_TOLERANCE_KG
        {
            fixed_point_mass_kg
        } else {
            0.5 * (lower_mass_kg + upper_mass_kg)
        };
        analyses.takeoff_mass_kg = candidate_mass_kg;
        match product::evaluate(config, request, &analyses) {
            Ok(candidate_result) => {
                let summary = candidate_result
                    .completed_summary()
                    .ok_or_else(|| "trip-fuel closure received an incomplete mission".to_owned())?;
                let residual_kg = candidate_mass_kg
                    - zero_fuel_mass_kg
                    - (1.0 + ARRIVAL_REMAINDER_FRACTION) * summary.trip_fuel_kg;
                if residual_kg.abs() <= MASS_RESIDUAL_TOLERANCE_KG {
                    return Ok(candidate_result);
                }
                if residual_kg >= 0.0 {
                    upper_mass_kg = candidate_mass_kg;
                    upper_result = candidate_result;
                    upper_residual_kg = residual_kg;
                } else {
                    lower_mass_kg = candidate_mass_kg;
                }
            }
            Err(_) => lower_mass_kg = candidate_mass_kg,
        }
        if upper_mass_kg - lower_mass_kg <= MASS_RESIDUAL_TOLERANCE_KG {
            return Ok(upper_result);
        }
    }
    Err(format!(
        "trip-fuel closure did not converge after 40 iterations (upper residual {upper_residual_kg:.1} kg)"
    ))
}

// The compatibility variant is only used by the in-crate W6.4 evidence tests.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MissionReferenceMode {
    /// Current product semantics: projected XY references and installed
    /// engine rating are the authoritative mission inputs.
    Product,
    /// Explicit frozen-evidence semantics: unfolded reference geometry,
    /// direct-sum compressibility, and cruise-required engine sizing.
    ReferenceCompatibility,
}

fn build_analyses(config: &AlasConfig, report: &AnalysisReport) -> Result<MissionAnalyses, String> {
    build_analyses_with_mode(config, report, MissionReferenceMode::Product)
}

/// Build the mission input boundary used by frozen W6.4/SUAVE evidence.
///
/// This is deliberately separate from [`build_analyses`]. The historical
/// vehicle fixture sizes its turbofan from cruise-required thrust and uses the
/// unfolded compatibility area; allowing those values to leak into the
/// product path would reintroduce throttle commands above one.
#[allow(dead_code)] // Called from the cfg(test) W6.4 module, not product builds.
fn build_analyses_reference_compatibility(
    config: &AlasConfig,
    report: &AnalysisReport,
) -> Result<MissionAnalyses, String> {
    build_analyses_with_mode(config, report, MissionReferenceMode::ReferenceCompatibility)
}

fn build_analyses_with_mode(
    config: &AlasConfig,
    report: &AnalysisReport,
    reference_mode: MissionReferenceMode,
) -> Result<MissionAnalyses, String> {
    let vlm_geometry = vlm_geometry(config, report, reference_mode)?;
    let surrogate = LiftSurrogate::train(
        &vlm_geometry,
        &VlmSettings::default(),
        &TrainingGrid::default(),
    )
    .map_err(|error| format!("mission lift surrogate failed: {error}"))?;

    let engine = &config.geometry.engine;
    let n_engines = engine.spanwise_positions_m.len();
    if n_engines == 0 {
        return Err("mission aircraft has no engines".to_owned());
    }
    let wings = mission_wings(config, report, reference_mode)?;
    let fuselage = mission_fuselage(config, report.design.fuselage_length_m);
    let nacelles = mission_nacelles(engine, n_engines);
    let installation = PropulsionInstallation {
        unit_positions_m: engine
            .spanwise_positions_m
            .iter()
            .map(|&y_m| [0.0, y_m, engine.z_m])
            .collect(),
        thrust_axes_body: vec![[1.0, 0.0, 0.0]; n_engines],
        nacelle_wetted_area_m2: None,
        frontal_area_m2: None,
    };
    let active_model = engine
        .active_model()
        .map_err(|error| format!("mission engine binding failed: {error}"))?;
    let (propulsion, legacy_turbofan) = match active_model {
        ActiveEngineModel::Turbofan(payload) => {
            // The legacy turbofan adapter returns one scalar installed force
            // and no moment. Represent it as the statically equivalent net
            // thrust line through the mission reference point; retaining the
            // physical nacelle z offset would claim an unmodelled pitch
            // moment and is correctly rejected by the propulsion boundary.
            let legacy_installation = PropulsionInstallation {
                unit_positions_m: engine
                    .spanwise_positions_m
                    .iter()
                    .map(|&y_m| [0.0, y_m, 0.0])
                    .collect(),
                ..installation.clone()
            };
            let design_thrust_total_n = match reference_mode {
                MissionReferenceMode::Product => {
                    payload.rated_thrust_kn * n_engines as f64 * 1_000.0
                }
                MissionReferenceMode::ReferenceCompatibility => {
                    historical_cruise_required_thrust_total_n(config, report)?
                }
            };
            let inputs = TurbofanInputs {
                number_of_engines: n_engines as f64,
                bypass_ratio: payload.bypass_ratio,
                overall_pressure_ratio: payload.overall_pressure_ratio,
                fan_pressure_ratio: payload.fan_pressure_ratio,
                turbine_inlet_temperature_k: payload.turbine_inlet_temp_k,
                cruise_mach: config.requirements.cruise_mach,
                cruise_altitude_m: config.requirements.cruise_altitude_m,
                design_thrust_total_n,
            };
            let params = match reference_mode {
                MissionReferenceMode::Product => VehicleBuilderParams {
                    part_power_model: PartPowerModel::IcaoLtoFuelFlow {
                        fuel_flow_ratios: payload.part_power_fuel_flow_ratios,
                    },
                    ..VehicleBuilderParams::default()
                },
                MissionReferenceMode::ReferenceCompatibility => {
                    VehicleBuilderParams::reference_compatibility()
                }
            };
            let sized = match reference_mode {
                MissionReferenceMode::Product => size_turbofan_to_static_rating(&inputs, &params),
                MissionReferenceMode::ReferenceCompatibility => size_turbofan(&inputs, &params),
            };
            let flow = sized.compressor_nondimensional_massflow;
            let legacy_model = LegacyTurbofanModel::new(
                inputs,
                params,
                flow,
                ModelProvenance {
                    model: ModelIdentity {
                        family: "legacy-mission-turbofan".to_owned(),
                        version: "compatibility-v1".to_owned(),
                    },
                    dataset: Some(engine.engine_name.clone()),
                    sources: vec![payload.part_power_source.clone()],
                },
                Vec::new(),
                legacy_installation,
            )
            .map_err(|error| format!("mission propulsion construction failed: {error}"))?;
            if config.mission.model == alas_config::MissionModel::TotalEnergy {
                let model = EmpiricalTurbofanModel::new(
                    EmpiricalTurbofanDeck {
                        takeoff_thrust_n: design_thrust_total_n,
                        // The compatibility catalogue key is still named
                        // `cruise_reference_*`; its documented meaning is
                        // the maximum-climb anchor. Keep that distinction
                        // explicit at the propulsion boundary.
                        max_climb_reference_thrust_n: payload.off_design.cruise_reference_thrust_n
                            * n_engines as f64,
                        max_climb_reference_altitude_m: payload
                            .off_design
                            .cruise_reference_altitude_m,
                        max_climb_reference_mach: payload.off_design.cruise_reference_mach,
                        bypass_ratio: payload.takeoff_bypass_ratio.unwrap_or(payload.bypass_ratio),
                        takeoff_fuel_flow_kg_s: payload.takeoff_fuel_flow_kg_s * n_engines as f64,
                        cruise_reference_tsfc_kg_kgf_h: payload.cruise_tsfc_kg_kgf_hr,
                        part_power_fuel_flow_ratios: payload.part_power_fuel_flow_ratios,
                        // The typed engine catalogue has no engine-specific
                        // climb-rate field. Reuse the configured mission
                        // profile's initial-climb rate when it is valid; it
                        // is an explicit per-aircraft input, not an invented
                        // universal 2500 ft/min engine default. Invalid or
                        // unavailable profile data remains an explicit None,
                        // which selects the correlation's zero-rate baseline.
                        max_climb_rate_ft_min: configured_max_climb_rate_ft_min(config),
                        flight_idle_fraction: 0.07,
                    },
                    legacy_model,
                    ModelProvenance {
                        model: ModelIdentity {
                            family: "bartel-young-openap-turbofan".to_owned(),
                            version: "three-region-v2".to_owned(),
                        },
                        dataset: Some(engine.engine_name.clone()),
                        sources: vec![
                            "Battel & Young (2008), Journal of Aircraft 45(4), DOI 10.2514/1.35589"
                                .to_owned(),
                            format!(
                                "{} [{}]",
                                payload.off_design.source, payload.off_design.evidence
                            ),
                            payload.part_power_source.clone(),
                        ],
                    },
                )
                .map_err(|error| format!("mission propulsion construction failed: {error}"))?;
                (PropulsionOrchestrator::new(model), None)
            } else {
                (
                    PropulsionOrchestrator::new(legacy_model),
                    Some(LegacyTurbofanCompatibility {
                        inputs,
                        params,
                        compressor_nondimensional_massflow: flow,
                    }),
                )
            }
        }
        ActiveEngineModel::Turboprop(payload) => {
            if reference_mode == MissionReferenceMode::ReferenceCompatibility {
                return Err("reference compatibility supports turbofan aircraft only".to_owned());
            }
            let unit_model = Pw127m568fModel {
                normal_takeoff_power_w: payload.takeoff_shaft_power_kw * 1_000.0,
                maximum_takeoff_reserve_power_w: payload.maximum_reserve_shaft_power_kw * 1_000.0,
                maximum_continuous_power_w: payload.maximum_continuous_shaft_power_kw * 1_000.0,
                maximum_climb_power_w: payload.maximum_climb_shaft_power_kw * 1_000.0,
                maximum_cruise_power_w: payload.maximum_cruise_shaft_power_kw * 1_000.0,
                governed_propeller_speed_rpm: payload.governed_propeller_speed_rpm,
                propeller_diameter_m: payload.propeller_diameter_m,
                reference_psfc_kg_kwh: payload.maximum_cruise_fuel_flow_kg_h
                    / (2.0 * payload.maximum_cruise_shaft_power_kw),
                ..Pw127m568fModel::default()
            };
            let model = Atr72TurbopropSystem::new(unit_model, Vec::new(), installation)
                .map_err(|error| format!("mission propulsion construction failed: {error}"))?;
            (PropulsionOrchestrator::new(model), None)
        }
    };

    if config
        .mission
        .route_payload_kg
        .is_some_and(|payload_kg| !payload_kg.is_finite() || payload_kg < 0.0)
    {
        return Err("mission route payload must be finite and nonnegative".to_owned());
    }
    let fuel_loading = plan_fuel_loading(config, &report.design, report);
    if !fuel_loading.zero_fuel_mass_kg.is_finite()
        || !fuel_loading.analyzed_carried_fuel_kg.is_finite()
        || !fuel_loading.analyzed_takeoff_mass_kg.is_finite()
        || fuel_loading.zero_fuel_mass_kg < 0.0
        || fuel_loading.analyzed_carried_fuel_kg <= 0.0
        || fuel_loading.analyzed_takeoff_mass_kg > config.requirements.mtow_kg
    {
        return Err(format!(
            "mission load state is invalid: ZFW={:.3} kg, carried fuel={:.3} kg, TOW={:.3} kg, MTOW limit={:.3} kg",
            fuel_loading.zero_fuel_mass_kg,
            fuel_loading.analyzed_carried_fuel_kg,
            fuel_loading.analyzed_takeoff_mass_kg,
            config.requirements.mtow_kg,
        ));
    }

    let reference_area_m2 = match reference_mode {
        MissionReferenceMode::Product => report.airplane.s_ref,
        MissionReferenceMode::ReferenceCompatibility => geometry_value(report, "wing_area_m2")?,
    };
    let drag_settings = match reference_mode {
        MissionReferenceMode::Product => DragSettings::default(),
        MissionReferenceMode::ReferenceCompatibility => DragSettings::reference_compatibility(),
    };
    if !reference_area_m2.is_finite() || reference_area_m2 <= 0.0 {
        return Err("mission aircraft has no positive finite reference area".to_owned());
    }

    Ok(MissionAnalyses {
        reference_area_m2,
        maximum_lift_coefficient: None,
        takeoff_mass_kg: fuel_loading.analyzed_takeoff_mass_kg,
        minimum_mass_kg: Some(fuel_loading.zero_fuel_mass_kg),
        fuselage_lift_correction: alas_aero::lift_surrogate::FUSELAGE_LIFT_CORRECTION,
        induced_drag_lift_correction: match reference_mode {
            MissionReferenceMode::Product => alas_aero::lift_surrogate::FUSELAGE_LIFT_CORRECTION,
            MissionReferenceMode::ReferenceCompatibility => 1.0,
        },
        enforce_throttle_envelope: matches!(reference_mode, MissionReferenceMode::Product),
        drag_settings,
        wings,
        fuselages: vec![fuselage],
        nacelles,
        // `vehicle_builder.py` appends the nacelles and then one turbofan
        // network containing every engine.
        network_count: 1,
        surrogate,
        legacy_turbofan,
        propulsion,
    })
}

/// Return the configured mission climb rate in the units required by the
/// Bartel--Young correlation. The engine catalogue currently has no
/// engine-specific rate field, so this uses an explicit aircraft mission
/// input rather than inventing a universal propulsion default.
fn configured_max_climb_rate_ft_min(config: &AlasConfig) -> Option<f64> {
    let rate_m_s = config.mission.profile.initial_climb_rate_m_s;
    (rate_m_s.is_finite() && rate_m_s >= 0.0)
        .then_some(rate_m_s * METRES_PER_SECOND_TO_FEET_PER_MINUTE)
}

/// Reproduce the old vehicle-builder's cruise-required turbofan target.
///
/// This target is `MTOW * g / (L/D)` and is intentionally not the product
/// engine rating. Keeping the calculation here makes the 196,878.813 N
/// fixture value explainable: it is the cruise requirement for the frozen
/// report, whereas the product's 934,000 N value is two selected 467 kN
/// static ratings.
fn historical_cruise_required_thrust_total_n(
    config: &AlasConfig,
    report: &AnalysisReport,
) -> Result<f64, String> {
    let l_over_d = report
        .trimmed_design_point
        .as_ref()
        .map(|point| point.l_over_d)
        .filter(|value| value.is_finite() && *value > 0.0)
        .or_else(|| {
            (report.design_point.l_over_d.is_finite() && report.design_point.l_over_d > 0.0)
                .then_some(report.design_point.l_over_d)
        })
        .ok_or_else(|| {
            "reference mission report has no positive finite lift-to-drag ratio".to_owned()
        })?;
    let thrust_n = config.requirements.mtow_kg * 9.81 / l_over_d;
    if !thrust_n.is_finite() || thrust_n <= 0.0 {
        return Err(
            "reference mission report produced no positive finite cruise thrust".to_owned(),
        );
    }
    Ok(thrust_n)
}

fn geometry_value(report: &AnalysisReport, name: &str) -> Result<f64, String> {
    report
        .geometry_summary
        .get(name)
        .copied()
        .ok_or_else(|| format!("mission report has no {name}"))
}

fn mission_wings(
    config: &AlasConfig,
    report: &AnalysisReport,
    reference_mode: MissionReferenceMode,
) -> Result<Vec<WingParams>, String> {
    let geometry = &config.geometry;
    let empennage = &geometry.empennage;
    let (wing_area, hstab_area, vstab_area, main_aspect_ratio) = match reference_mode {
        MissionReferenceMode::Product => {
            let hstab = report
                .airplane
                .wings
                .iter()
                .find(|surface| surface.name == "Horizontal Stabilizer")
                .map(|surface| surface.reference_area())
                .ok_or_else(|| "mission report has no horizontal stabilizer".to_owned())?;
            let vstab = report
                .airplane
                .wings
                .iter()
                .find(|surface| surface.name == "Vertical Stabilizer")
                // Wing::reference_area is an XY projection and is zero for
                // this vertical surface; retain its XZ planform area.
                .map(|surface| surface.unfolded_area())
                .ok_or_else(|| "mission report has no vertical stabilizer".to_owned())?;
            let main = report.airplane.s_ref;
            let ar = report.airplane.b_ref * report.airplane.b_ref / main;
            (main, hstab, vstab, ar)
        }
        MissionReferenceMode::ReferenceCompatibility => (
            geometry_value(report, "wing_area_m2")?,
            geometry_value(report, "h_stab_area_m2")?,
            geometry_value(report, "v_stab_area_m2")?,
            geometry_value(report, "aspect_ratio")?,
        ),
    };
    let hstab_span =
        2.0 * hstab_area / (empennage.hstab_root_chord_m + empennage.hstab_tip_chord_m);
    let vstab_span =
        2.0 * vstab_area / (empennage.vstab_root_chord_m + empennage.vstab_tip_chord_m);
    let hstab_sweep = if empennage.hstab_tip_le_m.1 == 0.0 {
        0.0
    } else {
        empennage.hstab_tip_le_m.0.atan2(empennage.hstab_tip_le_m.1)
    };
    let vstab_sweep = if empennage.vstab_tip_le_m.2 == 0.0 {
        0.0
    } else {
        empennage.vstab_tip_le_m.0.atan2(empennage.vstab_tip_le_m.2)
    };
    let wing = |mean_aerodynamic_chord_m,
                quarter_chord_sweep_rad,
                thickness_to_chord,
                reference_area_m2,
                aspect_ratio| WingParams {
        mean_aerodynamic_chord_m,
        quarter_chord_sweep_rad,
        thickness_to_chord,
        reference_area_m2,
        wetted_area_m2: reference_area_m2 * geometry.wing_wetted_area_factor,
        transition_x_upper: 0.0,
        transition_x_lower: 0.0,
        aspect_ratio,
        inviscid_lift_coefficient: 0.0,
        inviscid_induced_drag_coefficient: 0.0,
    };
    Ok(vec![
        wing(
            geometry_value(report, "mean_aerodynamic_chord_m")?,
            report.design.sweep_deg.to_radians(),
            0.12 * report.design.airfoil_thickness_scale,
            wing_area,
            main_aspect_ratio,
        ),
        wing(
            (empennage.hstab_root_chord_m + empennage.hstab_tip_chord_m) / 2.0,
            hstab_sweep,
            0.10,
            hstab_area,
            hstab_span * hstab_span / hstab_area,
        ),
        wing(
            (empennage.vstab_root_chord_m + empennage.vstab_tip_chord_m) / 2.0,
            vstab_sweep,
            0.08,
            vstab_area,
            vstab_span * vstab_span / vstab_area,
        ),
    ])
}

fn mission_fuselage(config: &AlasConfig, length_m: f64) -> FuselageParams {
    let diameter_m = config.geometry.fuselage.diameter_m;
    FuselageParams {
        length_m,
        effective_diameter_m: diameter_m,
        front_projected_area_m2: PI * diameter_m * diameter_m / 4.0,
        wetted_area_m2: PI * diameter_m * length_m,
    }
}

fn mission_nacelles(
    engine: &alas_config::geometry::EngineConfig,
    number_of_engines: usize,
) -> Vec<NacelleParams> {
    let length_m = engine.nacelle_length_m();
    let diameter_m = 2.0 * engine.radius_scale_m;
    (0..number_of_engines)
        .map(|_| NacelleParams {
            length_m,
            diameter_m,
            wetted_area_m2: 1.1 * PI * diameter_m * length_m,
            origin_count: 1,
        })
        .collect()
}

fn vlm_geometry(
    config: &AlasConfig,
    report: &AnalysisReport,
    reference_mode: MissionReferenceMode,
) -> Result<VlmGeometry, String> {
    // This deliberately follows `vehicle_builder.build_vehicle`, not the
    // display mesh in `report.airplane`. The two aircraft share areas, but the
    // mission model gives its tails different origins and uses
    // the design-vector quarter-chord sweep instead of the cranked mesh's
    // measured mean sweep. Feeding the latter into VORLAX changes the mission
    // surrogate before a segment has begun to iterate.
    let design = &report.design;
    let geometry = &config.geometry;
    let wing = &geometry.wing;
    let tail = &geometry.empennage;
    let main_area = match reference_mode {
        MissionReferenceMode::Product => report.airplane.s_ref,
        MissionReferenceMode::ReferenceCompatibility => geometry_value(report, "wing_area_m2")?,
    };
    if !main_area.is_finite() || main_area <= 0.0 {
        return Err("mission reduced geometry has no positive main-wing area".to_owned());
    }
    let hstab_area = report
        .airplane
        .wings
        .iter()
        .find(|surface| surface.name == "Horizontal Stabilizer")
        .map(|surface| match reference_mode {
            MissionReferenceMode::Product => surface.reference_area(),
            MissionReferenceMode::ReferenceCompatibility => surface.unfolded_area(),
        })
        .ok_or_else(|| {
            "mission reduced geometry requires an explicit horizontal stabilizer; no surrogate tail was substituted"
                .to_owned()
        })?;
    let vstab_area = report
        .airplane
        .wings
        .iter()
        .find(|surface| surface.name == "Vertical Stabilizer")
        // `reference_area()` projects onto XY and is zero for the vertical
        // tail.  Its aerodynamic planform is the unfolded XZ area.
        .map(|surface| surface.unfolded_area())
        .ok_or_else(|| {
            "mission reduced geometry requires an explicit vertical stabilizer; no surrogate tail was substituted"
                .to_owned()
        })?;
    if !hstab_area.is_finite() || hstab_area <= 0.0 {
        return Err(
            "mission reduced geometry has no positive horizontal-stabilizer area".to_owned(),
        );
    }
    if !vstab_area.is_finite() || vstab_area <= 0.0 {
        return Err("mission reduced geometry has no positive vertical-stabilizer area".to_owned());
    }
    let wing_x = wing.root_datum_x_m + design.wing_x_shift_m;
    let hstab_span = 2.0 * hstab_area / (tail.hstab_root_chord_m + tail.hstab_tip_chord_m);
    let vstab_span = 2.0 * vstab_area / (tail.vstab_root_chord_m + tail.vstab_tip_chord_m);

    let main = VlmWing {
        tag: "main_wing".to_owned(),
        symmetric: true,
        vertical: false,
        vortex_lift: false,
        span_projected_m: match reference_mode {
            MissionReferenceMode::Product => report.airplane.b_ref,
            MissionReferenceMode::ReferenceCompatibility => design.span_m,
        },
        chord_root_m: design.root_chord_m,
        chord_tip_m: design.tip_chord_m,
        taper: design.tip_chord_m / design.root_chord_m,
        aspect_ratio: match reference_mode {
            MissionReferenceMode::Product => {
                report.airplane.b_ref * report.airplane.b_ref / main_area
            }
            MissionReferenceMode::ReferenceCompatibility => {
                design.span_m * design.span_m / main_area
            }
        },
        sweep_quarter_chord_rad: design.sweep_deg.to_radians(),
        sweep_leading_edge_rad: None,
        twist_root_rad: wing.root_twist_deg.to_radians(),
        twist_tip_rad: design.tip_twist_deg.to_radians(),
        dihedral_rad: 0.0,
        area_reference_m2: main_area,
        origin_m: [wing_x, 0.0, wing.root_z_m],
    };
    let hstab = VlmWing {
        tag: "horizontal_stabilizer".to_owned(),
        symmetric: true,
        vertical: false,
        vortex_lift: false,
        span_projected_m: hstab_span,
        chord_root_m: tail.hstab_root_chord_m,
        chord_tip_m: tail.hstab_tip_chord_m,
        taper: tail.hstab_tip_chord_m / tail.hstab_root_chord_m,
        aspect_ratio: hstab_span * hstab_span / hstab_area,
        sweep_quarter_chord_rad: tail.hstab_tip_le_m.0.atan2(tail.hstab_tip_le_m.1),
        sweep_leading_edge_rad: None,
        twist_root_rad: tail.hstab_root_twist_deg.to_radians(),
        twist_tip_rad: tail.hstab_tip_twist_deg.to_radians(),
        dihedral_rad: 0.0,
        area_reference_m2: hstab_area,
        origin_m: [
            wing_x + tail.hstab_offset_from_tail_m + design.tail_x_shift_m,
            0.0,
            tail.hstab_z_m,
        ],
    };
    let vstab = VlmWing {
        tag: "vertical_stabilizer".to_owned(),
        symmetric: false,
        vertical: true,
        vortex_lift: false,
        span_projected_m: vstab_span,
        chord_root_m: tail.vstab_root_chord_m,
        chord_tip_m: tail.vstab_tip_chord_m,
        taper: tail.vstab_tip_chord_m / tail.vstab_root_chord_m,
        aspect_ratio: vstab_span * vstab_span / vstab_area,
        sweep_quarter_chord_rad: tail.vstab_tip_le_m.0.atan2(tail.vstab_tip_le_m.2),
        sweep_leading_edge_rad: None,
        twist_root_rad: 0.0,
        twist_tip_rad: 0.0,
        dihedral_rad: 0.0,
        area_reference_m2: vstab_area,
        origin_m: [
            wing_x + tail.vstab_offset_from_tail_m + design.tail_x_shift_m,
            0.0,
            tail.vstab_z_m,
        ],
    };

    // `vehicle_builder` never sets this property. VORLAX therefore takes its
    // fallback moment origin at the main-wing aerodynamic centre, which is
    // `[0, 0, 0]` on the constructed component plus its origin.
    Ok(VlmGeometry {
        reference_area_m2: main_area,
        center_of_gravity_m: [0.0, 0.0, 0.0],
        mean_aerodynamic_chord_m: report.airplane.c_ref,
        reference_span_m: match reference_mode {
            MissionReferenceMode::Product => report.airplane.b_ref,
            MissionReferenceMode::ReferenceCompatibility => design.span_m,
        },
        moment_reference_m: [main.origin_m[0], main.origin_m[2]],
        wings: vec![main, hstab, vstab],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::full_analysis::FullAnalysis;
    use alas_config::airports::get as get_airport;
    use alas_prop::mission_turbofan::freestream_from_atmosphere;

    #[test]
    fn empirical_climb_rate_uses_the_configured_profile_without_a_universal_fallback() {
        let mut config = AlasConfig::default();
        config.mission.profile.initial_climb_rate_m_s = 10.0;
        let configured = configured_max_climb_rate_ft_min(&config)
            .unwrap_or_else(|| panic!("valid mission climb rate"));
        assert!((configured - 1_968.504).abs() < 1.0e-9);
        assert!((configured - 2_500.0).abs() > 1.0e-9);

        config.mission.profile.initial_climb_rate_m_s = -1.0;
        assert_eq!(configured_max_climb_rate_ft_min(&config), None);
        config.mission.profile.initial_climb_rate_m_s = f64::NAN;
        assert_eq!(configured_max_climb_rate_ft_min(&config), None);
    }

    #[test]
    fn typed_empirical_mission_receives_the_configured_climb_rate() {
        let mut zero_rate_config = AlasConfig::default();
        zero_rate_config.mission.model = alas_config::MissionModel::TotalEnergy;
        zero_rate_config.mission.profile.initial_climb_rate_m_s = 0.0;
        let report = FullAnalysis::new(zero_rate_config.clone())
            .run(
                &alas_config::design_variables::DesignVector::default(),
                true,
            )
            .unwrap_or_else(|error| panic!("zero-rate report: {error}"));
        let mut configured_rate_config = zero_rate_config.clone();
        configured_rate_config
            .mission
            .profile
            .initial_climb_rate_m_s = 10.0;
        let zero_rate = build_analyses(&zero_rate_config, &report)
            .unwrap_or_else(|error| panic!("zero-rate mission analyses: {error}"));
        let configured_rate = build_analyses(&configured_rate_config, &report)
            .unwrap_or_else(|error| panic!("configured-rate mission analyses: {error}"));
        let atmosphere = zero_rate.atmosphere(1_500.0, 0.0);
        let speed_m_s = 0.4 * atmosphere.speed_of_sound_m_s;
        let zero_output = zero_rate
            .thrust_for_rating(
                &atmosphere,
                1_500.0,
                speed_m_s,
                0.4,
                9.80665,
                alas_mission::operating::ThrustRating::MaximumClimb,
                1.0,
            )
            .unwrap_or_else(|error| panic!("zero-rate thrust: {error}"));
        let configured_output = configured_rate
            .thrust_for_rating(
                &atmosphere,
                1_500.0,
                speed_m_s,
                0.4,
                9.80665,
                alas_mission::operating::ThrustRating::MaximumClimb,
                1.0,
            )
            .unwrap_or_else(|error| panic!("configured-rate thrust: {error}"));
        assert!((configured_output.thrust_n - zero_output.thrust_n).abs() > 1.0e-3);
    }

    #[test]
    fn schedule_matches_the_reference_order_and_converts_descent_units() {
        let config = AlasConfig::default();
        let origin = get_airport(&config.departure_airport)
            .unwrap_or_else(|error| panic!("default origin: {error}"));
        let destination = get_airport(&config.arrival_airport)
            .unwrap_or_else(|error| panic!("default destination: {error}"));
        let request = build_mission_request(&config, origin, destination, 5_000_000.0);
        let schedule = build_schedule(&request)
            .unwrap_or_else(|error| panic!("default route schedule: {error}"));
        assert_eq!(schedule.len(), 12);
        assert_eq!(schedule[0].tag, "takeoff");
        assert_eq!(schedule[1].tag, "initial_climb");
        assert_eq!(schedule[2].tag, "cruise_step_1");
        assert_eq!(schedule[6].tag, "cruise_step_3");
        assert_eq!(schedule[7].tag, "descent_1");
        assert_eq!(schedule[10].tag, "descent_4");
        assert_eq!(schedule[11].tag, "final_landing");
        let SegmentKind::Descent { altitude_end_m, .. } = schedule[7].kind else {
            panic!("descent schedule entry");
        };
        assert!((altitude_end_m - 9_144.0).abs() < 1e-12);
    }

    #[test]
    fn schedule_closes_the_requested_route_after_profile_legs() {
        let config = AlasConfig::default();
        let origin = get_airport(&config.departure_airport)
            .unwrap_or_else(|error| panic!("default origin: {error}"));
        let destination = get_airport(&config.arrival_airport)
            .unwrap_or_else(|error| panic!("default destination: {error}"));
        let route_distance_m = 6_500_000.0;
        let request = build_mission_request(&config, origin, destination, route_distance_m);
        let schedule = build_schedule(&request)
            .unwrap_or_else(|error| panic!("default route schedule: {error}"));

        let flown_distance_m =
            schedule_horizontal_distance(&schedule, request.departure_elevation_m);
        assert!(
            (flown_distance_m - route_distance_m).abs() < 1e-6,
            "schedule flew {flown_distance_m} m for a {route_distance_m} m route"
        );
    }

    #[test]
    fn preset_like_short_routes_scale_the_altitude_profile_and_close_distance() {
        let config = AlasConfig::default();
        let origin = get_airport(&config.departure_airport)
            .unwrap_or_else(|error| panic!("default origin: {error}"));
        let destination = get_airport(&config.arrival_airport)
            .unwrap_or_else(|error| panic!("default destination: {error}"));
        for route_distance_m in [442_000.0, 547_000.0] {
            let request = build_mission_request(&config, origin, destination, route_distance_m);
            let schedule = build_schedule(&request)
                .unwrap_or_else(|error| panic!("short route schedule: {error}"));
            let flown_distance_m =
                schedule_horizontal_distance(&schedule, request.departure_elevation_m);
            assert!(
                (flown_distance_m - route_distance_m).abs() < 1.0e-6,
                "schedule flew {flown_distance_m} m for a {route_distance_m} m route"
            );
            assert!(schedule.iter().all(|segment| match segment.kind {
                SegmentKind::Climb {
                    altitude_start_m: Some(start),
                    altitude_end_m,
                    ..
                } => altitude_end_m > start,
                SegmentKind::Descent {
                    altitude_start_m: Some(start),
                    altitude_end_m,
                    ..
                } => altitude_end_m < start,
                _ => true,
            }));
        }
    }

    #[test]
    fn a_zero_route_at_one_elevation_has_no_negative_or_vertical_legs() {
        let config = AlasConfig::default();
        let origin = get_airport(&config.departure_airport)
            .unwrap_or_else(|error| panic!("default origin: {error}"));
        let destination = get_airport(&config.arrival_airport)
            .unwrap_or_else(|error| panic!("default destination: {error}"));
        let mut request = build_mission_request(&config, origin, destination, 0.0);
        request.arrival_elevation_m = request.departure_elevation_m;

        let schedule =
            build_schedule(&request).unwrap_or_else(|error| panic!("zero route schedule: {error}"));
        assert_eq!(
            schedule_horizontal_distance(&schedule, request.departure_elevation_m),
            0.0
        );
        assert!(schedule.iter().all(|segment| matches!(
            segment.kind,
            SegmentKind::Cruise {
                distance_m: 0.0,
                ..
            }
        )));
    }

    #[test]
    fn an_irreducible_airport_elevation_change_reports_its_footprint() {
        let config = AlasConfig::default();
        let origin = get_airport(&config.departure_airport)
            .unwrap_or_else(|error| panic!("default origin: {error}"));
        let destination = get_airport(&config.arrival_airport)
            .unwrap_or_else(|error| panic!("default destination: {error}"));
        let mut request = build_mission_request(&config, origin, destination, 0.0);
        request.arrival_elevation_m = request.departure_elevation_m + 1_000.0;

        let error = match build_schedule(&request) {
            Err(error) => error,
            Ok(_) => panic!("zero horizontal distance cannot connect different elevations"),
        };
        assert!(error.contains("connect the airport elevations"), "{error}");
    }

    #[test]
    fn a_positive_route_requires_an_active_cruise_share() {
        let mut config = AlasConfig::default();
        config.mission.profile.cruise_1_distance_fraction = 0.0;
        config.mission.profile.cruise_2_distance_fraction = 0.0;
        config.mission.profile.cruise_3_distance_fraction = 0.0;
        let origin = get_airport(&config.departure_airport)
            .unwrap_or_else(|error| panic!("default origin: {error}"));
        let destination = get_airport(&config.arrival_airport)
            .unwrap_or_else(|error| panic!("default destination: {error}"));
        let request = build_mission_request(&config, origin, destination, 5_000_000.0);

        let error = match build_schedule(&request) {
            Err(error) => error,
            Ok(_) => panic!("the route remainder has no cruise leg"),
        };
        assert!(
            error.contains("active leg"),
            "unexpected route error: {error}"
        );
    }

    #[test]
    fn invalid_cruise_shares_are_rejected_before_schedule_construction() {
        let mut config = AlasConfig::default();
        config.mission.profile.cruise_1_distance_fraction = f64::NAN;
        let origin = get_airport(&config.departure_airport)
            .unwrap_or_else(|error| panic!("default origin: {error}"));
        let destination = get_airport(&config.arrival_airport)
            .unwrap_or_else(|error| panic!("default destination: {error}"));
        let request = build_mission_request(&config, origin, destination, 5_000_000.0);

        let error = match build_schedule(&request) {
            Err(error) => error,
            Ok(_) => panic!("a NaN cruise share is invalid"),
        };
        assert!(
            error.contains("finite and non-negative"),
            "unexpected cruise-share error: {error}"
        );
    }

    #[test]
    fn zero_share_cruise_legs_are_not_scheduled() {
        let mut config = AlasConfig::default();
        config.mission.profile.cruise_1_distance_fraction = 1.0;
        config.mission.profile.cruise_2_distance_fraction = 0.0;
        config.mission.profile.cruise_3_distance_fraction = 0.0;
        let origin = get_airport(&config.departure_airport)
            .unwrap_or_else(|error| panic!("default origin: {error}"));
        let destination = get_airport(&config.arrival_airport)
            .unwrap_or_else(|error| panic!("default destination: {error}"));
        let request = build_mission_request(&config, origin, destination, 5_000_000.0);
        let schedule = build_schedule(&request)
            .unwrap_or_else(|error| panic!("default route schedule: {error}"));
        let tags: Vec<&str> = schedule
            .iter()
            .map(|segment| segment.tag.as_str())
            .collect();
        assert!(!tags.contains(&"step_climb_1"));
        assert!(!tags.contains(&"cruise_step_2"));
        assert!(!tags.contains(&"step_climb_2"));
        assert!(!tags.contains(&"cruise_step_3"));
    }

    #[test]
    fn a_nonzero_departure_isa_deviation_reaches_every_segment() {
        let config = AlasConfig::default();
        let mut origin = get_airport(&config.departure_airport)
            .unwrap_or_else(|error| panic!("default origin: {error}"))
            .clone();
        let destination = get_airport(&config.arrival_airport)
            .unwrap_or_else(|error| panic!("default destination: {error}"));
        origin.isa_deviation_c = 17.0;
        let request = build_mission_request(&config, &origin, destination, 5_000_000.0);
        let schedule = build_schedule(&request)
            .unwrap_or_else(|error| panic!("default route schedule: {error}"));
        assert!(schedule
            .iter()
            .all(|segment| segment.temperature_deviation_k == 17.0));
    }

    #[test]
    fn product_and_reference_mission_inputs_have_distinct_explicit_policies() {
        let config = AlasConfig::default();
        let design = alas_config::design_variables::DesignVector::default();
        let product_report = FullAnalysis::new(config.clone())
            .run(&design, true)
            .unwrap_or_else(|error| panic!("product report: {error}"));
        let reference_report = FullAnalysis::new_reference_compatibility(config.clone())
            .run(&design, true)
            .unwrap_or_else(|error| panic!("reference report: {error}"));

        let product = build_analyses(&config, &product_report)
            .unwrap_or_else(|error| panic!("product mission inputs: {error}"));
        let reference = build_analyses_reference_compatibility(&config, &reference_report)
            .unwrap_or_else(|error| panic!("reference mission inputs: {error}"));

        let product_area = product_report.airplane.s_ref;
        let reference_area = match reference_report
            .geometry_summary
            .get("wing_area_m2")
            .copied()
        {
            Some(area) => area,
            None => panic!("reference geometry summary area"),
        };
        assert_eq!(product.reference_area_m2, product_area);
        assert_eq!(reference.reference_area_m2, reference_area);
        assert!(
            (product_area - reference_area).abs() > 1.0e-6,
            "product and compatibility reference areas must use distinct policies"
        );

        let rated_total_thrust_n = config.geometry.engine.thrust_kn
            * config.geometry.engine.spanwise_positions_m.len() as f64
            * 1000.0;
        let sample_atmosphere = product.atmosphere(3_000.0, 0.0);
        let rated = product.thrust(&sample_atmosphere, 3_000.0, 125.0, 0.38, 9.80665, 1.0);
        assert!(rated.thrust_n > 0.0);
        let reference_l_over_d = reference_report
            .trimmed_design_point
            .map(|point| point.l_over_d)
            .unwrap_or(reference_report.design_point.l_over_d);
        let expected_reference_thrust_n = config.requirements.mtow_kg * 9.81 / reference_l_over_d;
        let reference_legacy = reference
            .legacy_turbofan
            .as_ref()
            .expect("reference compatibility state");
        assert!(
            (reference_legacy.inputs.design_thrust_total_n - expected_reference_thrust_n).abs()
                < 1.0e-9
        );
        assert!(rated_total_thrust_n > reference_legacy.inputs.design_thrust_total_n);
        let product_legacy = product
            .legacy_turbofan
            .as_ref()
            .unwrap_or_else(|| panic!("product turbofan compatibility state"));
        assert_eq!(
            product_legacy.params.part_power_model,
            PartPowerModel::IcaoLtoFuelFlow {
                fuel_flow_ratios: config
                    .geometry
                    .engine
                    .turbofan
                    .as_ref()
                    .unwrap_or_else(|| panic!("typed turbofan payload"))
                    .part_power_fuel_flow_ratios,
            }
        );
        assert_eq!(
            reference_legacy.params.part_power_model,
            PartPowerModel::LegacyLinear
        );
        assert!(product.drag_settings.area_weighted_compressibility);
        assert!(!reference.drag_settings.area_weighted_compressibility);
        assert!(product
            .wings
            .iter()
            .find(|wing| wing.reference_area_m2 > 0.0)
            .is_some());
    }

    #[test]
    fn every_catalogue_turbofan_uses_the_typed_empirical_mission_deck() {
        let mut baseline = AlasConfig::default();
        baseline.mission.model = alas_config::MissionModel::TotalEnergy;
        let report = FullAnalysis::new(baseline.clone())
            .run(
                &alas_config::design_variables::DesignVector::default(),
                true,
            )
            .unwrap_or_else(|error| panic!("baseline report: {error}"));

        for spec in alas_config::engines::database()
            .iter()
            .filter(|spec| spec.technology == alas_config::PropulsionTechnology::Turbofan)
        {
            let mut config = baseline.clone();
            config.geometry.engine.engine_name = spec.name.clone();
            config
                .geometry
                .engine
                .try_apply_engine_spec()
                .unwrap_or_else(|error| panic!("{} binding: {error}", spec.name));
            let analyses = build_analyses(&config, &report)
                .unwrap_or_else(|error| panic!("{} mission deck: {error}", spec.name));
            assert!(analyses.legacy_turbofan.is_none(), "{}", spec.name);
            assert_eq!(
                analyses.propulsion.provenance().model.family,
                "bartel-young-openap-turbofan",
                "{}",
                spec.name
            );
            assert!(analyses
                .propulsion
                .provenance()
                .sources
                .iter()
                .any(|source| source.contains(
                    &spec
                        .off_design
                        .as_ref()
                        .unwrap_or_else(|| panic!("{} off-design data", spec.name))
                        .evidence
                )));

            let n_engines = config.geometry.engine.spanwise_positions_m.len() as f64;
            let typed_spec = spec
                .turbofan_spec()
                .unwrap_or_else(|| panic!("{} typed turbofan", spec.name));
            let sea_level = analyses.atmosphere(0.0, 0.0);
            let takeoff = analyses
                .thrust_for_rating(
                    &sea_level,
                    0.0,
                    0.0,
                    0.0,
                    9.80665,
                    alas_mission::operating::ThrustRating::TakeoffGoAround,
                    1.0,
                )
                .unwrap_or_else(|error| panic!("{} takeoff anchor: {error}", spec.name));
            assert!(
                (takeoff.thrust_n - typed_spec.rated_thrust_kn * 1_000.0 * n_engines).abs()
                    < 1.0e-6,
                "{} static thrust anchor",
                spec.name
            );
            assert!(
                (takeoff.fuel_flow_rate_kg_s - spec.takeoff_fuel_flow_kg_s * n_engines).abs()
                    < 1.0e-12,
                "{} static fuel anchor",
                spec.name
            );

            let off_design = spec
                .off_design
                .as_ref()
                .unwrap_or_else(|| panic!("{} off-design data", spec.name));
            let reference_atmosphere =
                analyses.atmosphere(off_design.cruise_reference_altitude_m, 0.0);
            let reference_speed =
                off_design.cruise_reference_mach * reference_atmosphere.speed_of_sound_m_s;
            let maximum_climb = analyses
                .thrust_for_rating(
                    &reference_atmosphere,
                    off_design.cruise_reference_altitude_m,
                    reference_speed,
                    off_design.cruise_reference_mach,
                    9.80665,
                    alas_mission::operating::ThrustRating::MaximumClimb,
                    1.0,
                )
                .unwrap_or_else(|error| {
                    panic!("{} maximum-climb thrust anchor: {error}", spec.name)
                });
            assert!(
                (maximum_climb.thrust_n - off_design.cruise_reference_thrust_n * n_engines).abs()
                    < 1.0e-6,
                "{} maximum-climb thrust anchor",
                spec.name
            );
        }
    }

    #[test]
    fn atr_mission_constructs_the_typed_turboprop_without_turbofan_state() {
        // Isolate propulsion construction from the ATR planform, which has
        // its own geometry validation coverage and is not under this seam.
        let mut config = AlasConfig::default();
        config.mission.model = alas_config::MissionModel::TotalEnergy;
        let report = FullAnalysis::new(config.clone())
            .run(
                &alas_config::design_variables::DesignVector::default(),
                true,
            )
            .unwrap_or_else(|error| panic!("ATR report: {error}"));
        config.geometry.engine.engine_name = "PW127M".to_owned();
        config
            .geometry
            .engine
            .try_apply_engine_spec()
            .unwrap_or_else(|error| panic!("PW127M binding: {error}"));
        let analyses = build_analyses(&config, &report)
            .unwrap_or_else(|error| panic!("ATR mission analyses: {error}"));

        assert!(analyses.legacy_turbofan.is_none());
        assert_eq!(
            analyses.propulsion.provenance().model.family,
            "pw127m-568f-turboprop-surrogate"
        );
        let atmosphere = analyses.atmosphere(0.0, 0.0);
        let output = analyses.thrust(&atmosphere, 0.0, 0.0, 0.0, 9.80665, 1.0);
        assert!(output.thrust_n.is_finite() && output.thrust_n > 0.0);
        assert!(output.fuel_flow_rate_kg_s.is_finite() && output.fuel_flow_rate_kg_s > 0.0);
        let climb = analyses
            .thrust_for_rating(
                &atmosphere,
                0.0,
                80.0,
                80.0 / atmosphere.speed_of_sound_m_s,
                9.80665,
                alas_mission::operating::ThrustRating::MaximumClimb,
                1.0,
            )
            .unwrap_or_else(|error| panic!("PW127M climb rating: {error}"));
        let cruise = analyses
            .thrust_for_rating(
                &atmosphere,
                0.0,
                80.0,
                80.0 / atmosphere.speed_of_sound_m_s,
                9.80665,
                alas_mission::operating::ThrustRating::Cruise,
                1.0,
            )
            .unwrap_or_else(|error| panic!("PW127M cruise rating: {error}"));
        assert!(climb.thrust_n > cruise.thrust_n);
        assert!(climb.fuel_flow_rate_kg_s > cruise.fuel_flow_rate_kg_s);

        let freestream = freestream_from_atmosphere(
            &atmosphere,
            0.0,
            80.0,
            80.0 / atmosphere.speed_of_sound_m_s,
            9.80665,
        );
        let rated_result = analyses
            .propulsion
            .evaluate(&alas_prop::system::PropulsionRequest {
                flight: (&freestream).into(),
                demand: alas_prop::system::PropulsionDemand::Rating(
                    alas_prop::system::PropulsionRating::Cruise,
                ),
                mode: alas_prop::system::OperatingMode::Normal,
                failure: alas_prop::system::FailureState::None,
                loads: alas_prop::system::PropulsionLoads::default(),
                state: alas_prop::system::PropulsionState::default(),
                time_step_s: None,
            })
            .unwrap_or_else(|error| panic!("PW127M rated resource result: {error}"));
        let fuel_consumption = rated_result.resource_flows[0]
            .mass_flow_kg_s
            .unwrap_or_else(|| panic!("Jet-A mass flow"));
        assert!(fuel_consumption > 0.0);
        assert_eq!(
            rated_result.state_derivatives[0].rate_per_s,
            -fuel_consumption
        );

        let idle_freestream = freestream_from_atmosphere(
            &atmosphere,
            0.0,
            80.0,
            80.0 / atmosphere.speed_of_sound_m_s,
            9.80665,
        );
        let idle_result = analyses
            .propulsion
            .evaluate(&alas_prop::system::PropulsionRequest {
                flight: (&idle_freestream).into(),
                demand: alas_prop::system::PropulsionDemand::RatedFraction {
                    rating: alas_prop::system::PropulsionRating::FlightIdle,
                    fraction: 1.0,
                },
                mode: alas_prop::system::OperatingMode::Normal,
                failure: alas_prop::system::FailureState::None,
                loads: alas_prop::system::PropulsionLoads::default(),
                state: alas_prop::system::PropulsionState::default(),
                time_step_s: None,
            })
            .unwrap_or_else(|error| panic!("PW127M idle surrogate: {error}"));
        assert!(matches!(
            idle_result.validity,
            alas_prop::system::ValidityStatus::Extrapolated { .. }
        ));
        let idle_fuel = idle_result.resource_flows[0]
            .mass_flow_kg_s
            .unwrap_or_else(|| panic!("PW127M idle Jet-A flow"));
        assert!(idle_fuel > 0.0);

        let error = build_analyses_reference_compatibility(&config, &report)
            .err()
            .unwrap_or_else(|| panic!("turboprop reference compatibility must be rejected"));
        assert!(error.contains("turbofan aircraft only"), "{error}");
    }

    #[test]
    fn atr_preset_reaches_full_analysis_with_its_typed_propulsion() {
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": "ATR72-600" }))
            .unwrap_or_else(|error| panic!("ATR preset: {error}"));
        let preset = alas_config::presets::get("ATR72-600")
            .unwrap_or_else(|error| panic!("ATR design vector: {error}"));
        let report = FullAnalysis::new(config.clone())
            .run(&preset.design_vector, true)
            .unwrap_or_else(|error| panic!("ATR full analysis: {error}"));
        let analyses = build_analyses(&config, &report)
            .unwrap_or_else(|error| panic!("ATR propulsion analyses: {error}"));
        assert!(analyses.legacy_turbofan.is_none());
        assert_eq!(
            analyses.propulsion.provenance().model.family,
            "pw127m-568f-turboprop-surrogate"
        );
    }

    #[test]
    fn a_pipeline_report_can_be_flown_by_the_native_mission() {
        let config = AlasConfig::default();
        let origin = get_airport(&config.departure_airport)
            .unwrap_or_else(|error| panic!("default origin: {error}"));
        let destination = get_airport(&config.arrival_airport)
            .unwrap_or_else(|error| panic!("default destination: {error}"));
        let report = FullAnalysis::new(config.clone())
            .run(
                &alas_config::design_variables::DesignVector::default(),
                true,
            )
            .unwrap_or_else(|error| panic!("default report: {error}"));
        let analyses = build_analyses(&config, &report)
            .unwrap_or_else(|error| panic!("mission analyses: {error}"));
        let sample = analyses.surrogate.evaluate(0.05, 0.7);
        assert!(sample.inviscid_lift_coefficient.is_finite());
        assert!(sample.inviscid_induced_drag_coefficient.is_finite());
        let atmosphere = analyses.atmosphere(3000.0, 0.0);
        let orchestrated = analyses.thrust(&atmosphere, 3000.0, 125.0, 0.38, 9.80665, 0.63);
        assert!(orchestrated.thrust_n.is_finite() && orchestrated.thrust_n > 0.0);
        assert!(orchestrated.fuel_flow_rate_kg_s.is_finite());
        let aero_sample = analyses.aerodynamics(0.0, 0.38, atmosphere.temperature_k, 1.0e7);
        assert!(
            aero_sample.drag.total.is_finite(),
            "drag={:?} wings={:?} fuselages={:?} nacelles={:?}",
            aero_sample.drag,
            analyses.wings,
            analyses.fuselages,
            analyses.nacelles
        );
        let request = build_mission_request(&config, origin, destination, 5_000_000.0);
        let first_spec = build_schedule(&request)
            .unwrap_or_else(|error| panic!("default route schedule: {error}"))
            .into_iter()
            .next()
            .unwrap_or_else(|| panic!("takeoff spec"));
        let mut first_segment = alas_mission::segments::Segment::new(first_spec, None)
            .unwrap_or_else(|error| panic!("takeoff segment: {error}"));
        first_segment.iterate(&analyses);
        assert!(
            first_segment
                .residuals
                .iter()
                .flatten()
                .all(|value| value.is_finite()),
            "nonfinite residuals {:?}; conditions mach={:?} aoa={:?} drag={:?}",
            first_segment.residuals,
            first_segment.conditions.mach,
            first_segment.conditions.angle_of_attack_rad,
            first_segment.conditions.drag_coefficient
        );

        let mut hot_origin = origin.clone();
        hot_origin.isa_deviation_c = 15.0;
        let hot_request = build_mission_request(&config, &hot_origin, destination, 5_000_000.0);
        let cold = Mission {
            schedule: build_schedule(&request)
                .unwrap_or_else(|error| panic!("default route schedule: {error}")),
        }
        .evaluate(&analyses)
        .unwrap_or_else(|error| panic!("standard-day mission produces conditions: {error}"));
        let hot = Mission {
            schedule: build_schedule(&hot_request)
                .unwrap_or_else(|error| panic!("hot route schedule: {error}")),
        }
        .evaluate(&analyses)
        .unwrap_or_else(|error| panic!("hot-day mission produces conditions: {error}"));
        let cold_density = cold.segments[0].conditions.density_kg_m3[0];
        let hot_density = hot.segments[0].conditions.density_kg_m3[0];
        let cold_temperature = cold.segments[0].conditions.temperature_k[0];
        let hot_temperature = hot.segments[0].conditions.temperature_k[0];
        assert!(hot_temperature > cold_temperature);
        assert!(hot_density < cold_density);

        let result = evaluate(&config, &report, origin, destination, 5_000_000.0)
            .unwrap_or_else(|error| panic!("default report flies: {error}"));
        assert_eq!(result.solutions.len(), result.scheduled_segment_count);
        assert!(result.solutions.iter().all(|solution| solution.converged));
        assert_eq!(result.segments.len(), result.scheduled_segment_count);
        assert!(result.completed_summary().is_some());
        assert!(result
            .segments
            .iter()
            .all(|segment| segment.conditions.len() == 16));
        assert!(result
            .segments
            .iter()
            .flat_map(|segment| segment.conditions.throttle.iter())
            .all(|throttle| throttle.is_finite() && (0.0..=1.0).contains(throttle)));
        assert!(result.initial_mass_kg() > result.final_mass_kg());
        assert!(result.fuel_burned_kg() > 0.0);
        assert!(result.block_time_s() > 0.0);
    }
}

#[cfg(test)]
mod w64;
