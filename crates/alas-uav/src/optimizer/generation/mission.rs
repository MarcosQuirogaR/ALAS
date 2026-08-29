// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mission-profile aggregation used by preliminary UAV candidate generation.
//!
//! A profile carries solved propulsion results for individual flight phases.
//! Keeping the integration here ensures the energy gate, peak-current gate,
//! and aerodynamic flight checks use the same phase definitions.

use crate::feasibility::FlightCondition;

use super::super::{OptimizationProblem, PropulsionOperatingPoint};

pub(super) struct MissionSummary {
    pub(super) duration_s: f64,
    pub(super) energy_wh: f64,
    pub(super) maximum_motor_current_a: f64,
    pub(super) maximum_motor_power_w: f64,
    pub(super) maximum_battery_current_a: f64,
}

pub(super) fn mission_summary(
    problem: &OptimizationProblem<'_>,
    stall: PropulsionOperatingPoint,
    cruise: PropulsionOperatingPoint,
    propulsor_count: f64,
    battery_voltage_v: f64,
) -> MissionSummary {
    let maximum_motor_current_a = stall.motor_current_a.max(cruise.motor_current_a);
    let maximum_motor_power_w = stall.motor_power_w.max(cruise.motor_power_w);
    let maximum_battery_current_a = propulsor_count * maximum_motor_current_a
        + problem.systems.avionics_power_w / battery_voltage_v;
    let Some(profile) = problem.mission_profile else {
        let duration_s = mission_duration(problem);
        return MissionSummary {
            duration_s,
            energy_wh: (propulsor_count * cruise.motor_power_w + problem.systems.avionics_power_w)
                * duration_s
                / 3600.0,
            maximum_motor_current_a,
            maximum_motor_power_w,
            maximum_battery_current_a,
        };
    };
    let mut summary = MissionSummary {
        duration_s: 0.0,
        energy_wh: 0.0,
        maximum_motor_current_a,
        maximum_motor_power_w,
        maximum_battery_current_a,
    };
    for phase in &profile.phases {
        summary.duration_s += phase.duration_s;
        summary.energy_wh +=
            (phase.battery_power_w + problem.systems.avionics_power_w) * phase.duration_s / 3600.0;
        summary.maximum_motor_current_a =
            summary.maximum_motor_current_a.max(phase.motor_current_a);
        summary.maximum_motor_power_w = summary.maximum_motor_power_w.max(phase.motor_power_w);
        summary.maximum_battery_current_a = summary
            .maximum_battery_current_a
            .max(phase.battery_current_a + problem.systems.avionics_power_w / battery_voltage_v);
    }
    summary
}

pub(super) fn flight_conditions(
    problem: &OptimizationProblem<'_>,
    stall: PropulsionOperatingPoint,
    cruise: PropulsionOperatingPoint,
    propulsor_count: f64,
) -> Vec<FlightCondition> {
    let density = problem.model.air_density_kg_m3;
    let mut conditions = if let Some(profile) = problem.mission_profile {
        profile
            .phases
            .iter()
            .map(|phase| FlightCondition {
                name: phase.name.clone(),
                density_kg_m3: phase.air_density_kg_m3,
                speed_m_s: phase.speed_m_s,
                load_factor: 1.0,
                available_thrust_n: Some(phase.total_thrust_n),
            })
            .collect()
    } else {
        vec![
            FlightCondition {
                name: "maximum permitted stall speed".to_owned(),
                density_kg_m3: density,
                speed_m_s: problem.objectives.maximum_stall_speed_m_s,
                load_factor: 1.0,
                available_thrust_n: Some(propulsor_count * stall.thrust_n),
            },
            FlightCondition {
                name: "cruise".to_owned(),
                density_kg_m3: density,
                speed_m_s: problem.objectives.cruise_speed_m_s,
                load_factor: 1.0,
                available_thrust_n: Some(propulsor_count * cruise.thrust_n),
            },
        ]
    };
    conditions.push(FlightCondition {
        name: "positive maneuver".to_owned(),
        density_kg_m3: density,
        speed_m_s: problem.objectives.cruise_speed_m_s,
        load_factor: problem.model.limit_load_factor,
        available_thrust_n: Some(propulsor_count * cruise.thrust_n),
    });
    conditions
}

pub(super) fn mission_duration(problem: &OptimizationProblem<'_>) -> f64 {
    problem.mission_profile.map_or_else(
        || {
            problem
                .objectives
                .endurance_s
                .max(problem.objectives.range_m / problem.objectives.cruise_speed_m_s)
        },
        |profile| profile.phases.iter().map(|phase| phase.duration_s).sum(),
    )
}
