// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Coupled feasibility checks for a fixed-wing electric UAV.
//!
//! The aerodynamic equations are the steady point-mass relations in John D.
//! Anderson, *Aircraft Performance and Design*, McGraw-Hill, 1999: lift is
//! `q S C_L`, and the preliminary drag polar is `C_D0 + k C_L^2`. Standard
//! gravity is the exact conventional value from the BIPM SI Brochure, 9th ed.
//! Component ratings are not aerodynamic models: published static thrust is
//! never treated as thrust available in flight. Every flight point therefore
//! carries thrust from an independently evaluated propeller operating point.

use crate::catalog::{EscSpec, MotorSpec};
use crate::packaging::{check_packaging, collect_mass_terms, total_mass};

/// Conventional standard acceleration of gravity, in metres per second squared.
pub const STANDARD_GRAVITY_M_S2: f64 = 9.806_65;

pub use crate::model::*;

/// Evaluate all coupled constraints for a selected fixed-wing UAV.
pub fn evaluate(design: &UavDesign) -> UavReport {
    let mut findings = Vec::new();
    validate_airframe(&design.airframe, &mut findings);
    validate_demands(design, &mut findings);
    check_component_compatibility(design, &mut findings);
    check_packaging(design, &mut findings);

    let (mass_terms, all_masses_known) = collect_mass_terms(design, &mut findings);
    let takeoff_mass_kg = all_masses_known.then(|| total_mass(&mass_terms)).flatten();
    let center_of_gravity_x_m = takeoff_mass_kg.map(|mass| {
        mass_terms
            .iter()
            .map(|term| term.mass_kg * term.x_m)
            .sum::<f64>()
            / mass
    });
    if let Some(cg) = center_of_gravity_x_m {
        if cg < design.airframe.forward_cg_limit_x_m || cg > design.airframe.aft_cg_limit_x_m {
            findings.push(failure(
                FindingKind::CenterOfGravityViolation,
                "loaded aircraft",
                format!(
                    "loaded CG {cg:.4} m is outside [{:.4}, {:.4}] m",
                    design.airframe.forward_cg_limit_x_m, design.airframe.aft_cg_limit_x_m
                ),
                Some(cg),
                None,
                Some("m"),
            ));
        }
    }

    let raw_nominal_battery_energy_wh = design.battery.spec.nominal_energy_wh();
    if let Some(energy) = raw_nominal_battery_energy_wh {
        if !positive(energy) {
            findings.push(invalid("nominal battery energy", energy));
        }
    }
    let nominal_battery_energy_wh =
        raw_nominal_battery_energy_wh.filter(|energy| positive(*energy));
    let valid_energy_policy = (0.0..=1.0)
        .contains(&design.mission_energy.maximum_depth_of_discharge)
        && (0.0..=1.0).contains(&design.mission_energy.reserve_fraction);
    let mission_energy_available_wh = nominal_battery_energy_wh.and_then(|energy| {
        (valid_energy_policy && positive(energy)).then_some(
            energy
                * design.mission_energy.maximum_depth_of_discharge
                * (1.0 - design.mission_energy.reserve_fraction),
        )
    });
    check_energy(design, mission_energy_available_wh, &mut findings);
    let flight_conditions = check_flight(design, takeoff_mass_kg, &mut findings);
    check_structure(design, &mut findings);

    UavReport {
        takeoff_mass_kg,
        center_of_gravity_x_m,
        nominal_battery_energy_wh,
        mission_energy_available_wh,
        flight_conditions,
        findings,
    }
}

fn validate_airframe(airframe: &Airframe, findings: &mut Vec<Finding>) {
    for (subject, value) in [
        ("fixed airframe mass", airframe.fixed_mass_kg),
        ("wing area", airframe.wing_area_m2),
        (
            "maximum lift coefficient",
            airframe.maximum_lift_coefficient,
        ),
        (
            "zero-lift drag coefficient",
            airframe.zero_lift_drag_coefficient,
        ),
        ("induced-drag factor", airframe.induced_drag_factor),
    ] {
        if !positive(value) {
            findings.push(invalid(subject, value));
        }
    }
    if !airframe.fixed_cg_x_m.is_finite()
        || !airframe.forward_cg_limit_x_m.is_finite()
        || !airframe.aft_cg_limit_x_m.is_finite()
        || airframe.forward_cg_limit_x_m >= airframe.aft_cg_limit_x_m
    {
        findings.push(invalid("airframe CG definition", airframe.fixed_cg_x_m));
    }
    if !valid_bay(airframe.equipment_bay) {
        findings.push(invalid("equipment bay", airframe.equipment_bay.min_x_m));
    }
}

