// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reading the mission fixtures back into the types the port runs on.
//!
//! Both mission fixtures carry the same two input blocks -- the resolved
//! vehicle and the trained surrogate tables -- because both were written from
//! one SUAVE run, so one reader serves both. Nothing here computes: every
//! number is transcribed from what the reference's own objects held after
//! `simple_sizing` and `finalize`, which is the point. A test that rebuilt the
//! vehicle from the request would be checking a vehicle assembly this port
//! does not contain.

#![allow(dead_code)] // each test binary uses a different part of this reader

use std::collections::BTreeMap;

use alas_aero::drag_buildup::{DragSettings, FuselageParams, NacelleParams, WingParams};
use alas_aero::lift_surrogate::{LiftSurrogate, TrainingGrid, TrainingTables};
use alas_mission::segments::{MissionAnalyses, SegmentKind, SegmentSpec};
use alas_prop::mission_turbofan::{TurbofanInputs, VehicleBuilderParams};
use serde::Deserialize;

/// The resolved vehicle, exactly as the generator read it off the analyses.
#[derive(Debug, Deserialize)]
pub struct Vehicle {
    pub reference_area_m2: f64,
    pub maximum_lift_coefficient: Option<f64>,
    pub takeoff_mass_kg: f64,
    pub fuselage_lift_correction: f64,
    pub wings: Vec<Wing>,
    pub fuselages: Vec<Fuselage>,
    pub nacelles: Vec<Nacelle>,
    pub network_count: usize,
    pub drag_settings: Settings,
    pub turbofan: Turbofan,
    pub config_tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Wing {
    pub tag: String,
    pub mean_aerodynamic_chord_m: f64,
    pub quarter_chord_sweep_rad: f64,
    pub thickness_to_chord: f64,
    pub reference_area_m2: f64,
    pub wetted_area_m2: f64,
    pub transition_x_upper: f64,
    pub transition_x_lower: f64,
    pub aspect_ratio: f64,
    pub segment_count: usize,
}

#[derive(Debug, Deserialize)]
pub struct Fuselage {
    pub tag: String,
    pub length_m: f64,
    pub effective_diameter_m: f64,
    pub front_projected_area_m2: f64,
    pub wetted_area_m2: f64,
}

#[derive(Debug, Deserialize)]
pub struct Nacelle {
    pub tag: String,
    pub length_m: f64,
    pub diameter_m: f64,
    pub wetted_area_m2: f64,
    pub origin_count: usize,
}

/// `Fidelity_Zero`'s settings as the analysis held them.
#[derive(Debug, Deserialize)]
pub struct Settings {
    pub wing_parasite_drag_form_factor: f64,
    pub fuselage_parasite_drag_form_factor: f64,
    pub viscous_lift_dependent_drag_factor: f64,
    pub trim_drag_correction_factor: f64,
    pub drag_coefficient_increment: f64,
    pub spoiler_drag_increment: f64,
    pub lift_to_drag_adjustment: f64,
    pub oswald_efficiency_factor: Option<f64>,
    pub span_efficiency: Option<f64>,
}

/// The turbofan network, read off the built vehicle.
#[derive(Debug, Deserialize)]
pub struct Turbofan {
    pub number_of_engines: f64,
    pub bypass_ratio: f64,
    pub fan_pressure_ratio: f64,
    pub turbine_inlet_temperature_k: f64,
    pub lpc_pressure_ratio: f64,
    pub hpc_pressure_ratio: f64,
    pub inlet_pressure_ratio: f64,
    pub inlet_polytropic_efficiency: f64,
    pub inlet_pressure_recovery: f64,
    pub lpc_polytropic_efficiency: f64,
    pub hpc_polytropic_efficiency: f64,
    pub fan_polytropic_efficiency: f64,
    pub combustor_pressure_ratio: f64,
    pub combustor_efficiency: f64,
    pub turbine_mechanical_efficiency: f64,
    pub turbine_polytropic_efficiency: f64,
    pub core_nozzle_pressure_ratio: f64,
    pub core_nozzle_polytropic_efficiency: f64,
    pub fan_nozzle_pressure_ratio: f64,
    pub fan_nozzle_polytropic_efficiency: f64,
    pub design_thrust_total_n: f64,
    pub compressor_nondimensional_massflow: f64,
    pub sfc_adjustment: f64,
    pub reference_temperature_k: f64,
    pub reference_pressure_pa: f64,
}

/// The sampled vortex-lattice tables `build_surrogate` was handed.
#[derive(Debug, Deserialize)]
pub struct SurrogateTraining {
    pub angle_of_attack_rad: Vec<f64>,
    pub mach: Vec<f64>,
    pub wing_tags: Vec<String>,
    pub lift_coefficient: Vec<Vec<f64>>,
    pub drag_coefficient: Vec<Vec<f64>>,
    pub wing_lift_coefficient: BTreeMap<String, Vec<Vec<f64>>>,
    pub wing_drag_coefficient: BTreeMap<String, Vec<Vec<f64>>>,
    pub supersonic_surrogate_is_absent: bool,
    pub transonic_surrogate_is_absent: bool,
}

/// One segment's schedule, as `mission_setup` set it.
#[derive(Debug, Deserialize)]
pub struct Spec {
    pub tag: String,
    pub kind: String,
    pub air_speed_m_s: f64,
    pub true_course_rad: f64,
    pub temperature_deviation_k: f64,
    pub number_control_points: usize,
    pub tolerance_solution: f64,
    pub max_evaluations: f64,
    pub step_size: Option<f64>,
    pub altitude_m: Option<f64>,
    pub distance_m: Option<f64>,
    pub altitude_start_m: Option<f64>,
    pub altitude_end_m: Option<f64>,
    pub rate_m_s: Option<f64>,
}

impl Spec {
    /// The port's own segment specification.
    ///
    /// A `kind` the port does not translate would build a segment that flies
    /// something other than what the fixture recorded, so it is a panic here
    /// rather than a silent fallback: the generator already refuses to write a
    /// mission carrying a fourth kind, and this is the reading half of that.
    pub fn to_spec(&self) -> SegmentSpec {
        let kind = match self.kind.as_str() {
            "climb" => SegmentKind::Climb {
                altitude_start_m: self.altitude_start_m,
                altitude_end_m: self
                    .altitude_end_m
                    .expect("a climb declares an end altitude"),
                climb_rate_m_s: self.rate_m_s.expect("a climb declares a rate"),
            },
            "descent" => SegmentKind::Descent {
                altitude_start_m: self.altitude_start_m,
                altitude_end_m: self
                    .altitude_end_m
                    .expect("a descent declares an end altitude"),
                descent_rate_m_s: self.rate_m_s.expect("a descent declares a rate"),
            },
            "cruise" => SegmentKind::Cruise {
                altitude_m: self.altitude_m,
                distance_m: self.distance_m.expect("a cruise declares a distance"),
            },
            other => panic!("the fixture flies a {other} segment, which is not translated"),
        };
        SegmentSpec {
            tag: self.tag.clone(),
            kind,
            air_speed_m_s: self.air_speed_m_s,
            true_course_rad: self.true_course_rad,
            temperature_deviation_k: self.temperature_deviation_k,
            number_control_points: self.number_control_points,
        }
    }
}

/// Build the analysis stack the port evaluates against.
pub fn analyses(vehicle: &Vehicle, training: &SurrogateTraining) -> MissionAnalyses {
    let grid = TrainingGrid {
        angle_of_attack_rad: training.angle_of_attack_rad.clone(),
        mach: training.mach.clone(),
    };
    let tables = TrainingTables {
        lift_coefficient: training.lift_coefficient.clone(),
        drag_coefficient: training.drag_coefficient.clone(),
        wing_lift_coefficient: training.wing_lift_coefficient.clone(),
        wing_drag_coefficient: training.wing_drag_coefficient.clone(),
    };
    let surrogate = LiftSurrogate::from_training(&grid, &training.wing_tags, &tables)
        .expect("the recorded tables fit");

    MissionAnalyses {
        reference_area_m2: vehicle.reference_area_m2,
        maximum_lift_coefficient: vehicle.maximum_lift_coefficient,
        takeoff_mass_kg: vehicle.takeoff_mass_kg,
        minimum_mass_kg: None,
        fuselage_lift_correction: vehicle.fuselage_lift_correction,
        drag_settings: DragSettings::default(),
        wings: vehicle
            .wings
            .iter()
            .map(|wing| WingParams {
                mean_aerodynamic_chord_m: wing.mean_aerodynamic_chord_m,
                quarter_chord_sweep_rad: wing.quarter_chord_sweep_rad,
                thickness_to_chord: wing.thickness_to_chord,
                reference_area_m2: wing.reference_area_m2,
                wetted_area_m2: wing.wetted_area_m2,
                transition_x_upper: wing.transition_x_upper,
                transition_x_lower: wing.transition_x_lower,
                aspect_ratio: wing.aspect_ratio,
                // Overwritten from the surrogate at every evaluation.
                inviscid_lift_coefficient: 0.0,
                inviscid_induced_drag_coefficient: 0.0,
            })
            .collect(),
        fuselages: vehicle
            .fuselages
            .iter()
            .map(|fuselage| FuselageParams {
                length_m: fuselage.length_m,
                effective_diameter_m: fuselage.effective_diameter_m,
                front_projected_area_m2: fuselage.front_projected_area_m2,
                wetted_area_m2: fuselage.wetted_area_m2,
            })
            .collect(),
        nacelles: vehicle
            .nacelles
            .iter()
            .map(|nacelle| NacelleParams {
                length_m: nacelle.length_m,
                diameter_m: nacelle.diameter_m,
                wetted_area_m2: nacelle.wetted_area_m2,
                origin_count: nacelle.origin_count,
            })
            .collect(),
        network_count: vehicle.network_count,
        surrogate,
        turbofan: TurbofanInputs {
            number_of_engines: vehicle.turbofan.number_of_engines,
            bypass_ratio: vehicle.turbofan.bypass_ratio,
            // The port backs the high-pressure ratio out of the overall one
            // exactly as `vehicle_builder.py` does, and the fixture records
            // the network's resolved value; multiplying back is how the
            // recorded network is expressed in the port's own input, and
            // `the_engine_the_port_assumes_is_the_one_that_was_flown` is what
            // checks the two agree.
            overall_pressure_ratio: vehicle.turbofan.hpc_pressure_ratio
                * vehicle.turbofan.lpc_pressure_ratio,
            fan_pressure_ratio: vehicle.turbofan.fan_pressure_ratio,
            turbine_inlet_temperature_k: vehicle.turbofan.turbine_inlet_temperature_k,
            // Only the sizing pass reads these three, and the mission flies an
            // engine that was sized before it started.
            cruise_mach: 0.0,
            cruise_altitude_m: 0.0,
            design_thrust_total_n: vehicle.turbofan.design_thrust_total_n,
        },
        turbofan_params: VehicleBuilderParams::default(),
        compressor_nondimensional_massflow: vehicle.turbofan.compressor_nondimensional_massflow,
    }
}

/// The `mission_segments` fixture reader.
pub mod segments;
