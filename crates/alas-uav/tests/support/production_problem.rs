// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Evidence-complete deterministic optimizer input shared by product tests.

use std::sync::OnceLock;

use alas_uav::catalog::Dimensions;
use alas_uav::optimizer::{
    optimize, DesignObjectives, GeometrySearchBounds, OptimizationProblem, OptimizedUav,
    PreliminaryModel, PropulsionMap, PropulsionOperatingPoint, SystemsDefinition, VariableBounds,
};
use alas_uav::Catalog;

pub fn optimized_baseline() -> (Catalog, OptimizedUav) {
    static BASELINE: OnceLock<(Catalog, OptimizedUav)> = OnceLock::new();
    BASELINE.get_or_init(build_optimized_baseline).clone()
}

fn build_optimized_baseline() -> (Catalog, OptimizedUav) {
    let catalog = Catalog::from_json(include_str!(
        "../../examples/data/optimizer_audit_catalog.json"
    ))
    .expect("the reviewed synthetic catalogue must parse");
    let maps = propulsion_maps();
    let optimized = optimize(&problem(&catalog, &maps, 256))
        .expect("the evidence-complete product test problem must optimize");
    (catalog, optimized)
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

fn point(speed_m_s: f64, thrust_n: f64, current_a: f64, power_w: f64) -> PropulsionOperatingPoint {
    PropulsionOperatingPoint {
        speed_m_s,
        thrust_n,
        motor_current_a: current_a,
        motor_power_w: power_w,
    }
}

fn dimensions(length_m: f64, width_m: f64, height_m: f64) -> Dimensions {
    Dimensions {
        length_m,
        width_m,
        height_m,
    }
}

fn bounds(minimum: f64, maximum: f64) -> VariableBounds {
    VariableBounds { minimum, maximum }
}
