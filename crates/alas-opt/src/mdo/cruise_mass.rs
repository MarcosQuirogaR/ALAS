// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The mid-cruise mass the in-loop cruise checks are evaluated at.
//!
//! # Definition
//!
//! The mid-cruise state is that of the design mission flown from the
//! candidate's own takeoff loading: the aircraft leaves at the takeoff mass
//! `TOW` carrying `F` kg of fuel at brake release, lands with the fuel its
//! dispatch plan keeps beyond the trip (`R`: contingency, alternate, final
//! reserve, additional and extra fuel and the taxi-in budget,
//! [`FuelPlan::destination_landing_fuel_kg`]), and so burns `F - R` in the
//! trip. Mid-cruise is the mean of the Breguet endpoints:
//!
//! `m_mid = TOW - (F - R) / 2`.
//!
//! Under every mission-closed MTOW mode the loading is the dispatch itself
//! (`F` is its takeoff fuel), so `F - R` is the dispatch's trip fuel and the
//! mass is the closed mission's own mid-cruise mass. Under Hard MTOW the
//! loading is the maximum the weight and volume budgets admit
//! (`alas_mass::loading::MtowFuelLoading`), and the mass is the mid-cruise
//! state of the longest mission that loading flies, a real flight state of
//! the aircraft at that takeoff mass, rather than the takeoff mass of one
//! mission combined with the trip of another.
//!
//! # Reserves of the longer mission
//!
//! A contingency priced as a share `f` of the trip
//! ([`FuelRule::TripFraction`]) grows with the trip, so the trip of the
//! loading is solved from `T + f T + R_other = F`, `R_other` being every
//! other quantity left on landing. Every other reserve (holds, diversion,
//! final reserve, flight-time shares) is kept at the dispatch plan's value:
//! those quantities are priced at the landing mass and flight time of the
//! plan's own mission and are not re-flown here.
//!
//! Units: kilograms.

use alas_mass::fuel_plan::{FuelPlan, FuelRule};

/// Trip fuel, kg, of the mission flown from `loaded_fuel_kg` of fuel at
/// brake release with the reserves of `plan`, never negative.
///
/// `NaN` when an input is not finite.
#[must_use]
pub fn trip_fuel_of_loading_kg(loaded_fuel_kg: f64, plan: &FuelPlan) -> f64 {
    let landing_fuel_kg = plan.destination_landing_fuel_kg();
    if !(loaded_fuel_kg.is_finite() && landing_fuel_kg.is_finite()) {
        return f64::NAN;
    }
    let trip_kg = match plan.contingency.rule {
        FuelRule::TripFraction { fraction } if fraction.is_finite() && fraction >= 0.0 => {
            let other_kg = landing_fuel_kg - plan.contingency.kg;
            (loaded_fuel_kg - other_kg) / (1.0 + fraction)
        }
        _ => loaded_fuel_kg - landing_fuel_kg,
    };
    trip_kg.max(0.0)
}

/// Mid-cruise mass, kg, of the mission flown from a takeoff loading of
/// `takeoff_mass_kg` carrying `loaded_fuel_kg` of fuel at brake release,
/// with the reserves of `plan` (see the module documentation).
///
/// The burned fuel is bounded to `[0, loaded_fuel_kg]`; `NaN` when an input
/// is not finite.
#[must_use]
pub fn mid_cruise_mass_of_loading_kg(
    takeoff_mass_kg: f64,
    loaded_fuel_kg: f64,
    plan: &FuelPlan,
) -> f64 {
    if !takeoff_mass_kg.is_finite() {
        return f64::NAN;
    }
    let trip_kg = trip_fuel_of_loading_kg(loaded_fuel_kg, plan);
    if trip_kg.is_nan() {
        return f64::NAN;
    }
    takeoff_mass_kg - 0.5 * trip_kg.min(loaded_fuel_kg.max(0.0))
}

#[cfg(test)]
#[path = "cruise_mass_tests.rs"]
mod tests;