fn validate_demands(design: &UavDesign, findings: &mut Vec<Finding>) {
    for (subject, value) in [
        (
            "battery current demand",
            design.propulsion_demand.battery_current_a,
        ),
        (
            "motor current demand",
            design.propulsion_demand.motor_current_a,
        ),
        ("motor power demand", design.propulsion_demand.motor_power_w),
        ("control bus voltage", design.control_bus.voltage_v),
        (
            "non-servo control bus current",
            design.control_bus.other_continuous_current_a,
        ),
        ("mission energy", design.mission_energy.mission_energy_wh),
    ] {
        if !nonnegative(value) || (subject == "control bus voltage" && value == 0.0) {
            findings.push(invalid(subject, value));
        }
    }
    for (subject, fraction) in [
        (
            "maximum depth of discharge",
            design.mission_energy.maximum_depth_of_discharge,
        ),
        ("reserve fraction", design.mission_energy.reserve_fraction),
    ] {
        if !fraction.is_finite() || !(0.0..=1.0).contains(&fraction) {
            findings.push(invalid(subject, fraction));
        }
    }
    let installed_propulsors = design.propulsor_count() as f64;
    let minimum_propulsion_current_a =
        design.propulsion_demand.motor_current_a * installed_propulsors;
    if design.propulsion_demand.battery_current_a < minimum_propulsion_current_a {
        findings.push(failure(
            FindingKind::InvalidInput,
            "propulsion electrical demand",
            "total battery current cannot be lower than the sum of installed motor currents"
                .to_owned(),
            Some(minimum_propulsion_current_a),
            Some(design.propulsion_demand.battery_current_a),
            Some("A"),
        ));
    }
    if design.servos.is_empty() {
        missing(
            "control system",
            "at least one installed actuator",
            findings,
        );
    }
    for servo in &design.servos {
        if !nonnegative(servo.required_torque_nm) {
            findings.push(invalid(
                &format!("{} torque demand", servo.id),
                servo.required_torque_nm,
            ));
        }
        if let Some(current) = servo.continuous_current_a {
            if !nonnegative(current) {
                findings.push(invalid(
                    &format!("{} continuous current", servo.id),
                    current,
                ));
            }
        }
    }
    if design.flight_conditions.is_empty() {
        missing("aerodynamics", "at least one flight condition", findings);
    }
    if design.structural_cases.is_empty() {
        missing("structures", "at least one evaluated load case", findings);
    }
}

fn check_component_compatibility(design: &UavDesign, findings: &mut Vec<Finding>) {
    let battery = &design.battery.spec;
    let cells = require(
        battery.series_cells.map(f64::from),
        &design.battery.id,
        "series cell count",
        findings,
    )
    .map(|value| value as u16);
    if let Some(cells) = cells {
        for (motor, esc, _) in design.propulsors() {
            check_cell_range(&motor.id, cells, &motor.spec, findings);
            check_esc_cell_range(&esc.id, cells, &esc.spec, findings);
        }
    }

    check_upper_rating(
        FindingKind::BatteryCurrentOverload,
        &design.battery.id,
        design.propulsion_demand.battery_current_a,
        battery.rated_discharge_current_a(),
        "rated discharge current",
        "A",
        findings,
    );
    for (motor, esc, propeller) in design.propulsors() {
        check_upper_rating(
            FindingKind::EscCurrentOverload,
            &esc.id,
            design.propulsion_demand.motor_current_a,
            esc.spec.continuous_current_a,
            "continuous current rating",
            "A",
            findings,
        );
        check_upper_rating(
            FindingKind::MotorCurrentOverload,
            &motor.id,
            design.propulsion_demand.motor_current_a,
            motor.spec.max_current_a,
            "maximum current rating",
            "A",
            findings,
        );
        check_upper_rating(
            FindingKind::MotorPowerOverload,
            &motor.id,
            design.propulsion_demand.motor_power_w,
            motor.spec.max_power_w,
            "maximum power rating",
            "W",
            findings,
        );
        check_propeller_recommendation(motor, propeller, findings);
    }
    check_control_bus(design, findings);
}

