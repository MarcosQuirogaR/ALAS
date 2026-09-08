// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


pub(super) fn metrics(
    problem: &OptimizationProblem<'_>,
    design: &UavDesign,
    report: &UavReport,
) -> Option<ObjectiveMetrics> {
    let cruise_case = design.flight_conditions.iter().find(|condition| {
        condition.load_factor == 1.0
            && (condition.speed_m_s - problem.objectives.cruise_speed_m_s).abs()
                <= 1.0e-9 * problem.objectives.cruise_speed_m_s.max(1.0)
    })?;
    let cruise = report
        .flight_conditions
        .iter()
        .find(|condition| condition.name == cruise_case.name)?;
    let required_thrust_n = cruise.required_thrust_n?;
    let takeoff_mass_kg = report.takeoff_mass_kg?;
    let duration = mission_duration(problem);
    let electrical_power_w = problem.mission_profile.map_or_else(
        || design.mission_energy.mission_energy_wh * 3600.0 / duration,
        |profile| {
            profile
                .phases
                .iter()
                .find(|phase| {
                    (phase.speed_m_s - problem.objectives.cruise_speed_m_s).abs()
                        <= 1.0e-9 * problem.objectives.cruise_speed_m_s.max(1.0)
                })
                .map(|phase| phase.battery_power_w + problem.systems.avionics_power_w)
                .unwrap_or(0.0)
        },
    );
    if electrical_power_w <= 0.0 {
        return None;
    }
    let efficiency = required_thrust_n * problem.objectives.cruise_speed_m_s / electrical_power_w;
    let mass_ratio = takeoff_mass_kg / problem.objectives.payload_mass_kg;
    Some(ObjectiveMetrics {
        mission_duration_s: duration,
        mission_energy_wh: design.mission_energy.mission_energy_wh,
        propulsive_efficiency: efficiency,
        takeoff_mass_kg,
        score: problem.objectives.efficiency_priority * (1.0 - efficiency)
            + (1.0 - problem.objectives.efficiency_priority) * mass_ratio,
    })
}

