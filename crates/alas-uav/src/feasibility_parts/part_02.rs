// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


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