fn check_cell_range(id: &str, cells: u16, motor: &MotorSpec, findings: &mut Vec<Finding>) {
    match (motor.min_series_cells, motor.max_series_cells) {
        (Some(minimum), Some(maximum)) if cells < minimum || cells > maximum => {
            findings.push(failure(
                FindingKind::CellCountMismatch,
                id,
                format!("{cells}S battery is outside motor range {minimum}S-{maximum}S"),
                Some(f64::from(cells)),
                None,
                Some("cells"),
            ))
        }
        (Some(_), Some(_)) => {}
        _ => missing(id, "supported battery cell-count range", findings),
    }
}

fn check_esc_cell_range(id: &str, cells: u16, esc: &EscSpec, findings: &mut Vec<Finding>) {
    match (esc.min_series_cells, esc.max_series_cells) {
        (Some(minimum), Some(maximum)) if cells < minimum || cells > maximum => {
            findings.push(failure(
                FindingKind::CellCountMismatch,
                id,
                format!("{cells}S battery is outside ESC range {minimum}S-{maximum}S"),
                Some(f64::from(cells)),
                None,
                Some("cells"),
            ))
        }
        (Some(_), Some(_)) => {}
        _ => missing(id, "supported battery cell-count range", findings),
    }
}

fn check_propeller_recommendation(
    motor_installation: &InstalledMotor,
    propeller_installation: &InstalledPropeller,
    findings: &mut Vec<Finding>,
) {
    let motor = &motor_installation.spec;
    let propeller = &propeller_installation.spec;
    match (
        motor.recommended_propeller_diameter_m,
        motor.recommended_propeller_pitch_m,
        propeller.diameter_m,
        propeller.pitch_m,
    ) {
        (Some(rec_d), Some(rec_p), Some(actual_d), Some(actual_p))
            if (rec_d - actual_d).abs() > 1.0e-6 || (rec_p - actual_p).abs() > 1.0e-6 =>
        {
            findings.push(unverified(
                FindingKind::PropellerCompatibilityUnverified,
                &propeller_installation.id,
                format!(
                    "selected {:.1}x{:.1} in propeller differs from motor recommendation {:.1}x{:.1} in; an operating map is required",
                    actual_d / 0.0254,
                    actual_p / 0.0254,
                    rec_d / 0.0254,
                    rec_p / 0.0254
                ),
            ));
        }
        (Some(_), Some(_), Some(_), Some(_)) => {}
        _ => missing(
            &propeller_installation.id,
            "motor/propeller recommendation or performance map",
            findings,
        ),
    }
    if propeller.electric_compatible == Some(false) {
        findings.push(failure(
            FindingKind::PropellerCompatibilityUnverified,
            &propeller_installation.id,
            "propeller source excludes electric propulsion".to_owned(),
            None,
            None,
            None,
        ));
    } else if propeller.electric_compatible.is_none() {
        missing(
            &propeller_installation.id,
            "electric-propulsion compatibility",
            findings,
        );
    }
}

fn check_control_bus(design: &UavDesign, findings: &mut Vec<Finding>) {
    let voltage = design.control_bus.voltage_v;
    match (
        design.esc.spec.bec_min_voltage_v,
        design.esc.spec.bec_max_voltage_v,
    ) {
        (Some(minimum), Some(maximum)) if voltage < minimum || voltage > maximum => {
            findings.push(failure(
                FindingKind::ServoVoltageMismatch,
                &design.esc.id,
                format!("BEC voltage {voltage:.2} V is outside {minimum:.2}-{maximum:.2} V"),
                Some(voltage),
                None,
                Some("V"),
            ))
        }
        (Some(_), Some(_)) => {}
        _ => missing(&design.esc.id, "BEC voltage range", findings),
    }

    let mut continuous_current = design.control_bus.other_continuous_current_a;
    let mut peak_current = design.control_bus.other_continuous_current_a;
    let mut all_continuous_known = true;
    let mut all_peak_known = true;
    for servo in &design.servos {
        if servo.spec.operating_points.is_empty() {
            missing(
                &servo.id,
                "published servo voltage and torque points",
                findings,
            );
        } else {
            match servo.spec.stall_torque_at_voltage(voltage) {
                Some(available) if servo.required_torque_nm > available => findings.push(failure(
                    FindingKind::ServoTorqueOverload,
                    &servo.id,
                    "required hinge torque exceeds interpolated published stall torque".to_owned(),
                    Some(servo.required_torque_nm),
                    Some(available),
                    Some("N m"),
                )),
                Some(_) => {}
                None => findings.push(failure(
                    FindingKind::ServoVoltageMismatch,
                    &servo.id,
                    "control-bus voltage lies outside published servo points".to_owned(),
                    Some(voltage),
                    None,
                    Some("V"),
                )),
            }
        }
        if let Some(current) = servo.continuous_current_a {
            continuous_current += current;
        } else {
            all_continuous_known = false;
            missing(
                &servo.id,
                "continuous current at the mission load",
                findings,
            );
        }
        if let Some(current) = servo.spec.stall_current_at_voltage(voltage) {
            peak_current += current;
        } else {
            all_peak_known = false;
            missing(&servo.id, "stall current at the selected voltage", findings);
        }
    }
    if all_continuous_known {
        check_upper_rating(
            FindingKind::BecCurrentOverload,
            &design.esc.id,
            continuous_current,
            design.esc.spec.bec_continuous_current_a,
            "BEC continuous current rating",
            "A",
            findings,
        );
    }
    if all_peak_known {
        check_upper_rating(
            FindingKind::BecCurrentOverload,
            &design.esc.id,
            peak_current,
            design.esc.spec.bec_peak_current_a,
            "BEC peak current rating",
            "A",
            findings,
        );
    }
}

