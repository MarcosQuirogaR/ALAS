// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! How a leg is adapted to the aircraft: the cruise level the policy picks,
//! the per-rung revision of a climb rate the aircraft cannot hold, and the
//! flown-altitude recovery when a level cannot be held or a rating-limited
//! ladder overruns the route.

use alas_config::mission::CruiseAltitudePolicy;
use alas_mass::fuel_plan::FuelModelError;

use super::climb_revision::{revise_from_hint, Hinted};
use super::integrate::{FlownLeg, FlyError};
use super::profile::{LegKind, ProfileGeometry};
use super::SegmentMissionModel;

/// Smallest commanded vertical rate that is still a climb, m/s.
///
/// The same floor `alas_pipeline::mission_stage::guidance` uses, for the same
/// reason: a zero-rate segment is not a climb and makes the altitude-time
/// differential singular.
pub(super) const MINIMUM_CLIMB_RATE_M_S: f64 = 0.5;

/// Coarse bracket scanned before the bisection, largest scale first. The
/// halving factor is the native guidance module's own; six steps reach
/// `2^-6`, below the floor scale of every shipped schedule.
pub(super) const CLIMB_RATE_BRACKET_FACTOR: f64 = 0.5;
pub(super) const CLIMB_RATE_BRACKET_STEPS: usize = 6;

/// Resolution of the commanded climb-rate scale, an order finer than the
/// rate schedule's own two significant figures, so the flown mission does
/// not depend on where the bisection stopped.
pub(super) const CLIMB_RATE_RESOLUTION: f64 = 1.0e-3;

/// Tolerance of the flown-altitude recovery, m: the bisection on the continuous
/// level stops when its bracket is narrower than this. The planned level is
/// the upper bracket, so the recovered level is the highest feasible level to
/// this tolerance, independent of any warm-start history.
const ALTITUDE_TOLERANCE_M: f64 = 1.0;

/// A flown leg and the inputs that reproduce it.
pub(super) struct Adapted {
    pub flown: FlownLeg,
    /// Cruise altitude the plan was built at, m.
    pub cruise_m: f64,
    /// Commanded-rate scales of the initial and the two step-climb rungs.
    pub scales: [f64; 3],
}

/// A fixed plan to fly without adaptation: cruise altitude and the step
/// positions (fractions of the cruise distance).
pub(super) type Fixed<'a> = (f64, &'a [f64]);

impl SegmentMissionModel {
    /// Fly a leg: from the frozen plan when one matches, otherwise adapted.
    pub(super) fn fly_leg(
        &self,
        leg: LegKind,
        mass_kg: f64,
        range_m: f64,
    ) -> Result<FlownLeg, FuelModelError> {
        self.check_cancelled()?;
        self.validate().map_err(FuelModelError::InvalidModel)?;
        if leg == LegKind::Trip {
            if let Some(result) = self.fly_frozen(mass_kg, range_m) {
                return result;
            }
        }
        self.fly_adapted(leg, mass_kg, range_m, false)
            .map(|adapted| adapted.flown)
    }

    /// Fly a leg, revising the commanded rate of each climb rung the
    /// aircraft cannot hold, and only that rung.
    ///
    /// A fixed climb rate is a guidance target, not an aircraft capability,
    /// and the native mission revises it the same way (halving down to a
    /// 0.5 m/s floor). Revising every en-route rung by one factor, as this
    /// model did, turned a single deficit at the top of climb into a 0.5 m/s
    /// crawl from the initial climb up: the A320-200 at 78 t "climbed" for
    /// 1,948 of 2,120 nmi and the B787-9 dispatch could not close. The
    /// revision now finds the largest scale of the failing rung alone that
    /// clears it; the extra time, distance and fuel are integrated and the
    /// leg is marked `adapted`. A leg that cannot climb at the floor is still
    /// rejected with its typed deficit.
    ///
    /// `plan_steps` plans step climbs, which only a frozen plan does: a step
    /// position is a discrete choice that would make trip fuel jump with
    /// mass inside a dispatch closure, so it is made once, like the step
    /// points of a filed flight plan, and an unfrozen trip holds its initial
    /// level.
    pub(super) fn fly_adapted_at_level(
        &self,
        leg: LegKind,
        mass_kg: f64,
        range_m: f64,
        plan_steps: bool,
    ) -> Result<Adapted, FuelModelError> {
        let mut scales = [1.0; 3];
        let mut revised = [false; 3];
        loop {
            self.check_cancelled()?;
            match self.fly_at_scales(leg, mass_kg, range_m, scales, None, plan_steps) {
                Ok((flown, cruise_m)) => {
                    let adapted = flown.adapted || revised.iter().any(|r| *r);
                    return Ok(Adapted {
                        flown: FlownLeg { adapted, ..flown },
                        cruise_m,
                        scales,
                    });
                }
                Err(FlyError::Climb { rung, error })
                    if (1..=3).contains(&rung) && !revised[rung - 1] =>
                {
                    scales[rung - 1] =
                        self.revise_rung(leg, mass_kg, range_m, scales, rung, &error, plan_steps)?;
                    revised[rung - 1] = true;
                }
                Err(error) => return Err(into_model_error(error, range_m)),
            }
        }
    }

