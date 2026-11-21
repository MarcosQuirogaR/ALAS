// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The fuel policy sized on the flown mission.
//!
//! The dispatch prices the trip, the diversion and the hold on the segment
//! mission model. The native flight then flies the profile's own ladder and
//! the reserve program, and burns what it burns. [`NativeScales`] carries the
//! ratios of the flown fuel to the priced fuel (trip, diversion, hold), and
//! [`Scaled`] is the burn model the dispatch is solved again on with those
//! ratios applied, so the takeoff mass carries the fuel the flown mission needs
//! and the flown reserves are really there when they are flown. Contingency
//! stays what the regulation states it as (a share of the trip fuel, or the
//! five-minute hold floor), computed on the scaled trip and hold.

use alas_mass::fuel_plan::{FuelBurnModel, FuelModelError, FuelPlan, FuelRule, LegEstimate};

use crate::mission_stage::reserve::FlownReserves;

/// Smallest and largest ratio of flown to priced fuel accepted: outside it the
/// flight and the model disagree about the mission, not about its fuel.
const SCALE_RANGE: (f64, f64) = (0.5, 2.0);

/// Ratios of the flown fuel to the priced fuel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NativeScales {
    /// Trip fuel.
    pub trip: f64,
    /// Missed approach, diversion and landing at the alternate.
    pub diversion: f64,
    /// Holding fuel flow (final reserve, no-alternate hold, contingency floor).
    pub hold: f64,
    /// Whether any ratio was held at the limit of [`SCALE_RANGE`]: the flight and the model then
    /// disagree about the mission, not about its fuel.
    pub clamped: bool,
}

impl Default for NativeScales {
    fn default() -> Self {
        Self {
            trip: 1.0,
            diversion: 1.0,
            hold: 1.0,
            clamped: false,
        }
    }
}

/// Share of the correction applied per flight: the update is damped so the fixed point is
/// approached without chasing the flight-to-flight noise of the guidance.
pub(in crate::mission_stage) const DAMPING: f64 = 0.8;

/// The corrected ratio and whether the correction hit the limit.
fn updated(scale: f64, flown: f64, priced: f64) -> (f64, bool) {
    if flown.is_finite() && flown > 0.0 && priced.is_finite() && priced > 0.0 {
        let target = scale * flown / priced;
        let limited = target.clamp(SCALE_RANGE.0, SCALE_RANGE.1);
        (scale + DAMPING * (limited - scale), limited != target)
    } else {
        (scale, false)
    }
}

impl NativeScales {
    /// These ratios corrected by one flight: `plan` is the policy plan the flight
    /// was flown at (priced with these ratios), `native_trip_kg` the flown trip
    /// fuel, `reserves` the flown reserve program when it completed.
    #[must_use]
    pub(in crate::mission_stage) fn after_flight(
        self,
        plan: &FuelPlan,
        native_trip_kg: f64,
        reserves: Option<&FlownReserves>,
    ) -> Self {
        let (trip, trip_hit) = updated(self.trip, native_trip_kg, plan.trip.kg);
        let mut next = Self {
            trip,
            clamped: self.clamped || trip_hit,
            ..self
        };
        if let Some(reserves) = reserves {
            if matches!(plan.alternate.rule, FuelRule::Diversion { .. }) {
                let (diversion, hit) = updated(
                    self.diversion,
                    reserves.alternate_fuel_kg,
                    plan.alternate.kg,
                );
                next.diversion = diversion;
                next.clamped |= hit;
            }
            let mut priced_hold_kg = plan.final_reserve.kg;
            if matches!(plan.alternate.rule, FuelRule::Holding { .. }) {
                priced_hold_kg += plan.alternate.kg;
            }
            if matches!(plan.final_reserve.rule, FuelRule::Holding { .. }) {
                let (hold, hit) = updated(self.hold, reserves.hold_fuel_kg(), priced_hold_kg);
                next.hold = hold;
                next.clamped |= hit;
            }
        }
        next
    }
}

/// A burn model whose trip, diversion and holding fuel are scaled to the flown mission.
pub(super) struct Scaled<'a> {
    pub inner: &'a dyn FuelBurnModel,
    pub scales: NativeScales,
}

impl FuelBurnModel for Scaled<'_> {
    fn deterministic_for_dispatch(&self) -> bool {
        self.inner.deterministic_for_dispatch()
    }

    fn check_cancellation(&self) -> Result<(), FuelModelError> {
        self.inner.check_cancellation()
    }

    fn trip(&self, takeoff_mass_kg: f64, range_m: f64) -> Result<LegEstimate, FuelModelError> {
        let leg = self.inner.trip(takeoff_mass_kg, range_m)?;
        Ok(LegEstimate {
            fuel_kg: leg.fuel_kg * self.scales.trip,
            ..leg
        })
    }

    fn diversion(
        &self,
        start_mass_kg: f64,
        distance_m: f64,
    ) -> Result<LegEstimate, FuelModelError> {
        let leg = self.inner.diversion(start_mass_kg, distance_m)?;
        Ok(LegEstimate {
            fuel_kg: leg.fuel_kg * self.scales.diversion,
            ..leg
        })
    }

    fn holding_fuel_flow_kg_s(&self, mass_kg: f64, altitude_m: f64) -> Result<f64, FuelModelError> {
        Ok(self.inner.holding_fuel_flow_kg_s(mass_kg, altitude_m)? * self.scales.hold)
    }

    fn cruise_fuel_flow_kg_s(&self, mass_kg: f64) -> Result<f64, FuelModelError> {
        self.inner.cruise_fuel_flow_kg_s(mass_kg)
    }

    fn taxi_fuel_flow_kg_s(&self) -> Result<f64, FuelModelError> {
        self.inner.taxi_fuel_flow_kg_s()
    }

    fn destination_elevation_m(&self) -> f64 {
        self.inner.destination_elevation_m()
    }
}
