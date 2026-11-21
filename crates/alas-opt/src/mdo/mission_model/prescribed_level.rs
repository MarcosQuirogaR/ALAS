// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A prescribed cruise level (`Declared`, `Design`) the aircraft cannot climb
//! to at the mass it is flown at.
//!
//! The `OptimumStep` policy chooses its own level, the highest with a residual
//! climb of 300 ft/min at top-of-climb mass, so it always has one. A prescribed
//! level is the user's, and at a heavy dispatch mass the aircraft may not be
//! able to reach it at all: the rung that climbs to it cannot hold even the
//! gentlest commanded climb the schedule admits (`adapt::revise_rung`). The
//! sizing closure iterates the take-off mass, so it meets such masses on its
//! way to the fixed point; the dispatch then bisects the take-off mass down to
//! the heaviest mass the model can fly at all and reports an unconverged
//! closure (A320-class `SUAVE` provenance mission with `Declared` FL390: the
//! model flies up to 358,670 kg and the mission needs 17,409 kg more).
//!
//! A prescribed level is the ceiling the user works below, as the level-flight
//! shortfall already treats it (`adapt::is_altitude_recoverable`: "a lower level is
//! ordinary dispatch practice, not a relaxation"). The
//! trip is flown at the highest level, on a 1 m grid, whose ladder the aircraft can
//! climb (a flight-level scan down from the prescribed level and a bisection inside
//! the last interval, so the level is continuous in the mass), the leg is marked
//! `adapted`, and the plausibility tier reports declared against flown level
//! (`PRICED-DECLARED-LEVEL`). The sizing closure converges. There is no hard-limit
//! policy.

use alas_config::mission::CruiseAltitudePolicy;
use alas_mass::fuel_plan::FuelModelError;

use super::adapt::Adapted;
use super::cruise_levels::LEVEL_SCAN_STEP_M;
use super::integrate::FlownLeg;
use super::profile::LegKind;
use super::SegmentMissionModel;

/// Resolution of the lowered level, m (the grid of the level-flight recovery).
const LEVEL_GRID_M: f64 = 1.0;

/// Whether `error` is the typed statement that a climb rung cannot climb even
/// at the gentlest commanded rate (`adapt::revise_rung`).
fn is_unattainable_level(error: &FuelModelError) -> bool {
    matches!(error, FuelModelError::NotConverged(reason) if reason.contains("it still cannot climb"))
}

impl SegmentMissionModel {
    /// Fly a leg, revising the commanded climb rates (`fly_adapted_at_level`);
    /// a prescribed level the aircraft cannot climb to is lowered to the highest
    /// one it can.
    pub(super) fn fly_adapted(
        &self,
        leg: LegKind,
        mass_kg: f64,
        range_m: f64,
        plan_steps: bool,
    ) -> Result<Adapted, FuelModelError> {
        let error = match self.fly_adapted_at_level(leg, mass_kg, range_m, plan_steps) {
            Err(error)
                if self.profile.cruise_altitude_policy != CruiseAltitudePolicy::OptimumStep
                    && is_unattainable_level(&error) =>
            {
                error
            }
            other => return other,
        };
        self.fly_below_prescribed_level(leg, mass_kg, range_m, plan_steps)?
            .ok_or(error)
    }

    /// The level the policy prescribes, m.
    fn prescribed_level_m(&self) -> f64 {
        match self.profile.cruise_altitude_policy {
            CruiseAltitudePolicy::Design => self.design_cruise_altitude_m,
            _ => self.cruise_altitude_m,
        }
    }

    /// The model with its prescribed level at `level_m`.
    fn at_prescribed_level(&self, level_m: f64) -> Self {
        let mut model = self.clone();
        match self.profile.cruise_altitude_policy {
            CruiseAltitudePolicy::Design => model.design_cruise_altitude_m = level_m,
            _ => model.cruise_altitude_m = level_m,
        }
        model
    }

    /// The trip at the highest level below the prescribed one the aircraft can
    /// climb to; `None` when no level down to the floor can be climbed to.
    fn fly_below_prescribed_level(
        &self,
        leg: LegKind,
        mass_kg: f64,
        range_m: f64,
        plan_steps: bool,
    ) -> Result<Option<Adapted>, FuelModelError> {
        let attempt = |level_m: f64| -> Result<Option<Adapted>, FuelModelError> {
            self.check_cancelled()?;
            match self
                .at_prescribed_level(level_m)
                .fly_adapted_at_level(leg, mass_kg, range_m, plan_steps)
            {
                Ok(adapted) => Ok(Some(adapted)),
                // Only a level the aircraft cannot climb to is scanned past. Any other failure
                // (a leg that did not converge, a rejected condition, a cancellation) is the
                // answer for this trip and is returned, not read as "unattainable".
                Err(error) if is_unattainable_level(&error) => Ok(None),
                Err(error) => Err(error),
            }
        };
        let floor_m = self.geometry().floor_cruise_m(leg);
        let mut high_m = self.prescribed_level_m();
        // Flight-level scan down from the prescribed level.
        let (mut best, mut low_m) = loop {
            let level_m = (high_m - LEVEL_SCAN_STEP_M).max(floor_m);
            if let Some(adapted) = attempt(level_m)? {
                break (adapted, level_m);
            }
            if level_m <= floor_m {
                return Ok(None);
            }
            high_m = level_m;
        };
        while high_m - low_m > LEVEL_GRID_M {
            let middle_m = 0.5 * (low_m + high_m);
            match attempt(middle_m)? {
                Some(adapted) => {
                    best = adapted;
                    low_m = middle_m;
                }
                None => high_m = middle_m,
            }
        }
        best.flown = FlownLeg {
            adapted: true,
            ..best.flown
        };
        Ok(Some(best))
    }
}
