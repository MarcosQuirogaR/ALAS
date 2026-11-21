// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The sizing of the takeoff mass on the flown mission: its outcome and the fuel it checks.

use super::SelectedLoadCase;
use crate::mission_stage::dispatch;
use crate::mission_stage::reserve::FlownReserves;

/// How the sizing of the takeoff mass on the flown mission ended.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SizingOutcome {
    /// The takeoff mass settled within the tolerance, the reserves were flown and the fuel on
    /// board covers what was flown.
    pub converged: bool,
    /// Flights flown by the sizing (the last one is the reported flight).
    pub passes: usize,
    /// Change of the takeoff mass at the last re-sizing, kg.
    pub mass_residual_kg: f64,
    /// Fuel on board at brake release less the fuel the flight needs (trip, flown reserves,
    /// contingency, taxi-in, additional, extra), kg: negative when short.
    pub fuel_residual_kg: f64,
    /// A flown-to-priced ratio was held at its limit.
    pub clamped: bool,
    /// The reserve program was flown (or none was asked for).
    pub reserves_flown: bool,
}

/// Fuel the flown mission needs from brake release, kg: the trip, the flown diversion and hold
/// (the priced ones for a reserve that is an allowance, not a flown phase), the contingency and
/// the taxi-in, additional and extra fuel of the plan.
pub(super) fn required_fuel_kg(
    plan: &alas_mass::fuel_plan::FuelPlan,
    trip_kg: f64,
    reserves: Option<&FlownReserves>,
) -> f64 {
    use alas_mass::fuel_plan::FuelRule;
    let flown_hold = reserves
        .filter(|reserves| !reserves.holds.is_empty())
        .map(FlownReserves::hold_fuel_kg);
    let (alternate_kg, final_kg) = if matches!(plan.alternate.rule, FuelRule::Holding { .. }) {
        // No alternate: the no-alternate hold and the final reserve are both holds.
        (
            0.0,
            flown_hold.unwrap_or(plan.alternate.kg + plan.final_reserve.kg),
        )
    } else {
        let alternate = reserves
            .filter(|_| matches!(plan.alternate.rule, FuelRule::Diversion { .. }))
            .map_or(plan.alternate.kg, |reserves| reserves.alternate_fuel_kg);
        let final_reserve = if matches!(plan.final_reserve.rule, FuelRule::Holding { .. }) {
            flown_hold.unwrap_or(plan.final_reserve.kg)
        } else {
            plan.final_reserve.kg
        };
        (alternate, final_reserve)
    };
    trip_kg
        + alternate_kg
        + final_kg
        + plan.contingency.kg
        + plan.additional.kg
        + plan.extra.kg
        + plan.taxi_in_fuel_kg()
}

/// The native trip fuel and the plan of a policy closure that was flown to completion.
pub(super) fn flown_policy(
    load_case: &SelectedLoadCase,
) -> Option<(f64, alas_mass::fuel_plan::FuelPlan)> {
    match &load_case.selection {
        dispatch::LoadCaseSelection::PolicyClosure(case) => {
            Some((case.native_check?.native_trip_fuel_kg, case.plan))
        }
        _ => None,
    }
}
