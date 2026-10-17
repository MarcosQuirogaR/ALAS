// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Fuel and time of the take-off ground roll (audit MIS-06).
//!
//! The planned ladder starts at the take-off speed over the runway, so the
//! brake-release-to-lift-off burn is not in any segment. It is a listed
//! allowance: the fuel flow at take-off rating over the profile's
//! `takeoff_roll_time_s` (the ICAO LTO reference take-off mode, 0.7 min =
//! 42 s: ICAO Annex 16 Vol II, Doc 9889). The flow is evaluated at the mean
//! roll speed (half the take-off segment speed for a uniform acceleration)
//! at the departure aerodrome. The OpenAP take-off statistics, lift-off
//! 85.3 m/s at a mean acceleration of 1.93 m/s^2 (A320), give 44 s, the same
//! order as the ICAO mode time; neither is validation of this aircraft.
//!
//! The allowance is a calibrated, labelled constant, not a derived roll
//! (no runway-acceleration model exists in the mission). The horizontal
//! distance of the roll is not removed from the route.

use alas_mass::fuel_plan::{FuelModelError, LegEstimate};
use alas_prop::system::PropulsionRating;

use super::adapt::Fixed;
use super::integrate::{FlownLeg, FlyError};
use super::profile::LegKind;
use super::SegmentMissionModel;

/// Fuel burned and time elapsed on the ground roll.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(super) struct Roll {
    /// Fuel, kg.
    pub fuel_kg: f64,
    /// Time, s.
    pub time_s: f64,
}

impl Roll {
    /// `flown`, with the roll's fuel and time counted in the leg and in the
    /// time and fuel to the top of the climb (both run from brake release).
    pub(super) fn added_to(self, flown: FlownLeg) -> FlownLeg {
        FlownLeg {
            leg: LegEstimate {
                fuel_kg: flown.leg.fuel_kg + self.fuel_kg,
                time_s: flown.leg.time_s + self.time_s,
            },
            climb_time_s: flown.climb_time_s + self.time_s,
            climb_fuel_kg: flown.climb_fuel_kg + self.fuel_kg,
            ..flown
        }
    }
}

impl SegmentMissionModel {
    /// Fly a leg at the given climb-rate scales and, for a trip, with the
    /// take-off ground roll in front of it (audit MIS-06); a diversion starts
    /// from a go-around in the air. Every flown trip, frozen, replayed or
    /// adapted, passes through here, so the plan and its replay agree.
    pub(super) fn fly_at_scales(
        &self,
        leg: LegKind,
        mass_kg: f64,
        range_m: f64,
        scales: [f64; 3],
        fixed: Option<Fixed<'_>>,
        plan_steps: bool,
    ) -> Result<(FlownLeg, f64), FlyError> {
        let roll = match leg {
            LegKind::Trip => self.takeoff_roll(mass_kg)?,
            LegKind::Diversion => Roll::default(),
        };
        let start_kg = mass_kg - roll.fuel_kg;
        self.fly_from_lift_off(leg, start_kg, range_m, scales, fixed, plan_steps)
            .map(|(flown, cruise_m)| (roll.added_to(flown), cruise_m))
    }

    /// The ground-roll allowance for a take-off at `mass_kg`; zero when the
    /// profile disables it (`takeoff_roll_time_s <= 0`).
    pub(super) fn takeoff_roll(&self, mass_kg: f64) -> Result<Roll, FuelModelError> {
        let time_s = self.profile.takeoff_roll_time_s;
        if !mass_kg.is_finite() || mass_kg <= 0.0 {
            return Err(FuelModelError::MassOutOfRange { mass_kg });
        }
        if !(time_s.is_finite() && time_s > 0.0) {
            return Ok(Roll::default());
        }
        let altitude_m = self.departure_elevation_m;
        let mean_speed_m_s = 0.5 * self.profile.takeoff_air_speed_m_s;
        let flight = self
            .propulsion
            .flight_condition(
                altitude_m,
                mean_speed_m_s,
                self.gravity_at(altitude_m),
                self.isa_deviation_c,
            )
            .map_err(super::deck_error)?;
        let point = self
            .propulsion
            .rated_point(flight, PropulsionRating::TakeoffGoAround)
            .map_err(super::deck_error)?;
        let fuel_kg = point.fuel_flow_kg_s * time_s;
        if !fuel_kg.is_finite() || fuel_kg < 0.0 || fuel_kg >= mass_kg {
            return Err(FuelModelError::NotConverged(format!(
                "take-off roll burn {fuel_kg} kg is unusable at {mass_kg} kg"
            )));
        }
        Ok(Roll { fuel_kg, time_s })
    }
}