fn check_energy(design: &UavDesign, available: Option<f64>, findings: &mut Vec<Finding>) {
    if !(0.0..=1.0).contains(&design.mission_energy.maximum_depth_of_discharge)
        || !(0.0..=1.0).contains(&design.mission_energy.reserve_fraction)
    {
        return;
    }
    match available {
        Some(available) if design.mission_energy.mission_energy_wh > available => findings.push(
            failure(
                FindingKind::EnergyShortfall,
                "mission energy",
                "mission consumption exceeds battery energy after depth-of-discharge and reserve limits".to_owned(),
                Some(design.mission_energy.mission_energy_wh),
                Some(available),
                Some("Wh"),
            ),
        ),
        Some(_) => {}
        None => missing(
            &design.battery.id,
            "nominal voltage and capacity required for energy",
            findings,
        ),
    }
}

fn check_flight(
    design: &UavDesign,
    takeoff_mass_kg: Option<f64>,
    findings: &mut Vec<Finding>,
) -> Vec<FlightConditionResult> {
    let Some(mass_kg) = takeoff_mass_kg else {
        missing("flight conditions", "complete takeoff mass", findings);
        return Vec::new();
    };
    if !positive(design.airframe.wing_area_m2)
        || !positive(design.airframe.maximum_lift_coefficient)
        || !positive(design.airframe.zero_lift_drag_coefficient)
        || !positive(design.airframe.induced_drag_factor)
    {
        return Vec::new();
    }
    let weight_n = mass_kg * STANDARD_GRAVITY_M_S2;
    let mut results = Vec::new();
    for condition in &design.flight_conditions {
        if !positive(condition.density_kg_m3)
            || !positive(condition.speed_m_s)
            || !positive(condition.load_factor)
        {
            findings.push(invalid(&condition.name, condition.speed_m_s));
            continue;
        }
        let dynamic_pressure_pa =
            0.5 * condition.density_kg_m3 * condition.speed_m_s * condition.speed_m_s;
        let required_lift_coefficient =
            condition.load_factor * weight_n / (dynamic_pressure_pa * design.airframe.wing_area_m2);
        let lift_feasible = required_lift_coefficient <= design.airframe.maximum_lift_coefficient;
        if !lift_feasible {
            findings.push(failure(
                FindingKind::InsufficientLift,
                &condition.name,
                "required lift coefficient exceeds the configured maximum".to_owned(),
                Some(required_lift_coefficient),
                Some(design.airframe.maximum_lift_coefficient),
                Some("C_L"),
            ));
        }
        let required_thrust_n = lift_feasible.then(|| {
            let drag_coefficient = design.airframe.zero_lift_drag_coefficient
                + design.airframe.induced_drag_factor
                    * required_lift_coefficient
                    * required_lift_coefficient;
            dynamic_pressure_pa * design.airframe.wing_area_m2 * drag_coefficient
        });
        if let Some(required_thrust_n) = required_thrust_n {
            match condition.available_thrust_n {
                Some(available) if positive(available) && required_thrust_n > available => {
                    findings.push(failure(
                        FindingKind::InsufficientThrust,
                        &condition.name,
                        "drag exceeds thrust available at this speed".to_owned(),
                        Some(required_thrust_n),
                        Some(available),
                        Some("N"),
                    ));
                }
                Some(available) if !positive(available) => findings.push(invalid(
                    &format!("{} available thrust", condition.name),
                    available,
                )),
                Some(_) => {}
                None => missing(
                    &condition.name,
                    "thrust available at flight speed",
                    findings,
                ),
            }
        }
        results.push(FlightConditionResult {
            name: condition.name.clone(),
            required_lift_coefficient,
            required_thrust_n,
        });
    }
    results
}

