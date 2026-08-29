// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Characterization of generated UAV geometry through the production core.

// Fixture construction and expected failure extraction are test-authoring invariants.
#![allow(clippy::expect_used)]

use alas_uav::optimizer::{
    EmpennageGeometry, FuselageGeometry, GeneratedGeometry, LandingGearGeometry, WingGeometry,
};
use alas_uav::{
    assess_generated_geometry_with_shared_core, build_airplane, SharedCoreFailure,
    SharedCoreInputs, SharedCoreLiftVerdict,
};

#[test]
fn generated_geometry_preserves_reference_dimensions_in_airplane_primitives() {
    let geometry = probe_geometry();
    let airplane = build_airplane(geometry, 0.72, "naca2412", "naca0012")
        .expect("probe airfoils and geometry are valid");

    assert_eq!(airplane.wings.len(), 3);
    assert_eq!(airplane.fuselages.len(), 1);
    assert_eq!(airplane.s_ref, geometry.wing.area_m2);
    assert_eq!(airplane.b_ref, geometry.wing.span_m);
    assert_eq!(airplane.c_ref, geometry.wing.mean_chord_m);
    assert_eq!(airplane.xyz_ref, [0.72, 0.0, 0.0]);
    assert_eq!(airplane.wings[0].xsecs[1].xyz_le[1], 1.4);
    assert_eq!(airplane.wings[1].xsecs[1].xyz_le[1], 0.55);
    assert_eq!(airplane.wings[2].xsecs[1].xyz_le[2], 0.42);
}

#[test]
fn generated_geometry_reaches_native_vlm_and_reports_model_discrepancy() {
    let assessment = assess_generated_geometry_with_shared_core(
        probe_geometry(),
        5.4,
        0.72,
        0.045,
        0.035,
        &SharedCoreInputs {
            main_airfoil_name: "naca2412".to_owned(),
            tail_airfoil_name: "naca0012".to_owned(),
            altitude_m: 0.0,
            speed_m_s: 20.0,
            angle_of_attack_deg: 4.0,
            spanwise_resolution: 4,
            chordwise_resolution: 3,
        },
    )
    .expect("the generated aircraft has a nonsingular production VLM mesh");

    assert!(assessment.vlm.cl_lift.is_finite());
    assert!(assessment.vlm.cd_drag.is_finite());
    assert!(assessment.required_lift_coefficient.is_finite());
    assert!(assessment.lift_margin_n.is_finite());
    assert!(assessment.induced_drag_coefficient_delta.is_finite());
    assert_eq!(assessment.preliminary_zero_lift_drag_coefficient, 0.035);
    assert_eq!(assessment.airplane.name, "Generated Fixed-Wing UAV");
    assert_eq!(assessment.lift_verdict(), SharedCoreLiftVerdict::Passed);
}

#[test]
fn completed_vlm_with_negative_lift_is_not_a_product_lift_pass() {
    let assessment = assess_generated_geometry_with_shared_core(
        probe_geometry(),
        5.4,
        0.72,
        0.045,
        0.035,
        &SharedCoreInputs {
            main_airfoil_name: "naca2412".to_owned(),
            tail_airfoil_name: "naca0012".to_owned(),
            altitude_m: 0.0,
            speed_m_s: 20.0,
            angle_of_attack_deg: -4.0,
            spanwise_resolution: 4,
            chordwise_resolution: 3,
        },
    )
    .expect("negative angle still produces a valid VLM solve");

    assert!(assessment.lift_margin_n < 0.0);
    assert_eq!(
        assessment.lift_verdict(),
        SharedCoreLiftVerdict::InsufficientLift
    );
}

#[test]
fn unknown_airfoils_are_explicit_failures_instead_of_silent_substitutions() {
    let error = build_airplane(probe_geometry(), 0.72, "not-a-real-airfoil", "naca0012")
        .expect_err("an unknown airfoil must not acquire a default section");
    assert!(
        matches!(error, SharedCoreFailure::InvalidInput(message) if message.contains("not available"))
    );
}

fn probe_geometry() -> GeneratedGeometry {
    GeneratedGeometry {
        wing: WingGeometry {
            area_m2: 0.98,
            aspect_ratio: 8.0,
            span_m: 2.8,
            mean_chord_m: 0.35,
            leading_edge_x_m: 0.54,
        },
        fuselage: FuselageGeometry {
            length_m: 1.9,
            diameter_m: 0.18,
            equipment_bay: alas_uav::feasibility::EquipmentBay {
                min_x_m: 0.2,
                max_x_m: 1.3,
                min_y_m: -0.06,
                max_y_m: 0.06,
                min_z_m: -0.05,
                max_z_m: 0.05,
            },
        },
        empennage: EmpennageGeometry {
            horizontal_area_m2: 0.22,
            horizontal_span_m: 1.1,
            vertical_area_m2: 0.105,
            vertical_span_m: 0.42,
            tail_arm_m: 1.08,
        },
        landing_gear: LandingGearGeometry {
            track_m: 0.5,
            wheelbase_m: 0.65,
            minimum_leg_length_m: 0.24,
            design_load_factor: 3.0,
        },
    }
}