    /// The largest commanded-rate scale of climb `rung` that clears it.
    ///
    /// The feasible set need not be an interval (a gentler climb lengthens
    /// the footprint and can force a lower level), so a geometric bracket is
    /// scanned largest first and the boundary inside it is then bisected,
    /// which keeps the flown mission continuous in the mass it is flown at.
    #[allow(clippy::too_many_arguments)] // the leg's inputs plus the rung under revision
    fn revise_rung(
        &self,
        leg: LegKind,
        mass_kg: f64,
        range_m: f64,
        scales: [f64; 3],
        rung: usize,
        deficit: &FuelModelError,
        plan_steps: bool,
    ) -> Result<f64, FuelModelError> {
        let p = &self.profile;
        let rate = [
            p.initial_climb_rate_m_s,
            p.step_climb_1_rate_m_s,
            p.step_climb_2_rate_m_s,
        ][rung - 1];
        let floor = (MINIMUM_CLIMB_RATE_M_S / rate).clamp(0.0, 1.0);
        // Ok(Some(true)) clears the rung, Ok(Some(false)) does not, Ok(None)
        // stops the revision (the gentler climb no longer fits the route).
        let clears = |scale: f64| -> Result<Option<bool>, FuelModelError> {
            let mut trial = scales;
            trial[rung - 1] = scale;
            match self.fly_at_scales(leg, mass_kg, range_m, trial, None, plan_steps) {
                Ok(_) => Ok(Some(true)),
                Err(FlyError::Climb { rung: other, .. }) => Ok(Some(other != rung)),
                Err(FlyError::TooShort { .. })
                | Err(FlyError::Fuel(FuelModelError::RouteTooShort { .. })) => Ok(None),
                Err(error) => Err(into_model_error(error, range_m)),
            }
        };
        let cannot_climb = || {
            FuelModelError::NotConverged(format!(
                "{deficit}; and at the gentlest commanded climb the schedule admits \
                 ({MINIMUM_CLIMB_RATE_M_S} m/s on climb rung {rung}) it still cannot climb"
            ))
        };
        self.check_cancelled()?;
        match self
            .memory
            .rung_scale(leg, rung)
            .and_then(|hint| revise_from_hint(hint, floor, &clears))
        {
            Some(Hinted::Scale(scale)) => {
                self.memory.set_rung_scale(leg, rung, scale);
                return Ok(scale);
            }
            Some(Hinted::CannotClimb) => return Err(cannot_climb()),
            None => {}
        }
        let mut bracket = None;
        let mut failed = 1.0;
        let mut scale = 1.0;
        for _ in 0..CLIMB_RATE_BRACKET_STEPS {
            self.check_cancelled()?;
            scale = (scale * CLIMB_RATE_BRACKET_FACTOR).max(floor);
            match clears(scale)? {
                Some(true) => {
                    bracket = Some((scale, failed));
                    break;
                }
                Some(false) => failed = scale,
                None => break,
            }
            if scale <= floor {
                break;
            }
        }
        let Some((mut low, mut high)) = bracket else {
            return Err(cannot_climb());
        };
        while high - low > CLIMB_RATE_RESOLUTION {
            self.check_cancelled()?;
            let middle = 0.5 * (low + high);
            if clears(middle)? == Some(true) {
                low = middle;
            } else {
                high = middle;
            }
        }
        self.memory.set_rung_scale(leg, rung, low);
        Ok(low)
    }