fn check_structure(design: &UavDesign, findings: &mut Vec<Finding>) {
    for case in &design.structural_cases {
        if !positive(case.load_factor) {
            findings.push(invalid(&case.name, case.load_factor));
            continue;
        }
        match case.allowable_load_factor {
            Some(allowable) if positive(allowable) && case.load_factor > allowable => findings
                .push(failure(
                    FindingKind::StructuralOverload,
                    &case.name,
                    "load factor exceeds evaluated structural allowable".to_owned(),
                    Some(case.load_factor),
                    Some(allowable),
                    Some("g"),
                )),
            Some(allowable) if !positive(allowable) => {
                findings.push(invalid(&case.name, allowable));
            }
            Some(_) => {}
            None => missing(&case.name, "allowable load factor", findings),
        }
        match (
            case.wing_root_bending_moment_nm,
            case.allowable_wing_root_bending_moment_nm,
        ) {
            (Some(demand), Some(allowable))
                if nonnegative(demand) && positive(allowable) && demand > allowable =>
            {
                findings.push(failure(
                    FindingKind::StructuralOverload,
                    &case.name,
                    "wing-root bending moment exceeds evaluated allowable".to_owned(),
                    Some(demand),
                    Some(allowable),
                    Some("N m"),
                ));
            }
            (Some(demand), Some(allowable)) if !nonnegative(demand) || !positive(allowable) => {
                findings.push(invalid(&case.name, demand));
            }
            (Some(_), Some(_)) => {}
            _ => missing(
                &case.name,
                "wing-root bending-moment demand and allowable",
                findings,
            ),
        }
    }
}

fn check_upper_rating(
    kind: FindingKind,
    subject: &str,
    demand: f64,
    rating: Option<f64>,
    field: &str,
    units: &'static str,
    findings: &mut Vec<Finding>,
) {
    match rating {
        Some(rating) if positive(rating) && demand > rating => findings.push(failure(
            kind,
            subject,
            format!("demand exceeds {field}"),
            Some(demand),
            Some(rating),
            Some(units),
        )),
        Some(rating) if !positive(rating) => findings.push(invalid(subject, rating)),
        Some(_) => {}
        None => missing(subject, field, findings),
    }
}

fn require(
    value: Option<f64>,
    subject: &str,
    field: &str,
    findings: &mut Vec<Finding>,
) -> Option<f64> {
    if value.is_none() {
        missing(subject, field, findings);
    }
    value
}

pub(crate) fn missing(subject: &str, field: &str, findings: &mut Vec<Finding>) {
    findings.push(unverified(
        FindingKind::MissingData,
        subject,
        format!("missing {field}; no value was inferred"),
    ));
}

pub(crate) fn invalid(subject: &str, value: f64) -> Finding {
    failure(
        FindingKind::InvalidInput,
        subject,
        format!("value {value} is not physically valid"),
        Some(value),
        None,
        None,
    )
}

pub(crate) fn failure(
    kind: FindingKind,
    subject: impl Into<String>,
    message: String,
    required: Option<f64>,
    available: Option<f64>,
    units: Option<&'static str>,
) -> Finding {
    Finding {
        severity: Severity::Failure,
        kind,
        subject: subject.into(),
        message,
        required,
        available,
        units,
    }
}

fn unverified(kind: FindingKind, subject: impl Into<String>, message: String) -> Finding {
    Finding {
        severity: Severity::Unverified,
        kind,
        subject: subject.into(),
        message,
        required: None,
        available: None,
        units: None,
    }
}

fn valid_bay(bay: EquipmentBay) -> bool {
    bay.min_x_m.is_finite()
        && bay.max_x_m.is_finite()
        && bay.min_y_m.is_finite()
        && bay.max_y_m.is_finite()
        && bay.min_z_m.is_finite()
        && bay.max_z_m.is_finite()
        && bay.min_x_m < bay.max_x_m
        && bay.min_y_m < bay.max_y_m
        && bay.min_z_m < bay.max_z_m
}

pub(crate) fn positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

fn nonnegative(value: f64) -> bool {
    value.is_finite() && value >= 0.0
}
