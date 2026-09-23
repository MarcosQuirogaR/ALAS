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
#[cfg(test)]
use alas_mission::Mission;
use alas_mission::{build_mission_request, MissionResult};
#[cfg(test)]
use alas_prop::mission_turbofan::PartPowerModel;
use alas_prop::mission_turbofan::{size_turbofan, TurbofanInputs, VehicleBuilderParams};
use alas_prop::system::{
    LegacyTurbofanModel, ModelIdentity, ModelProvenance, PropulsionInstallation,
    PropulsionOrchestrator,
};

use alas_opt::mdo::propulsion::product_orchestrator;

use crate::feasibility::plan_fuel_loading;
use crate::full_analysis::AnalysisReport;

pub mod dispatch;
mod flight;
mod guidance;
mod schedule;

pub(crate) use dispatch::{LoadCaseSelection, SelectedLoadCase};
use guidance::{adapt_failed_climb, adapt_failed_cruise, adapt_failed_descent};
use schedule::build_schedule;
#[cfg(test)]
use schedule::schedule_horizontal_distance;

const METRES_PER_SECOND_TO_FEET_PER_MINUTE: f64 = 3.28084 * 60.0;

/// Run the native mission for the selected report and route.
///
/// The route is flown at the load case [`dispatch::select_load_case`]
/// chooses: the takeoff mass the fuel policy requires, or the frozen
/// maximum-available-fuel case when the policy is switched off. The
/// selection itself is returned beside the flown result so the feasibility
/// stage can report the reserve plan the flight was sized to.
pub(crate) fn evaluate(
    config: &AlasConfig,
    report: &AnalysisReport,
    origin: &Airport,
    destination: &Airport,
    route_distance_m: f64,
) -> Result<(MissionResult, SelectedLoadCase), String> {
    let request = build_mission_request(config, origin, destination, route_distance_m);
    let schedule = build_schedule(&request)?;
    let mut analyses = build_analyses(config, report)?;
    let fuel_loading = plan_fuel_loading(config, &report.design, report);
    let load_case = dispatch::select_load_case(
        config,
        report,
        &fuel_loading,
        &mut analyses,
        &request,
        &schedule,
    )?;
    let result = flight::fly_with_guidance(schedule, &request, &analyses)?;
    Ok((result, load_case))
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
        ActiveEngineModel::Turbofan(payload)
            if reference_mode == MissionReferenceMode::ReferenceCompatibility =>
        {
            // The historical vehicle fixture sizes its turbofan from
            // cruise-required thrust with the frozen compatibility parameters;
            // the legacy scalar evaluator has no per-unit moment model, so its
            // equivalent thrust line runs through the mission reference point.
            let legacy_installation = PropulsionInstallation {
                unit_positions_m: engine
                    .spanwise_positions_m
                    .iter()
                    .map(|&y_m| [0.0, y_m, 0.0])
                    .collect(),
                ..installation.clone()
            };
            let design_thrust_total_n = historical_cruise_required_thrust_total_n(config, report)?;
            if !design_thrust_total_n.is_finite() || design_thrust_total_n <= 0.0 {
                return Err(
                    "reference mission aircraft has no positive finite cruise-required thrust"
                        .to_owned(),
                );
            }
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
            let params = VehicleBuilderParams::reference_compatibility();
            let sized = size_turbofan(&inputs, &params);
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
            (
                PropulsionOrchestrator::new(legacy_model),
                Some(LegacyTurbofanCompatibility {
                    inputs,
                    params,
                    compressor_nondimensional_massflow: flow,
                }),
            )
        }
        ActiveEngineModel::Turboprop(_)
            if reference_mode == MissionReferenceMode::ReferenceCompatibility =>
        {
            return Err("reference compatibility supports turbofan aircraft only".to_owned());
        }
        ActiveEngineModel::Turbofan(_) | ActiveEngineModel::Turboprop(_) => {
            // The product path shares one constructor with the optimizer's
            // candidate mission model, so the finalist mission and candidate
            // ranking read the same off-design deck.
            let (orchestrator, _) = product_orchestrator(
                engine,
                installation,
                config.requirements.cruise_mach,
                config.requirements.cruise_altitude_m,
                configured_max_climb_rate_ft_min(config),
            )?;
            (orchestrator, None)
        }
    };

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
            // SUAVE's Fidelity_Zero `fuselage_lift_correction` multiplies the
            // aircraft lift used for force balance. It does not say to scale
            // each VLM wing's induced drag, and `MissionAnalyses` squares this
            // field before the drag buildup. Keeping this at unity avoids an
            // unsupported CDi bias; a future calibrated wing-load model can
            // opt in explicitly at this boundary.
            MissionReferenceMode::Product => 1.0,
            MissionReferenceMode::ReferenceCompatibility => 1.0,
        },
        signed_cruise_force_residual: matches!(reference_mode, MissionReferenceMode::Product),
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