    /// Fly with revised climb `scales`, at the policy's cruise level or at
    /// `fixed`; returns the flown leg and the cruise altitude planned.
    ///
    /// Unfixed, a level the aircraft cannot hold or a rating-limited ladder
    /// that overruns the route lowers the level on a 1 m grid. Every other
    /// failure, and any failure of a fixed plan, returns.
    pub(super) fn fly_from_lift_off(
        &self,
        leg: LegKind,
        mass_kg: f64,
        range_m: f64,
        scales: [f64; 3],
        fixed: Option<Fixed<'_>>,
        plan_steps: bool,
    ) -> Result<(FlownLeg, f64), FlyError> {
        let mut profile = climb_rate_revision(&self.profile, scales);
        let geometry = ProfileGeometry {
            profile: &profile,
            ..self.geometry()
        };
        let optimum = self.profile.cruise_altitude_policy == CruiseAltitudePolicy::OptimumStep;
        let cruise_m = match fixed {
            Some((cruise_m, _)) => cruise_m,
            None => {
                let route_fit_m = geometry.plan(leg, range_m)?.planned_cruise_m;
                if optimum {
                    self.initial_cruise_level_m(&geometry, leg, mass_kg, range_m, route_fit_m)
                } else {
                    route_fit_m
                }
            }
        };
        // The first level of a route ladder is held to the same flyability rule.
        let initial_fraction = optimum
            .then(|| self.limited_initial_fraction(&geometry, leg, mass_kg, range_m, cruise_m))
            .flatten();
        if let Some(fraction) = initial_fraction {
            profile.initial_climb_altitude_fraction = fraction;
        }
        let geometry = ProfileGeometry {
            profile: &profile,
            ..self.geometry()
        };
        let steps_cap_m = (optimum && leg == LegKind::Trip && (plan_steps || fixed.is_some()))
            .then(|| {
                geometry.ladder_top_m(
                    self.departure_elevation_m,
                    geometry.configured_cruise_m_for(leg),
                )
            });
        let plan_at = |cruise_m: f64| -> Result<_, FlyError> {
            let mut plan =
                geometry
                    .plan_at(leg, range_m, cruise_m)
                    .map_err(|error| match error {
                        FuelModelError::RouteTooShort {
                            minimum_range_m, ..
                        } => FlyError::TooShort {
                            deficit_m: minimum_range_m - range_m,
                        },
                        error => FlyError::Fuel(error),
                    })?;
            if let Some(cap_m) = steps_cap_m {
                plan.step_levels = geometry.step_levels(&plan, cap_m)?;
            }
            Ok(plan)
        };
        let frozen_steps = fixed.map(|(_, steps)| steps);
        let first =
            plan_at(cruise_m).and_then(|plan| self.fly_counted(mass_kg, &plan, frozen_steps));
        match first {
            Ok(flown) => return Ok((flown, cruise_m)),
            Err(error) if fixed.is_some() || !is_flyerror_recoverable(&error) => return Err(error),
            Err(_) => {}
        }
        // Highest feasible level between the floor and `cruise_m`, found by
        // bisection on the continuous level to `ALTITUDE_TOLERANCE_M`. The earlier
        // search stepped a 1 m grid with a warm start; a continuous bracket gives
        // the same root without grid-step or bracket-history dependence.
        let attempt =
            |level_m: f64| plan_at(level_m).and_then(|plan| self.fly_counted(mass_kg, &plan, None));
        let floor_m = geometry.floor_cruise_m(leg).min(cruise_m);
        let mut low = floor_m;
        let mut best = attempt(low)?;
        let mut high = cruise_m;
        while high - low > ALTITUDE_TOLERANCE_M {
            self.check_cancelled()?;
            let middle = 0.5 * (low + high);
            match attempt(middle) {
                Ok(flown) => {
                    best = flown;
                    low = middle;
                }
                Err(error) if is_flyerror_recoverable(&error) => high = middle,
                Err(error) => return Err(error),
            }
        }
        // Under `OptimumStep` the recovered level is planned at a legal
        // flight level, as the initial-level choice is
        // (`cruise_levels::legal_level_at_or_below_m`): the legal levels at
        // or below the bisected one are tried downward to the floor of the
        // climb geometry, and the first that flies is kept. A legal level
        // that fails, for any reason, never discards the leg already flown:
        // only when no legal level above the floor flies does the
        // continuous level stay. A cancelled run still stops, as in the
        // bisection above.
        if optimum {
            let mut legal_m = super::cruise_levels::legal_level_at_or_below_m(low);
            while (floor_m..low).contains(&legal_m) {
                self.check_cancelled()?;
                if let Ok(flown) = attempt(legal_m) {
                    best = flown;
                    low = legal_m;
                    break;
                }
                legal_m = super::cruise_levels::legal_level_at_or_below_m(legal_m - 1.0);
            }
        }
        self.memory.set(leg, low.round() as i64);
        Ok((
            FlownLeg {
                adapted: true,
                ..best
            },
            low,
        ))
    }
}

