// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Block fuel of a stated reference mission, priced by the report's own route
//! pricing.
//!
//! A manufacturer quotes block fuel for a mission of a stated distance
//! ("300 NM"). This flies that distance, as still-air route distance, on the
//! report's aircraft with the route pricing the full analysis' mission stage
//! and the sandbox use ([`crate::mission_stage::dispatch::select_route_load_case`]):
//! the segment mission model, a trip plan frozen for the route, the
//! configured fuel policy, the report's own zero-fuel mass (operating empty
//! plus the payload the report carries), and the takeoff mass the policy
//! closure settles at. Block fuel is the gate-to-gate consumption of the
//! nominal flight: taxi out, trip and taxi in
//! ([`alas_mass::fuel_plan::FuelPlan::block_fuel_kg`]).

use alas_config::{airports, AlasConfig};
use alas_mass::dispatch::DispatchStatus;

use crate::full_analysis::AnalysisReport;
use crate::mission_stage::dispatch::select_route_load_case;

/// The priced reference mission. Masses in kg, distance in m.
#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceRouteFuel {
    /// Still-air route distance flown, m.
    pub range_m: f64,
    /// Block fuel (taxi out, trip, taxi in), kg.
    pub block_fuel_kg: f64,
    /// Trip fuel (brake release to landing), kg.
    pub trip_fuel_kg: f64,
    /// Fuel carried at brake release (trip, reserves, taxi-in budget), kg.
    pub takeoff_fuel_kg: f64,
    /// Reserve fuel the policy protects beyond the trip, kg.
    pub reserve_fuel_kg: f64,
    /// Taxi-out fuel, kg.
    pub taxi_out_fuel_kg: f64,
    /// Operating empty plus payload of the priced load case, kg.
    pub zero_fuel_mass_kg: f64,
    /// Takeoff mass the closure settled at, kg.
    pub takeoff_mass_kg: f64,
    /// Flight time from brake release to landing, s.
    pub trip_time_s: f64,
    /// Whether the dispatch closure converged inside its limits.
    pub converged: bool,
    /// Dispatch status in words.
    pub status: String,
    /// Fuel the route needs beyond what the limits (takeoff mass, tanks)
    /// admit, kg; zero when the route fits.
    pub shortfall_kg: f64,
}

/// Price a still-air route of `range_m` on the aircraft `report` describes.
///
/// # Errors
///
/// The reason when the airports of the configuration are unknown or the
/// route cannot be priced on the segment mission model.
pub fn reference_route_fuel(
    config: &AlasConfig,
    report: &AnalysisReport,
    range_m: f64,
) -> Result<ReferenceRouteFuel, String> {
    if !(range_m.is_finite() && range_m > 0.0) {
        return Err(format!("reference range {range_m} m is not usable"));
    }
    let origin = airports::get(&config.departure_airport).map_err(|error| error.to_string())?;
    let destination = airports::get(&config.arrival_airport).map_err(|error| error.to_string())?;
    let (_, load_case) = select_route_load_case(config, report, origin, destination, range_m)?;
    let reason = match &load_case.selection {
        crate::LoadCaseSelection::MaximumAvailableFuel { reason, .. } => reason.clone(),
        crate::LoadCaseSelection::PolicyClosure(_) => None,
    };
    let case = load_case.route_case().ok_or_else(|| {
        reason.unwrap_or_else(|| "the route could not be priced on the segment model".to_owned())
    })?;
    let plan = &case.plan;
    let status = match &case.analytic.status {
        DispatchStatus::Converged => "converged".to_owned(),
        other => format!("{other:?}"),
    };
    Ok(ReferenceRouteFuel {
        range_m,
        block_fuel_kg: plan.block_fuel_kg(),
        trip_fuel_kg: plan.trip.kg,
        takeoff_fuel_kg: plan.takeoff_fuel_kg(),
        reserve_fuel_kg: plan.reserve_fuel_kg(),
        taxi_out_fuel_kg: plan.taxi.kg,
        zero_fuel_mass_kg: load_case.zero_fuel_mass_kg,
        takeoff_mass_kg: case.analytic.takeoff_mass_kg,
        trip_time_s: plan.trip_time_s,
        converged: case.converged,
        status,
        shortfall_kg: case.shortfall_kg,
    })
}
