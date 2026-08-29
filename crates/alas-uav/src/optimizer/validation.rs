// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Validation of source-resolved optimizer inputs before sampling begins.
//!
//! Keeping these checks outside the search loop makes a missing map range or
//! an impossible mission brief an input error, rather than a misleading count
//! of physically rejected aircraft candidates.

use crate::catalog::Dimensions;

use super::{generation, OptimizationError, OptimizationProblem, PropulsionMap};

pub(super) fn validate_problem(problem: &OptimizationProblem<'_>) -> Result<(), OptimizationError> {
    let objectives = problem.objectives;
    for (name, value) in [
        ("endurance", objectives.endurance_s),
        ("range", objectives.range_m),
        ("cruise speed", objectives.cruise_speed_m_s),
        ("stall speed", objectives.maximum_stall_speed_m_s),
        ("payload mass", objectives.payload_mass_kg),
    ] {
        if !value.is_finite() || value <= 0.0 {
            return Err(OptimizationError::InvalidProblem(format!(
                "{name} must be positive"
            )));
        }
    }
    if objectives.cruise_speed_m_s <= objectives.maximum_stall_speed_m_s {
        return Err(OptimizationError::InvalidProblem(
            "cruise speed must exceed the maximum stall speed".to_owned(),
        ));
    }
    for (name, value) in [
        (
            "minimum efficiency",
            objectives.minimum_propulsive_efficiency,
        ),
        ("efficiency priority", objectives.efficiency_priority),
        (
            "maximum depth of discharge",
            problem.systems.maximum_depth_of_discharge,
        ),
        ("reserve fraction", problem.systems.reserve_fraction),
        (
            "servo current fraction",
            problem.systems.servo_continuous_current_fraction,
        ),
    ] {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(OptimizationError::InvalidProblem(format!(
                "{name} must lie in [0, 1]"
            )));
        }
    }
    for bounds in [
        problem.geometry_bounds.wing_area_m2,
        problem.geometry_bounds.wing_aspect_ratio,
        problem.geometry_bounds.fuselage_length_m,
        problem.geometry_bounds.wing_leading_edge_fraction,
    ] {
        if !bounds.valid_positive() {
            return Err(OptimizationError::InvalidProblem(
                "continuous bounds must be finite, positive, and ordered".to_owned(),
            ));
        }
    }
    if problem.evaluations == 0 {
        return Err(OptimizationError::InvalidProblem(
            "at least one candidate evaluation is required".to_owned(),
        ));
    }
    if !positive_dimensions(objectives.payload_dimensions) {
        return Err(OptimizationError::InvalidProblem(
            "payload dimensions must be positive".to_owned(),
        ));
    }
    generation::validate_model(problem.model, problem.systems)
        .and_then(|()| {
            validate_propulsion_maps(
                problem.propulsion_maps,
                [
                    problem.objectives.maximum_stall_speed_m_s,
                    problem.objectives.cruise_speed_m_s,
                ],
            )
        })
        .and_then(|()| validate_mission_profile(problem))
}

fn validate_propulsion_maps(
    maps: &[PropulsionMap],
    checked_speeds: [f64; 2],
) -> Result<(), OptimizationError> {
    for map in maps {
        if map.motor_id.trim().is_empty()
            || map.propeller_id.trim().is_empty()
            || map.series_cells == 0
            || map.motor_count == 0
            || map.evidence.trim().is_empty()
            || map.points.len() < 2
        {
            return Err(OptimizationError::InvalidProblem(
                "propulsion maps need component keys, evidence, and at least two points".to_owned(),
            ));
        }
        let mut previous_speed = -1.0;
        for point in &map.points {
            if !point.speed_m_s.is_finite()
                || point.speed_m_s < 0.0
                || point.speed_m_s <= previous_speed
                || !point.thrust_n.is_finite()
                || point.thrust_n <= 0.0
                || !point.motor_current_a.is_finite()
                || point.motor_current_a <= 0.0
                || !point.motor_power_w.is_finite()
                || point.motor_power_w <= 0.0
                || point.thrust_n * point.speed_m_s > point.motor_power_w
            {
                return Err(OptimizationError::InvalidProblem(format!(
                    "propulsion map '{}' has nonphysical or unordered points",
                    map.evidence
                )));
            }
            previous_speed = point.speed_m_s;
        }
        for speed in checked_speeds {
            if map
                .at_speed(speed)
                .is_some_and(|point| point.thrust_n * speed > point.motor_power_w)
            {
                return Err(OptimizationError::InvalidProblem(format!(
                    "propulsion map '{}' exceeds unit propulsive efficiency at a checked speed",
                    map.evidence
                )));
            }
        }
    }
    Ok(())
}

fn validate_mission_profile(problem: &OptimizationProblem<'_>) -> Result<(), OptimizationError> {
    let Some(profile) = problem.mission_profile else {
        return Ok(());
    };
    if profile.evidence.trim().is_empty() || profile.phases.is_empty() {
        return Err(OptimizationError::InvalidProblem(
            "mission profiles need evidence and at least one phase".to_owned(),
        ));
    }
    let mut names = Vec::<&str>::with_capacity(profile.phases.len());
    let mut duration_s = 0.0;
    let mut distance_m = 0.0;
    for phase in &profile.phases {
        if phase.name.trim().is_empty()
            || names.contains(&phase.name.as_str())
            || !phase.duration_s.is_finite()
            || phase.duration_s <= 0.0
            || !phase.speed_m_s.is_finite()
            || phase.speed_m_s <= 0.0
            || !phase.air_density_kg_m3.is_finite()
            || phase.air_density_kg_m3 <= 0.0
            || !phase.throttle.is_finite()
            || !(0.0..=1.0).contains(&phase.throttle)
            || !phase.total_thrust_n.is_finite()
            || phase.total_thrust_n < 0.0
            || !phase.motor_current_a.is_finite()
            || phase.motor_current_a < 0.0
            || !phase.motor_power_w.is_finite()
            || phase.motor_power_w < 0.0
            || !phase.battery_current_a.is_finite()
            || phase.battery_current_a < 0.0
            || !phase.battery_power_w.is_finite()
            || phase.battery_power_w < 0.0
        {
            return Err(OptimizationError::InvalidProblem(format!(
                "mission phase '{}' has an invalid, duplicate, or nonphysical value",
                phase.name
            )));
        }
        names.push(&phase.name);
        duration_s += phase.duration_s;
        distance_m += phase.speed_m_s * phase.duration_s;
    }
    if duration_s < problem.objectives.endurance_s {
        return Err(OptimizationError::InvalidProblem(format!(
            "mission profile duration {duration_s:.3} s is shorter than required endurance {:.3} s",
            problem.objectives.endurance_s
        )));
    }
    if distance_m < problem.objectives.range_m {
        return Err(OptimizationError::InvalidProblem(format!(
            "mission profile distance {distance_m:.3} m is shorter than required range {:.3} m",
            problem.objectives.range_m
        )));
    }
    let tolerance_m_s = 1.0e-9 * problem.objectives.cruise_speed_m_s.max(1.0);
    if !profile
        .phases
        .iter()
        .any(|phase| (phase.speed_m_s - problem.objectives.cruise_speed_m_s).abs() <= tolerance_m_s)
    {
        return Err(OptimizationError::InvalidProblem(
            "mission profile must include a one-g phase at the design cruise speed".to_owned(),
        ));
    }
    Ok(())
}

pub(super) fn positive_dimensions(dimensions: Dimensions) -> bool {
    [dimensions.length_m, dimensions.width_m, dimensions.height_m]
        .into_iter()
        .all(|value| value.is_finite() && value > 0.0)
}