/// Whether `error` is specifically a *level-flight* (cruise) rating shortfall
/// at the planned level: the one case where a lower level is ordinary
/// dispatch practice, not a relaxation. Climb and descent energy deficits and
/// every other typed rejection still surface directly.
fn is_altitude_recoverable(error: &FuelModelError) -> bool {
    matches!(
        error,
        FuelModelError::NotConverged(reason) if reason.contains("level flight energy deficit")
    )
}

/// Whether `error` is a climb-phase shortage of excess thrust, which a
/// slower commanded climb rate can pay for: `climb energy deficit` (the
/// commanded rate cannot be held) or `speed schedule not attained` (the
/// acceleration to the next commanded speed cannot be completed because the
/// same excess thrust is spent climbing). A descent deficit is a different
/// statement and is excluded.
pub(super) fn is_climb_rate_recoverable(error: &FuelModelError) -> bool {
    matches!(
        error,
        FuelModelError::NotConverged(reason)
            if reason.contains("climb energy deficit")
                || reason.contains("speed schedule not attained")
    )
}

/// A route-fit shortfall or a level-flight shortfall: worth a lower level.
fn is_flyerror_recoverable(error: &FlyError) -> bool {
    match error {
        FlyError::TooShort { .. } => true,
        FlyError::Fuel(reason) => is_altitude_recoverable(reason),
        FlyError::Climb { .. } => false,
    }
}

/// Fold a [`FlyError`] back into the public error type.
pub(super) fn into_model_error(error: FlyError, range_m: f64) -> FuelModelError {
    match error {
        FlyError::Fuel(error) | FlyError::Climb { error, .. } => error,
        FlyError::TooShort { deficit_m } => FuelModelError::RouteTooShort {
            range_m,
            minimum_range_m: range_m + deficit_m,
        },
    }
}

/// The en-route climb rates of `profile`, each scaled by its own entry of
/// `scales` (initial, step one, step two) and floored at
/// [`MINIMUM_CLIMB_RATE_M_S`].
///
/// The take-off rung's rate never moves: it is flown low and slow with the
/// most excess thrust an aeroplane ever has, and it spans an absolute height
/// that does not shrink with the cruise level, so slowing it only makes
/// short sectors unflyable. Speeds, altitudes, distance fractions and every
/// descent rate stay exactly as configured.
pub(super) fn climb_rate_revision(
    profile: &alas_config::MissionProfileConfig,
    scales: [f64; 3],
) -> alas_config::MissionProfileConfig {
    let mut revised = profile.clone();
    for (rate, scale) in [
        &mut revised.initial_climb_rate_m_s,
        &mut revised.step_climb_1_rate_m_s,
        &mut revised.step_climb_2_rate_m_s,
    ]
    .into_iter()
    .zip(scales)
    {
        if scale < 1.0 && rate.is_finite() && *rate > 0.0 {
            *rate = (*rate * scale).max(MINIMUM_CLIMB_RATE_M_S);
        }
    }
    revised
}
