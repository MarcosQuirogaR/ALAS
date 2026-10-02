// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The flown cruise: level segments between speed targets, the step climbs
//! the cruise-level rule (`cruise_levels`) qualifies, and the descent from
//! the level reached.

use alas_prop::system::PropulsionRating;

use super::cruise_levels::{
    CruiseOutcome, CRUISE_EPSILON_M, RESIDUAL_CLIMB_M_S, STEP_DECISION_INTERVAL_M,
};
use super::integrate::{FlyError, Integrator};
use super::profile::{ProfilePlan, Segment, SegmentKind};

impl Integrator<'_> {
    /// Whether opening a target of `tas_m_s` is rejected for a speed change
    /// still unpaid from the previous segment (`open_transition`'s boundary
    /// rule). In a climb that deficit belongs to the rung that held the
    /// budget, whose gentler commanded rate buys the time to pay it, not to
    /// the rung about to fly.
    pub(super) fn carries_unpaid_change_to(&self, tas_m_s: f64) -> bool {
        self.previous_tas_m_s.is_some_and(|previous| {
            self.pending_kinetic_j.abs() > self.transition_tolerance_j(previous, tas_m_s)
                && self.target_is_distinct(previous, tas_m_s)
        })
    }

    /// Additional level distance required to close a material speed transition
    /// at the current bound. The cruise scheduler calls this before a distinct
    /// target or after the final cruise rung; consecutive rungs at one target
    /// are allowed to share the same budget. A route attempt may lower cruise
    /// altitude and retry when the complete cruise allocation leaves too
    /// little distance; direct callers retain the strict boundary error in
    /// `open_transition`.
    fn level_transition_deficit_m(&mut self, segment: &Segment) -> Result<Option<f64>, FlyError> {
        let speed = segment.tas_m_s;
        let pending = self.pending_kinetic_j;
        let previous = self.previous_tas_m_s.unwrap_or(speed);
        let tolerance_j = self.transition_tolerance_j(previous, speed);
        if !pending.is_finite() || pending.abs() <= tolerance_j {
            return Ok(None);
        }
        let flight = self.model.propulsion.flight_condition(
            segment.end_altitude_m,
            speed,
            self.model.gravity_m_s2,
            self.model.isa_deviation_c,
        )?;
        let drag_n = self.phase_drag_n(SegmentKind::Cruise, &flight, self.mass_kg)?;
        let rating = if pending > 0.0 {
            PropulsionRating::Cruise
        } else {
            PropulsionRating::FlightIdle
        };
        let point = self.memo_point(&flight, rating)?;
        let excess_power_w = (point.thrust_n - drag_n) * speed;
        if !excess_power_w.is_finite() || pending * excess_power_w <= 0.0 {
            return Ok(None);
        }
        let deficit_m = pending.abs() * speed / excess_power_w.abs();
        if !deficit_m.is_finite() || deficit_m <= 0.0 {
            return Ok(None);
        }
        Ok(Some(deficit_m))
    }

    /// Distance into the next `chunk_m` of cruise at which the next level
    /// qualifies, interpolated between the masses at the chunk's two ends;
    /// `None` when it does not qualify within the chunk. Interpolating keeps
    /// the step position, and so trip fuel, continuous in the start mass.
    fn step_lookahead(
        &self,
        plan: &ProfilePlan,
        level: usize,
        rung: usize,
        chunk_m: f64,
    ) -> Option<f64> {
        let (altitude_m, rungs, _) = plan.level(level);
        let (next_m, next_rungs, _) = plan.level(level + 1);
        let (speed, next_speed) = (rungs[rung].0, next_rungs[rung].0);
        let model = self.model;
        // Residual-climb margin and relative specific-air-range gain.
        let margins = |mass_kg: f64| -> Option<(f64, f64)> {
            let here = model
                .specific_air_range_m_kg(mass_kg, altitude_m, speed)
                .ok()?;
            let there = model
                .specific_air_range_m_kg(mass_kg, next_m, next_speed)
                .ok()?;
            let residual = model.residual_climb_m_s(mass_kg, next_m, next_speed).ok()?;
            Some((residual - RESIDUAL_CLIMB_M_S, there / here - 1.0))
        };
        let start_kg = self.mass_kg;
        let end_kg = start_kg
            - chunk_m
                / model
                    .specific_air_range_m_kg(start_kg, altitude_m, speed)
                    .ok()?;
        let (start, end) = (margins(start_kg)?, margins(end_kg)?);
        let crossing = |at_start: f64, at_end: f64| {
            if at_start >= 0.0 {
                Some(0.0)
            } else if at_end >= 0.0 {
                Some(at_start / (at_start - at_end))
            } else {
                None
            }
        };
        Some(crossing(start.0, end.0)?.max(crossing(start.1, end.1)?) * chunk_m)
    }

    /// Whether to start the next step climb at `flown_m` of the cruise.
    ///
    /// Frozen: exactly at the recorded positions. Otherwise when `scheduled`
    /// by the lookahead, or when the next level keeps the residual-climb
    /// margin at the current mass and improves the specific air range at the
    /// same rung; in every case only if the step climb (and, for a new step,
    /// the longer descent) still fits in the remaining cruise distance.
    #[allow(clippy::too_many_arguments)] // the cruise loop's state, read once
    fn step_due(
        &self,
        plan: &ProfilePlan,
        level: usize,
        rung: usize,
        flown_m: f64,
        cruise_distance_m: f64,
        frozen: Option<&[f64]>,
        taken: usize,
        scheduled: bool,
    ) -> bool {
        // A step opens a new speed target, so it waits until the current
        // one is attained (frozen positions are deferred the same way).
        let previous = self.previous_tas_m_s.unwrap_or(0.0);
        let pending_paid = self.pending_kinetic_j.abs()
            <= self.transition_tolerance_j(previous, plan.level(level).1[rung].0);
        if level >= plan.step_levels.len() || !pending_paid {
            return false;
        }
        let (altitude_m, rungs, descent) = plan.level(level);
        let (next_m, next_rungs, next_descent) = plan.level(level + 1);
        let (speed, next_speed) = (rungs[rung].0, next_rungs[rung].0);
        // Fit with the slower of the commanded rate and the qualifying
        // residual, so a rating-limited step cannot overrun the cruise.
        let rate = self
            .model
            .profile
            .step_climb_2_rate_m_s
            .min(RESIDUAL_CLIMB_M_S);
        let climb_m = (next_m - altitude_m) / rate * (next_speed * next_speed - rate * rate).sqrt();
        let footprint = |segments: &[Segment]| -> f64 {
            segments.iter().map(|s| s.horizontal_distance_m).sum()
        };
        // A new step must leave the longer descent and one decision interval
        // of cruise at the new level; a frozen step need only fit, since its
        // cruise distance already excludes the longer descent. The gap
        // between the two keeps the descent fixed point from toggling a
        // marginal step on and off.
        let margin_m = if frozen.is_some() {
            0.0
        } else {
            footprint(next_descent) - footprint(descent) + STEP_DECISION_INTERVAL_M
        };
        if climb_m + margin_m > cruise_distance_m - flown_m {
            return false;
        }
        if let Some(frozen) = frozen {
            return frozen
                .get(taken)
                .is_some_and(|&p| p * cruise_distance_m <= flown_m + CRUISE_EPSILON_M);
        }
        if scheduled {
            return true;
        }
        let model = self.model;
        model.level_qualifies(self.mass_kg, next_m, next_speed)
            && matches!(
                (
                    model.specific_air_range_m_kg(self.mass_kg, altitude_m, speed),
                    model.specific_air_range_m_kg(self.mass_kg, next_m, next_speed),
                ),
                (Ok(here), Ok(there)) if there > here
            )
    }

    /// Fly the cruise over `cruise_distance_m`, with step climbs when the
    /// plan carries step levels, then the descent ladder from the level
    /// reached.
    pub(super) fn fly_cruise_and_descent(
        &mut self,
        plan: &ProfilePlan,
        cruise_distance_m: f64,
        frozen: Option<&[f64]>,
    ) -> Result<CruiseOutcome, FlyError> {
        let stepping = !plan.step_levels.is_empty();
        let fraction_sum: f64 = plan.cruise_rungs.iter().map(|(_, f)| f).sum();
        let mut level = 0;
        let mut flown_m = 0.0;
        let mut rung_end_m = 0.0;
        let mut step_positions = Vec::new();
        let mut last_cruise_segment: Option<Segment> = None;
        let mut scheduled = false;
        for (rung, &(_, fraction)) in plan.cruise_rungs.iter().enumerate() {
            rung_end_m += cruise_distance_m * fraction / fraction_sum;
            while rung_end_m - flown_m > CRUISE_EPSILON_M {
                if stepping
                    && self.step_due(
                        plan,
                        level,
                        rung,
                        flown_m,
                        cruise_distance_m,
                        frozen,
                        step_positions.len(),
                        scheduled,
                    )
                {
                    scheduled = false;
                    let (altitude_m, _, _) = plan.level(level);
                    let (next_m, next_rungs, _) = plan.level(level + 1);
                    let speed = next_rungs[rung].0;
                    let rate = self.model.profile.step_climb_2_rate_m_s;
                    step_positions.push(flown_m / cruise_distance_m.max(CRUISE_EPSILON_M));
                    if let Some(climb) =
                        Segment::vertical(SegmentKind::Climb, altitude_m, next_m, speed, rate)?
                    {
                        flown_m += self.fly_segment(&climb)?;
                    }
                    level += 1;
                    last_cruise_segment = None;
                    continue;
                }
                let (altitude_m, rungs, _) = plan.level(level);
                let speed = rungs[rung].0;
                let mut distance_m = rung_end_m - flown_m;
                scheduled = false;
                if stepping {
                    distance_m = distance_m.min(STEP_DECISION_INTERVAL_M);
                    let to_step_m = match frozen {
                        Some(frozen) => frozen
                            .get(step_positions.len())
                            .map(|p| p * cruise_distance_m - flown_m),
                        None if level < plan.step_levels.len() => {
                            self.step_lookahead(plan, level, rung, distance_m)
                        }
                        None => None,
                    };
                    if let Some(to_step_m) = to_step_m.filter(|d| *d > CRUISE_EPSILON_M) {
                        if to_step_m < distance_m {
                            distance_m = to_step_m;
                            scheduled = frozen.is_none();
                        }
                    }
                }
                let segment = Segment::level(SegmentKind::Cruise, altitude_m, speed, distance_m);
                // A speed target may be split across several cruise
                // segments. Let a material boundary-energy budget continue
                // through consecutive segments at the same target; only
                // require it to close before the next distinct target. The
                // first cruise segment checks the speed the climb ended at:
                // a ladder too short to pay its own acceleration is a route
                // shortfall, not a climb the aircraft cannot fly.
                if self.allow_route_transition_deficit {
                    let previous = last_cruise_segment.or_else(|| {
                        self.previous_tas_m_s
                            .map(|tas| Segment::level(SegmentKind::Cruise, altitude_m, tas, 0.0))
                    });
                    if let Some(previous) = previous {
                        if self.target_is_distinct(previous.tas_m_s, speed) {
                            if let Some(deficit_m) = self.level_transition_deficit_m(&previous)? {
                                return Err(FlyError::TooShort { deficit_m });
                            }
                        }
                    }
                }
                self.fly_segment(&segment)?;
                flown_m += distance_m;
                last_cruise_segment = Some(segment);
            }
        }
        // The descent begins at a distinct speed target. If the final cruise
        // target still has unpaid kinetic energy, report the extra horizontal
        // distance needed to close that boundary rather than letting the
        // descent ladder misstate the failure.
        if self.allow_route_transition_deficit {
            let (altitude_m, _, _) = plan.level(level);
            let last = last_cruise_segment.or_else(|| {
                self.previous_tas_m_s
                    .map(|tas| Segment::level(SegmentKind::Cruise, altitude_m, tas, 0.0))
            });
            if let Some(last) = last {
                if let Some(deficit_m) = self.level_transition_deficit_m(&last)? {
                    return Err(FlyError::TooShort { deficit_m });
                }
            }
        }
        let (final_altitude_m, _, descent) = plan.level(level);
        let mut descent_footprint_m = 0.0;
        for segment in descent {
            descent_footprint_m += self.fly_segment(segment)?;
        }
        Ok(CruiseOutcome {
            descent_footprint_m,
            final_altitude_m,
            step_positions,
            cruise_flown_m: flown_m,
        })
    }
}
