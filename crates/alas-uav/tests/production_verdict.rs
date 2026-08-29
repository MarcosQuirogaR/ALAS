// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Test fixtures use expected failures and an evidence-complete optimizer run
// as assertions; unwraps cannot escape from library code.
#![allow(clippy::expect_used)]

//! Causal product-verdict tests for generated fixed-wing UAVs.

#[path = "support/production_problem.rs"]
mod production_problem;

use alas_uav::catalog::ComponentKind;
use alas_uav::feasibility::{FindingKind, Severity};
use alas_uav::{
    verify_coupled_production, CoupledFinding, CoupledOutcome, HorizontalTailTrimAuthority,
    LongitudinalStabilityVerdict, PitchTrimAssessment, PitchTrimUnverifiedReason,
    ProductionVerificationInputs, SharedCoreInputs,
};

#[test]
fn a_stable_evidence_complete_aircraft_with_bounded_tail_authority_is_accepted() {
    let (catalog, optimized) = production_problem::optimized_baseline();
    let outcome =
        verify_coupled_production(&optimized, &catalog, &inputs_with_authority(-15.0, 15.0))
            .expect("the production VLM must solve the evidence-complete aircraft");

    let CoupledOutcome::Accepted(audit) = outcome else {
        panic!("expected accepted coupled result, got {outcome:#?}");
    };
    assert_eq!(
        audit
            .longitudinal_stability
            .expect("the VLM derivative must be retained")
            .verdict,
        LongitudinalStabilityVerdict::Stable
    );
    let PitchTrimAssessment::Converged(trim) = audit.pitch_trim else {
        panic!("the evidenced tail range must bracket trim");
    };
    assert!(trim.pitching_moment_coefficient_residual.abs() <= 1.0e-8);
    assert!(trim.lift_margin_n >= 0.0);
}

#[test]
fn absent_pitch_control_geometry_is_unverified_instead_of_assumed() {
    let (catalog, optimized) = production_problem::optimized_baseline();
    let outcome = verify_coupled_production(&optimized, &catalog, &inputs_without_authority())
        .expect("the production VLM must still evaluate static stability");

    let CoupledOutcome::Unverified(audit) = outcome else {
        panic!("missing control authority must be unverified, got {outcome:#?}");
    };
    assert!(audit.findings.iter().any(|finding| {
        matches!(
            finding,
            CoupledFinding::PitchTrimUnverified(PitchTrimUnverifiedReason::MissingControlAuthority)
        )
    }));
}

#[test]
fn an_aft_loaded_cg_produces_a_causal_unstable_slope_rejection() {
    let (catalog, mut optimized) = production_problem::optimized_baseline();
    optimized.design.airframe.fixed_cg_x_m = optimized.geometry.fuselage.length_m;
    optimized.design.airframe.forward_cg_limit_x_m = 0.0;
    optimized.design.airframe.aft_cg_limit_x_m = 1.1 * optimized.geometry.fuselage.length_m;
    let outcome = verify_coupled_production(&optimized, &catalog, &inputs_without_authority())
        .expect("the aft-CG aircraft must remain numerically solvable");

    let CoupledOutcome::Rejected(audit) = outcome else {
        panic!("an unstable derivative must reject the aircraft, got {outcome:#?}");
    };
    assert!(audit.findings.iter().any(|finding| {
        matches!(
            finding,
            CoupledFinding::LongitudinalInstability { dcm_dcl } if *dcm_dcl > 0.0
        )
    }));
}

#[test]
fn a_tail_range_that_does_not_cross_zero_moment_remains_unverified() {
    let (catalog, optimized) = production_problem::optimized_baseline();
    let outcome =
        verify_coupled_production(&optimized, &catalog, &inputs_with_authority(30.0, 31.0))
            .expect("the endpoint VLM evaluations must complete");

    assert!(matches!(outcome, CoupledOutcome::Unverified(_)));
    assert!(outcome.audit().findings.iter().any(|finding| {
        matches!(
            finding,
            CoupledFinding::PitchTrimUnverified(PitchTrimUnverifiedReason::NoMomentBracket)
        )
    }));
}

#[test]
fn zero_moment_without_enough_lift_is_a_hard_rejection() {
    let (catalog, optimized) = production_problem::optimized_baseline();
    let mut inputs = inputs_with_authority(-15.0, 15.0);
    inputs.flight.speed_m_s = 5.0;
    let outcome = verify_coupled_production(&optimized, &catalog, &inputs)
        .expect("the low-speed trim solve must complete");

    let CoupledOutcome::Rejected(audit) = outcome else {
        panic!("insufficient trimmed lift must reject, got {outcome:#?}");
    };
    assert!(audit.findings.iter().any(|finding| {
        matches!(
            finding,
            CoupledFinding::InsufficientTrimmedLift {
                required_lift_coefficient,
                available_lift_coefficient,
            } if required_lift_coefficient > available_lift_coefficient
        )
    }));
}

