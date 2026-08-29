// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A failed unwrap in this test target is the assertion reporting a broken
// physical contract, not a panic escaping from library code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Coupled fixed-wing UAV feasibility-contract tests.

use alas_uav::catalog::{
    BatterySpec, Dimensions, EscSpec, MotorSpec, PropellerSpec, ServoOperatingPoint, ServoSpec,
};
use alas_uav::feasibility::{
    evaluate, Airframe, ControlBusDemand, EquipmentBay, FindingKind, FlightCondition,
    InstalledBattery, InstalledEsc, InstalledMass, InstalledMotor, InstalledPropeller,
    InstalledServo, MissionEnergyDemand, Placement, PropulsionElectricalDemand, StructuralCase,
    UavDesign,
};

#[test]
fn a_complete_design_passes_every_coupled_constraint() {
    let design = complete_design();
    let report = evaluate(&design);
    assert!(report.verified_feasible(), "{:#?}", report.findings);
    assert!(!report.has_failure());
    assert!(!report.has_unverified_constraint());
    assert!((report.takeoff_mass_kg.expect("complete mass") - 3.93).abs() < 1.0e-12);
    assert!(
        (report
            .nominal_battery_energy_wh
            .expect("complete battery energy")
            - 74.0)
            .abs()
            < 1.0e-12
    );
    assert!(
        (report
            .mission_energy_available_wh
            .expect("complete available energy")
            - 47.36)
            .abs()
            < 1.0e-12
    );
    let cg = report
        .center_of_gravity_x_m
        .expect("complete mass has a CG");
    assert!(cg > 0.45 && cg < 0.55, "loaded CG was {cg}");
    assert_eq!(report.flight_conditions.len(), 1);
    assert!(report.flight_conditions[0].required_lift_coefficient < 1.0);
    assert!(
        report.flight_conditions[0]
            .required_thrust_n
            .expect("pre-stall point has a drag result")
            < 40.0
    );
}

#[test]
fn overloads_are_reported_by_physical_failure_mode() {
    let mut design = complete_design();
    design.propulsion_demand.battery_current_a = 120.0;
    design.propulsion_demand.motor_current_a = 90.0;
    design.propulsion_demand.motor_power_w = 900.0;
    design.mission_energy.mission_energy_wh = 60.0;
    design.servos[0].required_torque_nm = 1.2;
    design.flight_conditions[0].speed_m_s = 6.0;
    design.flight_conditions[0].available_thrust_n = Some(1.0);
    design.flight_conditions.push(FlightCondition {
        name: "thrust-limited cruise".to_owned(),
        density_kg_m3: 1.225,
        speed_m_s: 20.0,
        load_factor: 1.0,
        available_thrust_n: Some(1.0),
    });
    design.structural_cases[0].load_factor = 5.0;
    design.structural_cases[0].wing_root_bending_moment_nm = Some(150.0);

    let report = evaluate(&design);
    assert!(report.has_failure());
    for expected in [
        FindingKind::BatteryCurrentOverload,
        FindingKind::EscCurrentOverload,
        FindingKind::MotorCurrentOverload,
        FindingKind::MotorPowerOverload,
        FindingKind::ServoTorqueOverload,
        FindingKind::EnergyShortfall,
        FindingKind::InsufficientLift,
        FindingKind::InsufficientThrust,
        FindingKind::StructuralOverload,
    ] {
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.kind == expected),
            "missing {expected:?} in {:#?}",
            report.findings
        );
    }
}

#[test]
fn absent_vendor_values_never_turn_into_plausible_numbers() {
    let mut design = complete_design();
    design.motor.spec.mass_kg = None;
    design.motor.spec.max_current_a = None;
    design.servos[0].spec.operating_points[0].stall_current_a = None;
    let report = evaluate(&design);

    assert_eq!(report.takeoff_mass_kg, None);
    assert_eq!(report.center_of_gravity_x_m, None);
    assert!(report.has_unverified_constraint());
    assert!(report.findings.iter().any(|finding| {
        finding.kind == FindingKind::MissingData
            && finding.subject == "motor"
            && finding.message.contains("no value was inferred")
    }));
    assert!(report.findings.iter().any(|finding| {
        finding.kind == FindingKind::MissingData
            && finding.subject == "aileron-servo"
            && finding.message.contains("stall current")
    }));
}

#[test]
fn cell_count_cg_and_packaging_incompatibilities_are_not_hidden() {
    let mut design = complete_design();
    design.battery.spec.series_cells = Some(7);
    design.battery.placement.center_x_m = 0.98;
    design.battery.placement.center_y_m = 0.19;
    design.airframe.aft_cg_limit_x_m = 0.48;
    let report = evaluate(&design);

    assert!(report
        .findings
        .iter()
        .any(|finding| finding.kind == FindingKind::CellCountMismatch));
    assert!(report
        .findings
        .iter()
        .any(|finding| finding.kind == FindingKind::PackagingViolation));
    assert!(report
        .findings
        .iter()
        .any(|finding| finding.kind == FindingKind::CenterOfGravityViolation));
}

#[test]
fn an_empty_or_internally_inconsistent_analysis_cannot_pass_as_verified() {
    let mut design = complete_design();
    design.flight_conditions.clear();
    design.structural_cases.clear();
    design.propulsion_demand.battery_current_a = 10.0;
    design.propulsion_demand.motor_current_a = 20.0;
    let report = evaluate(&design);

    assert!(!report.verified_feasible());
    assert!(report.findings.iter().any(|finding| {
        finding.kind == FindingKind::MissingData && finding.subject == "aerodynamics"
    }));
    assert!(report.findings.iter().any(|finding| {
        finding.kind == FindingKind::MissingData && finding.subject == "structures"
    }));
    assert!(report.findings.iter().any(|finding| {
        finding.kind == FindingKind::InvalidInput
            && finding.subject == "propulsion electrical demand"
    }));
}