/// Convert the explicitly configured initial-climb rate to the units used by
/// the OpenAP/Bartel-Young maximum-climb correlation. Missing or invalid
/// configuration is represented as `None`; no universal aircraft-independent
/// climb rate is invented at the propulsion boundary.
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
    // 9.81, not `config.requirements.gravity_m_s2` (9.80665 as of physics
    // review v1.2, finding F5): this function's whole purpose is
    // reproducing the frozen 196,878.813 N fixture above, so it keeps the
    // frozen two-decimal constant rather than the corrected one.
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
    let mission_hstab_incidence_deg = match reference_mode {
        MissionReferenceMode::Product => report
            .trimmed_design_point
            .map(|point| point.trim_ih_deg)
            .filter(|incidence| incidence.is_finite())
            .unwrap_or(tail.hstab_root_twist_deg),
        MissionReferenceMode::ReferenceCompatibility => tail.hstab_root_twist_deg,
    };

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
        // The full analysis solves the cruise pitching-moment trim before the
        // mission surrogate is trained. A fixed trimmed incidence is the
        // closest available cruise surrogate; the point-mass mission itself
        // has no elevator or Cm residual, so phase-specific trim remains a
        // declared fidelity limit rather than being implied here.
        twist_root_rad: mission_hstab_incidence_deg.to_radians(),
        twist_tip_rad: mission_hstab_incidence_deg.to_radians(),
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
    fn a_vertical_rate_equal_to_airspeed_is_rejected_before_geometry() {
        let mut config = AlasConfig::default();
        config.mission.profile.takeoff_climb_rate_m_s =
            config.mission.profile.takeoff_air_speed_m_s;
        let origin = get_airport(&config.departure_airport)
            .unwrap_or_else(|error| panic!("default origin: {error}"));
        let destination = get_airport(&config.arrival_airport)
            .unwrap_or_else(|error| panic!("default destination: {error}"));
        let request = build_mission_request(&config, origin, destination, 5_000_000.0);

        let error = match build_schedule(&request) {
            Err(error) => error,
            Ok(_) => panic!("a vertical rate equal to airspeed is invalid"),
        };
        assert!(error.contains("must be below airspeed"), "{error}");
    }

    #[test]
    fn a_nonfinite_cruise_speed_is_rejected_before_geometry() {
        let mut config = AlasConfig::default();
        config.mission.profile.cruise_2_air_speed_m_s = f64::NAN;
        let origin = get_airport(&config.departure_airport)
            .unwrap_or_else(|error| panic!("default origin: {error}"));
        let destination = get_airport(&config.arrival_airport)
            .unwrap_or_else(|error| panic!("default destination: {error}"));
        let request = build_mission_request(&config, origin, destination, 5_000_000.0);

        let error = match build_schedule(&request) {
            Err(error) => error,
            Ok(_) => panic!("a NaN cruise speed is invalid"),
        };
        assert!(error.contains("cruise 2 airspeed"), "{error}");
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

        let reference_l_over_d = reference_report
            .trimmed_design_point
            .map(|point| point.l_over_d)
            .unwrap_or(reference_report.design_point.l_over_d);
        let expected_reference_thrust_n = config.requirements.mtow_kg * 9.81 / reference_l_over_d;
        let reference_legacy = reference
            .legacy_turbofan
            .as_ref()
            .unwrap_or_else(|| panic!("reference mission retains legacy compatibility inputs"));
        assert!(
            (reference_legacy.inputs.design_thrust_total_n - expected_reference_thrust_n).abs()
                < 1.0e-9
        );
        assert!(product.legacy_turbofan.is_none());
        assert_eq!(
            product.propulsion.provenance().model.family,
            "bartel-young-openap-turbofan"
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

        // A command above the rating must still move the force. While the
        // solver's throttle was clamped inside the residual closure every
        // command above one produced the identical force, so the residual was
        // flat there, its Jacobian column was zero, and `hybrd` stalled with
        // `NoProgressSinceIterations` instead of reporting a thrust shortfall
        // - measured on the delivered AVE finalist as a request of 128.246 on
        // `descent_4` while the recorded throttle read exactly 1.000. This is
        // the property that was missing.
        // Force, the deck's reported flight-idle floor, and fuel flow at one
        // commanded throttle, taken through the real takeoff segment.
        let probe_at = |command: f64| -> (f64, f64, f64) {
            let mut probe = alas_mission::segments::Segment::new(
                build_schedule(&request)
                    .unwrap_or_else(|error| panic!("probe schedule: {error}"))
                    .into_iter()
                    .next()
                    .unwrap_or_else(|| panic!("probe takeoff spec")),
                None,
            )
            .unwrap_or_else(|error| panic!("probe segment: {error}"));
            for throttle in &mut probe.throttle {
                *throttle = command;
            }
            probe.iterate(&analyses);
            (
                probe.conditions.thrust_force_vector_n[0][0],
                probe.conditions.available_throttle_floor[0],
                probe.conditions.vehicle_mass_rate_kg_s[0],
            )
        };
        let thrust_at = |command: f64| probe_at(command).0;
        let rated = thrust_at(1.0);
        let over = thrust_at(1.5);
        assert!(
            rated.is_finite() && over.is_finite() && rated > 0.0,
            "the probe must produce a usable rated force: rated={rated}, over={over}"
        );
        assert!(
            over > rated * 1.4,
            "a command above the rating must continue the force, not repeat it: \
             1.0 -> {rated} N, 1.5 -> {over} N"
        );
        // And nothing inside the envelope moves: this is the guarantee that
        // the continuation cannot change a segment that already converged.
        let half = thrust_at(0.5);
        assert!(
            half.is_finite() && half < rated,
            "inside the envelope the deck is unchanged: 0.5 -> {half} N, 1.0 -> {rated} N"
        );

        // The same property at the *lower* bound, which is the one that
        // actually stopped the A320-200. A deck's lowest deliverable force is
        // flight idle, a positive fraction of the rating, so the whole band
        // below it produced the identical force: the residual was flat there,
        // the throttle block of the Jacobian was zero, and a root find that
        // wandered into the band could never leave it. Measured on the
        // product deck over an A320-200 `descent_1`, the solve stalled
        // `NoProgressSinceIterations` after 215 residual evaluations at a
        // request of -6.761 while a root existed at 0.737-0.934, entirely
        // inside the envelope.
        let (at_zero, floor, idle_fuel_flow) = probe_at(0.0);
        assert!(
            floor.is_finite() && floor > 0.0 && floor < 1.0,
            "the deck must report a positive flight-idle floor below the rating: {floor}"
        );
        let below = thrust_at(0.5 * floor);
        let (negative, _, fuel_below) = probe_at(-0.5);
        assert!(
            at_zero.is_finite() && below.is_finite() && negative.is_finite(),
            "the continuation must stay finite below the floor: \
             -0.5 -> {negative} N, 0.0 -> {at_zero} N, {floor} -> {below} N"
        );
        assert!(
            negative < at_zero && at_zero < below && below < half,
            "below the flight-idle floor the force must keep a gradient, not repeat: \
             -0.5 -> {negative} N, 0.0 -> {at_zero} N, half-floor -> {below} N, \
             0.5 -> {half} N"
        );
        // Fuel flow is never continued through the floor: the point is
        // refused either way, and a burn below idle would feed the mass ODE a
        // mass the aeroplane never lost.
        assert!(
            (fuel_below - idle_fuel_flow).abs() <= 1.0e-12 * idle_fuel_flow.abs().max(1.0),
            "fuel flow must stay at the idle value below the floor: \
             {fuel_below} kg/s against {idle_fuel_flow} kg/s"
        );
        // Nothing at or above the floor is touched: the deck answers directly
        // and reports no floor at all.
        let (_, no_floor, _) = probe_at(0.5);
        assert!(
            no_floor == 0.0,
            "a command the deck answers directly must report no floor: {no_floor}"
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

        let (result, load_case) = evaluate(&config, &report, origin, destination, 5_000_000.0)
            .unwrap_or_else(|error| panic!("default report flies: {error}"));
        assert!((result.initial_mass_kg() - load_case.takeoff_mass_kg).abs() < 1.0e-6);
        assert_eq!(result.solutions.len(), result.scheduled_segment_count);
        assert!(result
            .solutions
            .iter()
            .all(|solution| solution.converged && !solution.throttle_limited));
        assert_eq!(result.segments.len(), result.scheduled_segment_count);
        assert!(result.completed_summary().is_some());
        assert!(result.figure_data_ready());
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