#[test]
fn structural_servo_and_packaging_overloads_survive_into_one_rejection() {
    let (catalog, mut optimized) = production_problem::optimized_baseline();
    optimized.design.structural_cases[0].allowable_load_factor = Some(0.5);
    optimized.design.structural_cases[0].allowable_wing_root_bending_moment_nm = Some(0.1);
    optimized.design.servos[0].required_torque_nm = 100.0;
    optimized.design.battery.placement.center_y_m =
        2.0 * optimized.design.airframe.equipment_bay.max_y_m;
    let outcome = verify_coupled_production(&optimized, &catalog, &inputs_without_authority())
        .expect("overloads are physical findings, not solver errors");

    let CoupledOutcome::Rejected(audit) = outcome else {
        panic!("supplied overloads must reject, got {outcome:#?}");
    };
    for expected in [
        FindingKind::StructuralOverload,
        FindingKind::ServoTorqueOverload,
        FindingKind::PackagingViolation,
    ] {
        assert!(has_feasibility_finding(
            &audit.findings,
            expected,
            Severity::Failure
        ));
    }
}

#[test]
fn current_and_thrust_failures_remain_hard_product_rejections() {
    let (catalog, mut optimized) = production_problem::optimized_baseline();
    optimized.design.propulsion_demand.battery_current_a = 900.0;
    optimized.design.propulsion_demand.motor_current_a = 800.0;
    optimized.design.propulsion_demand.motor_power_w = 10_000.0;
    for condition in &mut optimized.design.flight_conditions {
        condition.available_thrust_n = Some(0.01);
    }
    let outcome = verify_coupled_production(&optimized, &catalog, &inputs_without_authority())
        .expect("component failures are verdict data, not execution failures");

    let CoupledOutcome::Rejected(audit) = outcome else {
        panic!("current and thrust overloads must reject, got {outcome:#?}");
    };
    for expected in [
        FindingKind::BatteryCurrentOverload,
        FindingKind::EscCurrentOverload,
        FindingKind::MotorCurrentOverload,
        FindingKind::MotorPowerOverload,
        FindingKind::InsufficientThrust,
    ] {
        assert!(has_feasibility_finding(
            &audit.findings,
            expected,
            Severity::Failure
        ));
    }
}

#[test]
fn a_published_landing_gear_mass_limit_is_rechecked_at_the_product_boundary() {
    let (mut catalog, optimized) = production_problem::optimized_baseline();
    let gear = catalog
        .records
        .iter_mut()
        .find(|record| record.id == optimized.components.landing_gear_id)
        .expect("the selected landing gear must be in the test catalogue");
    let ComponentKind::LandingGear(spec) = &mut gear.kind else {
        panic!("the selected record must remain landing gear");
    };
    spec.max_aircraft_mass_kg = Some(0.1);
    let outcome = verify_coupled_production(&optimized, &catalog, &inputs_without_authority())
        .expect("a landing-gear overload is verdict data");

    let CoupledOutcome::Rejected(audit) = outcome else {
        panic!("a published landing-gear overload must reject, got {outcome:#?}");
    };
    assert!(has_feasibility_finding(
        &audit.findings,
        FindingKind::LandingGearOverload,
        Severity::Failure
    ));
}

#[test]
fn missing_component_and_landing_gear_evidence_cannot_pass_retail_style_data() {
    let (mut catalog, mut optimized) = production_problem::optimized_baseline();
    optimized.design.motor.spec.mass_kg = None;
    optimized.components.propulsion_evidence.clear();
    let gear = catalog
        .records
        .iter_mut()
        .find(|record| record.id == optimized.components.landing_gear_id)
        .expect("the selected landing gear must be in the test catalogue");
    let ComponentKind::LandingGear(spec) = &mut gear.kind else {
        panic!("the selected record must remain landing gear");
    };
    spec.max_aircraft_mass_kg = None;
    let outcome = verify_coupled_production(&optimized, &catalog, &inputs_without_authority())
        .expect("missing evidence must produce a typed outcome");

    let CoupledOutcome::Unverified(audit) = outcome else {
        panic!("missing evidence cannot be accepted, got {outcome:#?}");
    };
    assert!(has_feasibility_finding(
        &audit.findings,
        FindingKind::MissingData,
        Severity::Unverified
    ));
    assert!(audit.shared_core.is_none());
}

fn inputs_without_authority() -> ProductionVerificationInputs {
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
        trim_authority: None,
    }
}

fn inputs_with_authority(minimum: f64, maximum: f64) -> ProductionVerificationInputs {
    let mut inputs = inputs_without_authority();
    inputs.trim_authority = Some(HorizontalTailTrimAuthority {
        minimum_incidence_deg: minimum,
        maximum_incidence_deg: maximum,
        evidence: "bench-qualified all-moving tail travel report HT-01".to_owned(),
    });
    inputs
}

fn has_feasibility_finding(
    findings: &[CoupledFinding],
    kind: FindingKind,
    severity: Severity,
) -> bool {
    findings.iter().any(|finding| {
        matches!(
            finding,
            CoupledFinding::Feasibility(value)
                if value.kind == kind && value.severity == severity
        )
    })
}
