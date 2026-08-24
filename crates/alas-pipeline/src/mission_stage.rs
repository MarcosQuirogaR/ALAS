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
use alas_config::AlasConfig;
use alas_mission::segments::{MissionAnalyses, SegmentKind, SegmentSpec};
use alas_mission::{build_mission_request, Mission, MissionRequest, MissionResult};
use alas_prop::mission_turbofan::{size_turbofan, TurbofanInputs, VehicleBuilderParams};

use crate::feasibility::plan_fuel_loading;
use crate::full_analysis::AnalysisReport;

const CONTROL_POINTS: usize = 16;
const METRES_PER_FOOT: f64 = 0.3048;
const METRES_PER_NAUTICAL_MILE: f64 = 1852.0;

/// Run the native mission for the selected report and route.
pub(crate) fn evaluate(
    config: &AlasConfig,
    report: &AnalysisReport,
    origin: &Airport,
    destination: &Airport,
    route_distance_m: f64,
) -> Result<MissionResult, String> {
    let analyses = build_analyses(config, report)?;
    let request = build_mission_request(config, origin, destination, route_distance_m);
    let schedule = build_schedule(&request);
    Mission { schedule }
        .evaluate(&analyses)
        .map_err(|error| format!("native mission failed: {error}"))
}

fn build_analyses(config: &AlasConfig, report: &AnalysisReport) -> Result<MissionAnalyses, String> {
    let vlm_geometry = vlm_geometry(config, report);
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
    let wings = mission_wings(config, report)?;
    let fuselage = mission_fuselage(config, report.design.fuselage_length_m);
    let nacelles = mission_nacelles(engine, n_engines);
    let lift_to_drag = report
        .trimmed_design_point
        .map(|point| point.l_over_d)
        .unwrap_or(report.design_point.l_over_d);
    if !lift_to_drag.is_finite() || lift_to_drag <= 0.0 {
        return Err("mission report has no positive lift-to-drag ratio".to_owned());
    }
    let engine_inputs = TurbofanInputs {
        number_of_engines: n_engines as f64,
        bypass_ratio: engine.bypass_ratio,
        overall_pressure_ratio: engine.overall_pressure_ratio,
        fan_pressure_ratio: engine.fan_pressure_ratio,
        turbine_inlet_temperature_k: engine.turbine_inlet_temp_k,
        cruise_mach: config.requirements.cruise_mach,
        cruise_altitude_m: config.requirements.cruise_altitude_m,
        design_thrust_total_n: config.requirements.mtow_kg * 9.81 / lift_to_drag,
    };
    let sized_engine = size_turbofan(&engine_inputs, &VehicleBuilderParams::default());

    let fuel_loading = plan_fuel_loading(config, &report.design, report);

    Ok(MissionAnalyses {
        reference_area_m2: geometry_value(report, "wing_area_m2")?,
        maximum_lift_coefficient: None,
        takeoff_mass_kg: fuel_loading.analyzed_takeoff_mass_kg,
        minimum_mass_kg: Some(fuel_loading.zero_fuel_mass_kg),
        fuselage_lift_correction: alas_aero::lift_surrogate::FUSELAGE_LIFT_CORRECTION,
        drag_settings: DragSettings::default(),
        wings,
        fuselages: vec![fuselage],
        nacelles,
        // `vehicle_builder.py` appends the nacelles and then one turbofan
        // network containing every engine.
        network_count: 1,
        surrogate,
        turbofan: engine_inputs,
        turbofan_params: VehicleBuilderParams::default(),
        compressor_nondimensional_massflow: sized_engine.compressor_nondimensional_massflow,
    })
}

fn geometry_value(report: &AnalysisReport, name: &str) -> Result<f64, String> {
    report
        .geometry_summary
        .get(name)
        .copied()
        .ok_or_else(|| format!("mission report has no {name}"))
}

