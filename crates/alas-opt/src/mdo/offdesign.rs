// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The selected route flown off-design by an aircraft sized on its design
//! mission.
//!
//! When the takeoff mass is closed on the design mission (the MTOW band
//! mode, or the payload-adjusted mode with a design range), or the run
//! planned a route other than the great circle the aircraft was sized on,
//! the route is not what sized the aircraft; it is a mission the sized
//! aircraft has to be able to fly, and the one the report's mission and
//! delivery check fly (`alas_config::MissionConfig::route_distance_m`). It
//! is flown here with the same dispatch
//! solver and fuel policy, at the route's own cruise altitude on a trip plan
//! frozen for the route ([`super::sizing::planned_mission`]), with the
//! laid-out payload and the converged trimmed drag, and with no MTOW, landing
//! or tank limit applied inside the solve, so the three checks read the
//! unclamped requirement: the ramp fuel (reserve-inclusive takeoff fuel plus
//! the taxi fuel that also occupies the tanks) against usable tank capacity, route payload against the derived design structural payload
//! (derived MZFW minus OEW), and route takeoff mass against the takeoff-mass
//! limit: the MTOW a design mission closed on, else the declared MTOW.

use alas_config::optimizer::UNBOUNDED_DISPATCH_MTOW_KG;
use alas_config::AlasConfig;
use alas_mass::dispatch::{DispatchLimits, DispatchSolution, DispatchStatus};

use super::mission_model::{FreezeError, SegmentMissionModel, MISSION_STEP_UNCONVERGED};
use super::sizing::planned_mission::solve_planned_dispatch;
use super::trim::TrimmedPolar;
use super::types::CandidateFailure;

/// The route flown off-design at the closed takeoff mass.
#[derive(Debug, Clone, PartialEq)]
pub struct OffDesignFlight {
    /// Route still-air distance, m.
    pub range_m: f64,
    /// Laid-out route payload, kg.
    pub payload_kg: f64,
    /// Derived design structural payload, kg: derived design MZFW minus OEW,
    /// which is the design payload.
    pub payload_limit_kg: f64,
    /// Takeoff-mass limit the route is flown under, kg.
    pub mtow_kg: f64,
    /// Route takeoff mass required, kg: route zero-fuel mass plus the
    /// reserve-inclusive takeoff fuel, unclamped.
    pub required_takeoff_mass_kg: f64,
    /// Reserve-inclusive takeoff fuel of the route, kg.
    pub takeoff_fuel_kg: f64,
    /// Usable tank capacity, kg, or `NaN` when it could not be resolved.
    pub usable_capacity_kg: f64,
    /// The route dispatch solution.
    pub dispatch: DispatchSolution,
}

/// What the off-design flight reads from the closed candidate.
pub(crate) struct ClosedAircraft<'a> {
    /// Converged cruise trim, whose drag the route is flown on.
    pub polar: &'a TrimmedPolar,
    /// Operating empty mass, kg.
    pub operating_empty_mass_kg: f64,
    /// Laid-out route payload, kg.
    pub payload_kg: f64,
    /// Design payload, kg.
    pub design_payload_kg: f64,
    /// Takeoff-mass limit, kg: the MTOW a design mission closed on, else
    /// the declared MTOW.
    pub mtow_kg: f64,
    /// The closed takeoff mass the route's trip plan is first frozen at,
    /// kg, as the reporting dispatch plans it.
    pub planning_mass_kg: f64,
    /// Usable tank capacity, kg, when resolved.
    pub usable_capacity_kg: Option<f64>,
}

/// Fly the route `range_m` with `route_model` off-design.
///
/// # Errors
///
/// `cancelled` when the run was cancelled during the solve.
pub(crate) fn fly_route(
    config: &AlasConfig,
    route_model: &SegmentMissionModel,
    range_m: f64,
    aircraft: &ClosedAircraft<'_>,
) -> Result<OffDesignFlight, CandidateFailure> {
    let model = route_model
        .clone()
        .with_cruise_drag(aircraft.polar.drag.cruise_drag());
    let objective = &config.optimizer.objective;
    let zero_fuel_mass_kg = aircraft.operating_empty_mass_kg + aircraft.payload_kg;
    let limits = DispatchLimits {
        mtow_kg: UNBOUNDED_DISPATCH_MTOW_KG,
        mzfw_kg: None,
        mlw_kg: None,
        usable_capacity_kg: None,
    };
    // The route is flown on a trip plan frozen for its own mission, planned
    // first at the closed takeoff mass, as the closure flies its own.
    let dispatch = solve_planned_dispatch(
        &model,
        zero_fuel_mass_kg,
        aircraft.planning_mass_kg,
        range_m,
        &config.fuel_policy,
        &limits,
        objective.sizing_max_iterations.max(1) as usize,
        objective.sizing_tolerance_kg,
    )
    .map_err(|error| CandidateFailure {
        reason: match error {
            FreezeError::StepUnconverged { .. } => MISSION_STEP_UNCONVERGED,
            FreezeError::Fuel(_) => "cancelled",
        },
    })?;
    if matches!(dispatch.status, DispatchStatus::Cancelled) {
        return Err(CandidateFailure {
            reason: "cancelled",
        });
    }
    let takeoff_fuel_kg = dispatch.plan.takeoff_fuel_kg();
    Ok(OffDesignFlight {
        range_m,
        payload_kg: aircraft.payload_kg,
        payload_limit_kg: aircraft.design_payload_kg,
        mtow_kg: aircraft.mtow_kg,
        required_takeoff_mass_kg: zero_fuel_mass_kg + takeoff_fuel_kg,
        takeoff_fuel_kg,
        usable_capacity_kg: aircraft.usable_capacity_kg.unwrap_or(f64::NAN),
        dispatch,
    })
}