fn complete_design() -> UavDesign {
    let internal = |x| Placement {
        center_x_m: x,
        center_y_m: 0.0,
        center_z_m: 0.0,
        inside_equipment_bay: true,
    };
    let external = |x| Placement {
        center_x_m: x,
        center_y_m: 0.0,
        center_z_m: 0.0,
        inside_equipment_bay: false,
    };
    let small = Dimensions {
        length_m: 0.04,
        width_m: 0.03,
        height_m: 0.02,
    };
    UavDesign {
        airframe: Airframe {
            fixed_mass_kg: 3.0,
            fixed_cg_x_m: 0.5,
            wing_area_m2: 0.8,
            maximum_lift_coefficient: 1.5,
            zero_lift_drag_coefficient: 0.035,
            induced_drag_factor: 0.055,
            forward_cg_limit_x_m: 0.3,
            aft_cg_limit_x_m: 0.7,
            equipment_bay: EquipmentBay {
                min_x_m: 0.0,
                max_x_m: 1.0,
                min_y_m: -0.2,
                max_y_m: 0.2,
                min_z_m: -0.2,
                max_z_m: 0.2,
            },
        },
        battery: InstalledBattery {
            id: "battery".to_owned(),
            spec: BatterySpec {
                chemistry: "LiPo".to_owned(),
                series_cells: Some(4),
                nominal_voltage_v: Some(14.8),
                capacity_ah: Some(5.0),
                discharge_rating_c: Some(20.0),
                mass_kg: Some(0.5),
                dimensions: Some(Dimensions {
                    length_m: 0.12,
                    width_m: 0.04,
                    height_m: 0.04,
                }),
                connector: Some("XT60".to_owned()),
            },
            placement: internal(0.45),
        },
        motor: InstalledMotor {
            id: "motor".to_owned(),
            spec: MotorSpec {
                kv_rpm_per_v: Some(700.0),
                winding_resistance_ohm: Some(0.05),
                no_load_current_a: Some(1.0),
                no_load_test_voltage_v: Some(10.0),
                min_series_cells: Some(3),
                max_series_cells: Some(6),
                max_current_a: Some(60.0),
                max_power_w: Some(800.0),
                max_static_thrust_n: Some(35.0),
                mass_kg: Some(0.15),
                dimensions: Some(small),
                recommended_propeller_diameter_m: Some(12.0 * 0.0254),
                recommended_propeller_pitch_m: Some(6.0 * 0.0254),
            },
            placement: external(0.05),
        },
        esc: InstalledEsc {
            id: "esc".to_owned(),
            spec: EscSpec {
                min_series_cells: Some(3),
                max_series_cells: Some(6),
                continuous_current_a: Some(80.0),
                bec_min_voltage_v: Some(5.0),
                bec_max_voltage_v: Some(8.4),
                bec_continuous_current_a: Some(5.0),
                bec_peak_current_a: Some(10.0),
                mass_kg: Some(0.08),
                dimensions: Some(small),
            },
            placement: internal(0.4),
        },
        propeller: InstalledPropeller {
            id: "propeller".to_owned(),
            spec: PropellerSpec {
                diameter_m: Some(12.0 * 0.0254),
                pitch_m: Some(6.0 * 0.0254),
                blade_count: Some(2),
                mass_kg: Some(0.04),
                bore_diameter_m: Some(0.006),
                electric_compatible: Some(true),
            },
            placement: external(0.0),
        },
        additional_propulsors: Vec::new(),
        servos: vec![
            servo("aileron-servo", internal(0.55), small),
            servo("elevator-servo", internal(0.6), small),
        ],
        other_items: vec![InstalledMass {
            id: "autopilot".to_owned(),
            mass_kg: Some(0.1),
            dimensions: Some(small),
            placement: internal(0.5),
        }],
        propulsion_demand: PropulsionElectricalDemand {
            battery_current_a: 40.0,
            motor_current_a: 38.0,
            motor_power_w: 560.0,
        },
        control_bus: ControlBusDemand {
            voltage_v: 5.0,
            other_continuous_current_a: 0.5,
        },
        mission_energy: MissionEnergyDemand {
            mission_energy_wh: 40.0,
            maximum_depth_of_discharge: 0.8,
            reserve_fraction: 0.2,
        },
        flight_conditions: vec![FlightCondition {
            name: "cruise".to_owned(),
            density_kg_m3: 1.225,
            speed_m_s: 20.0,
            load_factor: 1.0,
            available_thrust_n: Some(40.0),
        }],
        structural_cases: vec![StructuralCase {
            name: "pull-up".to_owned(),
            load_factor: 3.5,
            allowable_load_factor: Some(4.0),
            wing_root_bending_moment_nm: Some(80.0),
            allowable_wing_root_bending_moment_nm: Some(100.0),
        }],
    }
}

fn servo(id: &str, placement: Placement, dimensions: Dimensions) -> InstalledServo {
    InstalledServo {
        id: id.to_owned(),
        spec: ServoSpec {
            mass_kg: Some(0.03),
            dimensions: Some(dimensions),
            operating_points: vec![ServoOperatingPoint {
                voltage_v: 5.0,
                stall_torque_nm: 1.0,
                seconds_per_60_deg: Some(0.1),
                stall_current_a: Some(1.0),
            }],
        },
        placement,
        required_torque_nm: 0.4,
        continuous_current_a: Some(0.2),
    }
}