fn mission_wings(config: &AlasConfig, report: &AnalysisReport) -> Result<Vec<WingParams>, String> {
    let geometry = &config.geometry;
    let empennage = &geometry.empennage;
    let wing_area = geometry_value(report, "wing_area_m2")?;
    let hstab_area = geometry_value(report, "h_stab_area_m2")?;
    let vstab_area = geometry_value(report, "v_stab_area_m2")?;
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
            geometry_value(report, "aspect_ratio")?,
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

fn vlm_geometry(config: &AlasConfig, report: &AnalysisReport) -> VlmGeometry {
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
    let main_area = report.airplane.s_ref;
    let hstab_area = report
        .airplane
        .wings
        .iter()
        .find(|surface| surface.name == "Horizontal Stabilizer")
        .map(|surface| surface.area())
        .unwrap_or(0.20 * main_area);
    let vstab_area = report
        .airplane
        .wings
        .iter()
        .find(|surface| surface.name == "Vertical Stabilizer")
        .map(|surface| surface.area())
        .unwrap_or(0.10 * main_area);
    let wing_x = wing.root_datum_x_m + design.wing_x_shift_m;
    let hstab_span = 2.0 * hstab_area / (tail.hstab_root_chord_m + tail.hstab_tip_chord_m);
    let vstab_span = 2.0 * vstab_area / (tail.vstab_root_chord_m + tail.vstab_tip_chord_m);

    let main = VlmWing {
        tag: "main_wing".to_owned(),
        symmetric: true,
        vertical: false,
        vortex_lift: false,
        span_projected_m: design.span_m,
        chord_root_m: design.root_chord_m,
        chord_tip_m: design.tip_chord_m,
        taper: design.tip_chord_m / design.root_chord_m,
        aspect_ratio: design.span_m * design.span_m / main_area,
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
    VlmGeometry {
        reference_area_m2: main_area,
        center_of_gravity_m: [0.0, 0.0, 0.0],
        mean_aerodynamic_chord_m: report.airplane.c_ref,
        reference_span_m: design.span_m,
        moment_reference_m: [main.origin_m[0], main.origin_m[2]],
        wings: vec![main, hstab, vstab],
    }
}

fn build_schedule(request: &MissionRequest) -> Vec<SegmentSpec> {
    let p = &request.profile;
    let cruise_altitude = request.cruise_altitude_m;
    let first_level = (cruise_altitude * p.initial_climb_altitude_fraction)
        .max(request.departure_elevation_m + 3000.0);
    let second_level =
        (cruise_altitude * p.step_climb_1_altitude_fraction).max(first_level + 300.0);
    let leg = |fraction: f64| (request.route_distance_m * fraction).max(METRES_PER_NAUTICAL_MILE);
    let temperature_deviation_k = request.departure_isa_deviation_c;
    let mut schedule = Vec::with_capacity(12);

    schedule.push(climb(
        "takeoff",
        Some(request.departure_elevation_m),
        request.departure_elevation_m + p.takeoff_altitude_gain_m,
        p.takeoff_air_speed_m_s,
        p.takeoff_climb_rate_m_s,
        temperature_deviation_k,
    ));
    schedule.push(climb(
        "initial_climb",
        None,
        first_level,
        p.initial_climb_air_speed_m_s,
        p.initial_climb_rate_m_s,
        temperature_deviation_k,
    ));
    let active_cruise = [
        p.cruise_1_distance_fraction > 0.0,
        p.cruise_2_distance_fraction > 0.0,
        p.cruise_3_distance_fraction > 0.0,
    ];
    if active_cruise[0] {
        schedule.push(cruise(
            "cruise_step_1",
            None,
            leg(p.cruise_1_distance_fraction),
            p.cruise_1_air_speed_m_s,
            temperature_deviation_k,
        ));
    }
    if active_cruise[1] {
        schedule.push(climb(
            "step_climb_1",
            None,
            second_level,
            p.step_climb_1_air_speed_m_s,
            p.step_climb_1_rate_m_s,
            temperature_deviation_k,
        ));
        schedule.push(cruise(
            "cruise_step_2",
            None,
            leg(p.cruise_2_distance_fraction),
            p.cruise_2_air_speed_m_s,
            temperature_deviation_k,
        ));
    }
    if active_cruise[2] {
        schedule.push(climb(
            "step_climb_2",
            None,
            cruise_altitude,
            p.step_climb_2_air_speed_m_s,
            p.step_climb_2_rate_m_s,
            temperature_deviation_k,
        ));
        schedule.push(cruise(
            "cruise_step_3",
            None,
            leg(p.cruise_3_distance_fraction),
            p.cruise_3_air_speed_m_s,
            temperature_deviation_k,
        ));
    }

    let descent_steps = [
        (
            p.descent_1_altitude_ft,
            p.descent_1_air_speed_m_s,
            p.descent_1_rate_m_s,
        ),
        (
            p.descent_2_altitude_ft,
            p.descent_2_air_speed_m_s,
            p.descent_2_rate_m_s,
        ),
        (
            p.descent_3_altitude_ft,
            p.descent_3_air_speed_m_s,
            p.descent_3_rate_m_s,
        ),
        (
            p.descent_4_altitude_ft,
            p.descent_4_air_speed_m_s,
            p.descent_4_rate_m_s,
        ),
    ];
    let arrival_ft = request.arrival_elevation_m / METRES_PER_FOOT;
    for (index, (altitude_ft, speed, rate)) in descent_steps.into_iter().enumerate() {
        if altitude_ft > arrival_ft {
            schedule.push(descent(
                &format!("descent_{}", index + 1),
                None,
                altitude_ft * METRES_PER_FOOT,
                speed,
                rate,
                temperature_deviation_k,
            ));
        }
    }
    schedule.push(descent(
        "final_landing",
        None,
        request.arrival_elevation_m,
        p.landing_air_speed_m_s,
        p.landing_descent_rate_m_s,
        temperature_deviation_k,
    ));
    schedule
}

fn climb(
    tag: &str,
    start: Option<f64>,
    end: f64,
    speed: f64,
    rate: f64,
    temperature_deviation_k: f64,
) -> SegmentSpec {
    SegmentSpec {
        tag: tag.to_owned(),
        kind: SegmentKind::Climb {
            altitude_start_m: start,
            altitude_end_m: end,
            climb_rate_m_s: rate,
        },
        air_speed_m_s: speed,
        true_course_rad: 0.0,
        temperature_deviation_k,
        number_control_points: CONTROL_POINTS,
    }
}

fn cruise(
    tag: &str,
    altitude: Option<f64>,
    distance: f64,
    speed: f64,
    temperature_deviation_k: f64,
) -> SegmentSpec {
    SegmentSpec {
        tag: tag.to_owned(),
        kind: SegmentKind::Cruise {
            altitude_m: altitude,
            distance_m: distance,
        },
        air_speed_m_s: speed,
        true_course_rad: 0.0,
        temperature_deviation_k,
        number_control_points: CONTROL_POINTS,
    }
}

fn descent(
    tag: &str,
    start: Option<f64>,
    end: f64,
    speed: f64,
    rate: f64,
    temperature_deviation_k: f64,
) -> SegmentSpec {
    SegmentSpec {
        tag: tag.to_owned(),
        kind: SegmentKind::Descent {
            altitude_start_m: start,
            altitude_end_m: end,
            descent_rate_m_s: rate,
        },
        air_speed_m_s: speed,
        true_course_rad: 0.0,
        temperature_deviation_k,
        number_control_points: CONTROL_POINTS,
    }
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
        let schedule = build_schedule(&request);
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
        let schedule = build_schedule(&request);
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
        let schedule = build_schedule(&request);
        assert!(schedule
            .iter()
            .all(|segment| segment.temperature_deviation_k == 17.0));
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
            schedule: build_schedule(&request),
        }
        .evaluate(&analyses)
        .unwrap_or_else(|error| panic!("standard-day mission produces conditions: {error}"));
        let hot = Mission {
            schedule: build_schedule(&hot_request),
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
        assert!(
            result.solutions.iter().all(|solution| solution.converged),
            "dynamic mission convergence diagnostics: {:?}",
            result.solutions
        );
        assert_eq!(result.segments.len(), 12);
        assert!(result
            .segments
            .iter()
            .all(|segment| segment.conditions.len() == 16));
        assert!(result.initial_mass_kg() > result.final_mass_kg());
        assert!(result.fuel_burned_kg() > 0.0);
        assert!(result.block_time_s() > 0.0);
        assert!(result
            .segments
            .iter()
            .flat_map(|segment| segment.conditions.aircraft_range_m.iter())
            .any(|&range| range > 4_000_000.0));
    }
}

#[cfg(test)]
mod w64;
