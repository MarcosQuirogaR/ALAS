// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Electrical mission-phase simulation for source-bounded UAV powertrains.
//!
//! A two-point propulsion map can demonstrate available thrust, but it cannot
//! represent the energy and peak-current consequences of takeoff, climb,
//! cruise, and reserve phases. This module evaluates every requested phase
//! with the same coupled motor/propeller model and carries the result into the
//! preliminary optimizer without treating a cruise surrogate as a mission.

use crate::optimizer::{MissionPhase, MissionProfile};
use crate::Catalog;

use super::{
    solve_catalogue_powertrain, CataloguePowertrainSelection, ElectricFlightCondition,
    ElectricPropulsionError, ElectricPropulsionResult,
};

/// One commanded electrical flight phase.
#[derive(Debug, Clone, PartialEq)]
pub struct ElectricMissionPhase {
    /// Stable user-facing phase name.
    pub name: String,
    /// Time spent at the declared steady condition.
    pub duration_s: f64,
    /// Coupled propulsion flight condition, including speed and throttle.
    pub condition: ElectricFlightCondition,
}

/// Ordered commanded phases and their source identifier.
#[derive(Debug, Clone, PartialEq)]
pub struct ElectricMissionPlan {
    /// Design brief, test plan, or solver-case identifier for review.
    pub evidence: String,
    /// Ordered phase definitions.
    pub phases: Vec<ElectricMissionPhase>,
}

/// Solved electrical and propulsive result for one mission phase.
#[derive(Debug, Clone, PartialEq)]
pub struct ElectricMissionPhaseResult {
    /// Commanded phase definition.
    pub phase: ElectricMissionPhase,
    /// Coupled source-bounded propulsion result.
    pub propulsion: ElectricPropulsionResult,
    /// Still-air distance accumulated during this phase.
    pub distance_m: f64,
    /// Propulsion-only electrical energy used in this phase.
    pub propulsion_energy_wh: f64,
}

/// Aggregate result of a complete multi-phase electrical mission.
#[derive(Debug, Clone, PartialEq)]
pub struct ElectricMissionResult {
    /// Source identifier inherited from the input plan.
    pub evidence: String,
    /// Solved phase results in the requested order.
    pub phases: Vec<ElectricMissionPhaseResult>,
    /// Total elapsed mission time.
    pub total_duration_s: f64,
    /// Still-air distance accumulated over the phases.
    pub total_distance_m: f64,
    /// Propulsion-only electrical energy before avionics and reserve policy.
    pub propulsion_energy_wh: f64,
    /// Largest source-model battery current in any phase.
    pub maximum_battery_current_a: f64,
    /// Largest source-model battery electrical power in any phase.
    pub maximum_battery_power_w: f64,
}

impl ElectricMissionResult {
    /// Convert solved phases into the optimizer's source-resolved mission input.
    ///
    /// The optimizer adds its declared avionics draw and battery reserve policy
    /// once, rather than duplicating either in the propulsion solver.
    pub fn optimizer_profile(&self) -> MissionProfile {
        MissionProfile {
            evidence: self.evidence.clone(),
            phases: self
                .phases
                .iter()
                .map(|result| MissionPhase {
                    name: result.phase.name.clone(),
                    duration_s: result.phase.duration_s,
                    speed_m_s: result.phase.condition.speed_m_s,
                    air_density_kg_m3: result.phase.condition.air_density_kg_m3,
                    throttle: result.phase.condition.throttle,
                    total_thrust_n: result.propulsion.total_thrust_n,
                    motor_current_a: result.propulsion.per_motor.motor_current_a,
                    motor_power_w: result.propulsion.per_motor.motor_electrical_power_w,
                    battery_current_a: result.propulsion.battery_current_a,
                    battery_power_w: result.propulsion.battery_power_w,
                })
                .collect(),
        }
    }
}

/// Solve each commanded phase for one selected catalogue powertrain.
pub fn simulate_catalogue_mission(
    catalog: &Catalog,
    selection: &CataloguePowertrainSelection,
    plan: &ElectricMissionPlan,
) -> Result<ElectricMissionResult, ElectricPropulsionError> {
    if plan.evidence.trim().is_empty() || plan.phases.is_empty() {
        return Err(ElectricPropulsionError::InvalidInput(
            "an electrical mission needs evidence and at least one phase".to_owned(),
        ));
    }
    let mut result = ElectricMissionResult {
        evidence: plan.evidence.clone(),
        phases: Vec::with_capacity(plan.phases.len()),
        total_duration_s: 0.0,
        total_distance_m: 0.0,
        propulsion_energy_wh: 0.0,
        maximum_battery_current_a: 0.0,
        maximum_battery_power_w: 0.0,
    };
    for phase in &plan.phases {
        if phase.name.trim().is_empty() || !phase.duration_s.is_finite() || phase.duration_s <= 0.0
        {
            return Err(ElectricPropulsionError::InvalidInput(format!(
                "mission phase '{}' needs a non-empty name and positive finite duration",
                phase.name
            )));
        }
        let propulsion = solve_catalogue_powertrain(catalog, selection, phase.condition)?;
        let distance_m = phase.condition.speed_m_s * phase.duration_s;
        let propulsion_energy_wh = propulsion.battery_power_w * phase.duration_s / 3600.0;
        result.total_duration_s += phase.duration_s;
        result.total_distance_m += distance_m;
        result.propulsion_energy_wh += propulsion_energy_wh;
        result.maximum_battery_current_a = result
            .maximum_battery_current_a
            .max(propulsion.battery_current_a);
        result.maximum_battery_power_w = result
            .maximum_battery_power_w
            .max(propulsion.battery_power_w);
        result.phases.push(ElectricMissionPhaseResult {
            phase: phase.clone(),
            propulsion,
            distance_m,
            propulsion_energy_wh,
        });
    }
    Ok(result)
}