pub(super) fn validate_model(
    model: PreliminaryModel,
    systems: SystemsDefinition,
) -> Result<(), OptimizationError> {
    let positive_values = [
        model.air_density_kg_m3,
        model.maximum_lift_coefficient,
        model.zero_lift_drag_coefficient,
        model.oswald_efficiency,
        model.limit_load_factor,
        model.structural_safety_factor,
        model.horizontal_tail_volume_coefficient,
        model.vertical_tail_volume_coefficient,
        model.horizontal_tail_aspect_ratio,
        model.vertical_tail_aspect_ratio,
        model.spar_cap_width_fraction,
        model.spar_cap_separation_fraction,
        model.aileron_area_fraction,
        model.aileron_chord_fraction,
        model.elevator_area_fraction,
        model.elevator_chord_fraction,
        model.hinge_moment_coefficient,
        model.landing_gear_track_fraction,
        model.landing_gear_wheelbase_fraction,
        model.propeller_ground_clearance_m,
        model.fixed_systems_mass_kg,
        systems.avionics_mass_kg,
        systems.avionics_power_w,
        systems.control_bus_current_a,
        systems.control_bus_voltage_v,
    ];
    let fractions = [
        model.oswald_efficiency,
        model.forward_cg_chord_fraction,
        model.aft_cg_chord_fraction,
        model.nose_length_fraction,
        model.tailcone_length_fraction,
        model.spar_cap_width_fraction,
        model.spar_cap_separation_fraction,
        model.aileron_area_fraction,
        model.aileron_chord_fraction,
        model.elevator_area_fraction,
        model.elevator_chord_fraction,
        model.landing_gear_track_fraction,
        model.landing_gear_wheelbase_fraction,
    ];
    if positive_values
        .into_iter()
        .any(|value| !value.is_finite() || value <= 0.0)
        || fractions
            .into_iter()
            .any(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
        || model.forward_cg_chord_fraction >= model.aft_cg_chord_fraction
        || model.nose_length_fraction + model.tailcone_length_fraction >= 1.0
        || !super::validation::positive_dimensions(systems.avionics_dimensions)
        || model.equipment_clearance_m < 0.0
        || model.equipment_gap_m < 0.0
        || systems.minimum_receiver_channels == 0
    {
        return Err(OptimizationError::InvalidProblem(
            "preliminary model, CG, bay, or avionics inputs are inconsistent".to_owned(),
        ));
    }
    Ok(())
}

fn installed_servo(
    role: &str,
    spec: &ServoSpec,
    placement: Placement,
    required_torque_nm: f64,
    continuous_current_a: Option<f64>,
) -> InstalledServo {
    InstalledServo {
        id: role.to_owned(),
        spec: spec.clone(),
        placement,
        required_torque_nm,
        continuous_current_a,
    }
}

fn check_control_equipment(
    problem: &OptimizationProblem<'_>,
    selection: &Selection<'_>,
    findings: &mut Vec<Finding>,
) {
    match selection.receiver.channel_count {
        Some(available) if available < problem.systems.minimum_receiver_channels => {
            findings.push(failure(
                FindingKind::InvalidInput,
                &selection.receiver_record.id,
                "receiver does not provide enough independently addressable channels",
                Some(f64::from(problem.systems.minimum_receiver_channels)),
                Some(f64::from(available)),
                Some("channels"),
            ))
        }
        Some(_) => {}
        None => findings.push(missing(
            &selection.receiver_record.id,
            "published receiver channel count",
        )),
    }
    for (record, minimum, maximum) in [
        (
            selection.receiver_record,
            selection.receiver.min_voltage_v,
            selection.receiver.max_voltage_v,
        ),
        (
            selection.electronics_record,
            selection.electronics.min_voltage_v,
            selection.electronics.max_voltage_v,
        ),
    ] {
        match (minimum, maximum) {
            (Some(minimum), Some(maximum))
                if problem.systems.control_bus_voltage_v < minimum
                    || problem.systems.control_bus_voltage_v > maximum =>
            {
                findings.push(failure(
                    FindingKind::ControlVoltageMismatch,
                    &record.id,
                    "control-bus voltage is outside the published supply range",
                    Some(problem.systems.control_bus_voltage_v),
                    None,
                    Some("V"),
                ));
            }
            (Some(_), Some(_)) => {}
            _ => findings.push(missing(&record.id, "published supply-voltage range")),
        }
    }
}

fn operating_point(map: &super::PropulsionMap, speed_m_s: f64) -> PropulsionOperatingPoint {
    map.at_speed(speed_m_s).unwrap_or(PropulsionOperatingPoint {
        speed_m_s,
        thrust_n: 0.0,
        motor_current_a: 0.0,
        motor_power_w: 0.0,
    })
}

fn failure(
    kind: FindingKind,
    subject: &str,
    message: &str,
    required: Option<f64>,
    available: Option<f64>,
    units: Option<&'static str>,
) -> Finding {
    Finding {
        severity: Severity::Failure,
        kind,
        subject: subject.to_owned(),
        message: message.to_owned(),
        required,
        available,
        units,
    }
}

fn missing(subject: &str, field: &str) -> Finding {
    Finding {
        severity: Severity::Unverified,
        kind: FindingKind::MissingData,
        subject: subject.to_owned(),
        message: format!("missing {field}; no value was inferred"),
        required: None,
        available: None,
        units: None,
    }
}

fn invalid(subject: &str, value: f64, message: &str) -> Finding {
    Finding {
        severity: Severity::Failure,
        kind: FindingKind::InvalidInput,
        subject: subject.to_owned(),
        message: message.to_owned(),
        required: Some(value),
        available: None,
        units: None,
    }
}

