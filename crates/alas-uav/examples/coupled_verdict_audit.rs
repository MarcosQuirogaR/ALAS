// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reproducible JSON audit of the coupled fixed-wing UAV product verdict.

use std::io::{self, Write};

use alas_uav::catalog::Dimensions;
use alas_uav::optimizer::{
    optimize, DesignObjectives, GeometrySearchBounds, OptimizationProblem, PreliminaryModel,
    PropulsionMap, PropulsionOperatingPoint, SystemsDefinition, VariableBounds,
};
use alas_uav::{
    verify_coupled_production, Catalog, CoupledOutcome, HorizontalTailTrimAuthority,
    PitchTrimAssessment, ProductionVerificationInputs, SharedCoreInputs,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let catalog = Catalog::from_json(include_str!("data/optimizer_audit_catalog.json"))?;
    let maps = propulsion_maps();
    let optimized = optimize(&problem(&catalog, &maps))?;
    let no_authority = verify_coupled_production(&optimized, &catalog, &verification_inputs(None))?;
    let authority = HorizontalTailTrimAuthority {
        minimum_incidence_deg: -15.0,
        maximum_incidence_deg: 15.0,
        evidence: "bench-qualified all-moving tail travel report HT-01".to_owned(),
    };
    let bounded =
        verify_coupled_production(&optimized, &catalog, &verification_inputs(Some(authority)))?;
    let audit = bounded.audit();
    let stability = audit
        .longitudinal_stability
        .as_ref()
        .ok_or("the evidence-complete case did not produce a stability derivative")?;
    let PitchTrimAssessment::Converged(trim) = &audit.pitch_trim else {
        return Err("the evidenced incidence range did not converge to trim".into());
    };

    let stdout = io::stdout();
    let mut output = io::BufWriter::new(stdout.lock());
    writeln!(output, "{{")?;
    writeln!(output, "  \"schema_version\": 1,")?;
    writeln!(output, "  \"seed\": {},", optimized.seed)?;
    writeln!(
        output,
        "  \"evaluation_index\": {},",
        optimized.evaluation_index
    )?;
    writeln!(
        output,
        "  \"without_control_authority\": \"{}\",",
        outcome_name(&no_authority)
    )?;
    writeln!(
        output,
        "  \"with_evidenced_tail_authority\": \"{}\",",
        outcome_name(&bounded)
    )?;
    writeln!(
        output,
        "  \"loaded_cg_x_m\": {:.12},",
        stability.center_of_gravity_x_m
    )?;
    writeln!(
        output,
        "  \"lower_alpha_deg\": {:.12},",
        stability.lower_alpha_deg
    )?;
    writeln!(
        output,
        "  \"upper_alpha_deg\": {:.12},",
        stability.upper_alpha_deg
    )?;
    writeln!(
        output,
        "  \"dcm_dcl\": {:.12},",
        stability
            .dcm_dcl
            .ok_or("stability derivative was indeterminate")?
    )?;
    writeln!(
        output,
        "  \"trim_incidence_deg\": {:.12},",
        trim.incidence_deg
    )?;
    writeln!(
        output,
        "  \"trim_cm_residual\": {:.6e},",
        trim.pitching_moment_coefficient_residual
    )?;
    writeln!(
        output,
        "  \"required_lift_coefficient\": {:.12},",
        trim.required_lift_coefficient
    )?;
    writeln!(
        output,
        "  \"trimmed_lift_coefficient\": {:.12},",
        trim.trimmed_lift_coefficient
    )?;
    writeln!(
        output,
        "  \"trimmed_lift_margin_n\": {:.12},",
        trim.lift_margin_n
    )?;
    writeln!(
        output,
        "  \"authority_evidence\": \"{}\"",
        trim.authority_evidence
    )?;
    writeln!(output, "}}")?;
    Ok(())
}

fn outcome_name(outcome: &CoupledOutcome) -> &'static str {
    match outcome {
        CoupledOutcome::Accepted(_) => "accepted",
        CoupledOutcome::Rejected(_) => "rejected",
        CoupledOutcome::Unverified(_) => "unverified",
    }
}

fn verification_inputs(
    trim_authority: Option<HorizontalTailTrimAuthority>,
) -> ProductionVerificationInputs {
    ProductionVerificationInputs {
        flight: SharedCoreInputs {
            main_airfoil_name: "naca2412".to_owned(),
            tail_airfoil_name: "naca0012".to_owned(),
            altitude_m: 0.0,
            speed_m_s: 20.0,
            angle_of_attack_deg: 4.0,
            spanwise_resolution: 3,
            chordwise_resolution: 3,
        },
        stability_probe_delta_deg: 0.5,
        trim_authority,
    }
}

fn problem<'a>(catalog: &'a Catalog, maps: &'a [PropulsionMap]) -> OptimizationProblem<'a> {
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
        evaluations: 256,
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
