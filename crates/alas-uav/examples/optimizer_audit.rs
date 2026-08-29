// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reproducible human-readable audit of one accepted and one rejected search.

use std::io::{self, Write};

use alas_uav::catalog::Dimensions;
use alas_uav::optimizer::{
    optimize, DesignObjectives, GeometrySearchBounds, OptimizationError, OptimizationProblem,
    PreliminaryModel, PropulsionMap, PropulsionOperatingPoint, SystemsDefinition, VariableBounds,
};
use alas_uav::{seed_catalog, Catalog};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let reviewed = Catalog::from_json(include_str!("data/optimizer_audit_catalog.json"))?;
    let maps = propulsion_maps();
    let feasible = optimize(&problem(&reviewed, &maps, 1_024))?;
    let stdout = io::stdout();
    let mut output = io::BufWriter::new(stdout.lock());
    writeln!(output, "ALAS fixed-wing UAV mixed optimizer audit")?;
    writeln!(output, "case: synthetic evidence-complete design")?;
    writeln!(
        output,
        "verified_feasible: {}",
        feasible.report.verified_feasible()
    )?;
    writeln!(output, "seed: {}", feasible.seed)?;
    writeln!(output, "evaluation: {}", feasible.evaluation_index)?;
    writeln!(
        output,
        "takeoff_mass_kg: {:.6}",
        feasible.metrics.takeoff_mass_kg
    )?;
    writeln!(
        output,
        "mission_energy_wh: {:.6}",
        feasible.metrics.mission_energy_wh
    )?;
    writeln!(
        output,
        "propulsive_efficiency: {:.6}",
        feasible.metrics.propulsive_efficiency
    )?;
    writeln!(
        output,
        "wing_area_m2: {:.6}",
        feasible.geometry.wing.area_m2
    )?;
    writeln!(output, "wing_span_m: {:.6}", feasible.geometry.wing.span_m)?;
    writeln!(
        output,
        "fuselage_length_m: {:.6}",
        feasible.geometry.fuselage.length_m
    )?;
    writeln!(output, "findings: {}", feasible.report.findings.len())?;

    let retail = seed_catalog().map_err(|error| error.to_string())?;
    writeln!(output, "case: embedded retail seed without inferred data")?;
    match optimize(&problem(retail, &[], 16)) {
        Err(OptimizationError::NoFeasibleDesign(summary)) => {
            writeln!(output, "verified_feasible: false")?;
            writeln!(
                output,
                "evaluated_candidates: {}",
                summary.evaluated_candidates
            )?;
            for rejection in summary.rejections {
                writeln!(
                    output,
                    "rejection: {:?} candidates={}",
                    rejection.kind, rejection.candidates
                )?;
            }
        }
        result => return Err(format!("unexpected retail-seed result: {result:?}").into()),
    }
    Ok(())
}

fn problem<'a>(
    catalog: &'a Catalog,
    maps: &'a [PropulsionMap],
    evaluations: usize,
) -> OptimizationProblem<'a> {
    OptimizationProblem {
        catalog,
        propulsion_maps: maps,
        mission_profile: None,
        objectives: DesignObjectives {
            endurance_s: 900.0,
            range_m: 10_000.0,
            cruise_speed_m_s: 20.0,
            maximum_stall_speed_m_s: 10.0,
            payload_mass_kg: 0.5,
            payload_dimensions: dimensions(0.25, 0.12, 0.10),
            minimum_propulsive_efficiency: 0.15,
            efficiency_priority: 0.5,
        },
        geometry_bounds: GeometrySearchBounds {
            wing_area_m2: bounds(0.9, 1.2),
            wing_aspect_ratio: bounds(8.0, 10.0),
            fuselage_length_m: bounds(1.8, 2.0),
            wing_leading_edge_fraction: bounds(0.25, 0.45),
        },
        model: PreliminaryModel {
            air_density_kg_m3: 1.225,
            maximum_lift_coefficient: 1.6,
            zero_lift_drag_coefficient: 0.035,
            oswald_efficiency: 0.8,
            limit_load_factor: 3.0,
            structural_safety_factor: 1.5,
            horizontal_tail_volume_coefficient: 0.5,
            vertical_tail_volume_coefficient: 0.04,
            horizontal_tail_aspect_ratio: 4.0,
            vertical_tail_aspect_ratio: 1.6,
            forward_cg_chord_fraction: 0.15,
            aft_cg_chord_fraction: 0.35,
            equipment_clearance_m: 0.015,
            equipment_gap_m: 0.015,
            nose_length_fraction: 0.10,
            tailcone_length_fraction: 0.30,
            spar_cap_width_fraction: 0.06,
            spar_cap_separation_fraction: 0.14,
            aileron_area_fraction: 0.08,
            aileron_chord_fraction: 0.25,
            elevator_area_fraction: 0.30,
            elevator_chord_fraction: 0.30,
            hinge_moment_coefficient: 0.01,
            landing_gear_track_fraction: 0.18,
            landing_gear_wheelbase_fraction: 0.35,
            propeller_ground_clearance_m: 0.05,
            fixed_systems_mass_kg: 0.15,
        },
        systems: SystemsDefinition {
            avionics_mass_kg: 0.10,
            avionics_dimensions: dimensions(0.08, 0.06, 0.03),
            avionics_power_w: 25.0,
            control_bus_current_a: 0.8,
            control_bus_voltage_v: 6.0,
            minimum_receiver_channels: 6,
            servo_continuous_current_fraction: 0.2,
            maximum_depth_of_discharge: 0.8,
            reserve_fraction: 0.2,
        },
        required_electronics_role: "gps_sensor",
        seed: 0x5eed_cafe,
        evaluations,
    }
}

fn propulsion_maps() -> Vec<PropulsionMap> {
    vec![PropulsionMap {
        motor_id: "motor-reviewed".to_owned(),
        propeller_id: "propeller-reviewed".to_owned(),
        series_cells: 6,
        motor_count: 1,
        evidence: "independent in-flight dynamometer map R1".to_owned(),
        points: vec![
            point(1.0, 50.0, 25.0, 500.0),
            point(25.0, 25.0, 35.0, 750.0),
        ],
    }]
}

fn point(speed: f64, thrust: f64, current: f64, power: f64) -> PropulsionOperatingPoint {
    PropulsionOperatingPoint {
        speed_m_s: speed,
        thrust_n: thrust,
        motor_current_a: current,
        motor_power_w: power,
    }
}

fn dimensions(length: f64, width: f64, height: f64) -> Dimensions {
    Dimensions {
        length_m: length,
        width_m: width,
        height_m: height,
    }
}

fn bounds(minimum: f64, maximum: f64) -> VariableBounds {
    VariableBounds { minimum, maximum }
}
