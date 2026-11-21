// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The speed change at the entry of a climb rung, flown as the native flight flies it.
//!
//! Where the planned ladder asks for a faster rung than the aeroplane has (the first rung after
//! the take-off band, the rung above the 10,000 ft restriction, a step climb), the native
//! junction logic cuts a path piece (`*_path_entry`): an acceleration in the climb with BADA's
//! energy share, at the maximum-climb rating (the take-off rating in the take-off band), that
//! gains the height the speed change allows; it flies the change level only where the engine
//! cannot climb at the minimum path-change rate. The priced route model flies the same piece
//! ([`SegmentMissionModel::climbing_acceleration`]) and the same level fallback, so the two
//! models charge the same leg. A speed change below the minimum
//! ([`alas_config::mission::MINIMUM_SPEED_CHANGE_M_S`]) is left to the rung's own kinetic budget.

use alas_config::mission::{MINIMUM_PATH_CHANGE_RATE_M_S, MINIMUM_SPEED_CHANGE_M_S};
use alas_mass::fuel_plan::FuelModelError;
use alas_prop::system::PropulsionRating;

use super::trace::{piece_forces, FlownStep, StepPhase};
use super::{FlyError, Integrator, ProfilePlan, Segment, SegmentKind};
use crate::mdo::mission_model::level_acceleration::LevelAcceleration;
use crate::mdo::mission_model::takeoff_acceleration::{Acceleration, ClimbingAcceleration};

/// Passes at fixing the speed the piece ends at (the end altitude depends on the target speed,
/// and the target is the speed of the rung the piece ends in).
const RUNG_ITERATIONS: usize = 8;

/// Speed change between two passes below which the end speed is settled, m/s.
const SETTLED_M_S: f64 = 0.01;

/// What the entry piece flew.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::mdo::mission_model) struct ClimbEntry {
    /// Horizontal distance flown, m.
    pub distance_m: f64,
    /// Altitude it ended at, m (the altitude it started at for a level change).
    pub end_altitude_m: f64,
}

impl Integrator<'_> {
    /// The speed the aeroplane has: the last target less what is still unpaid.
    pub(super) fn actual_speed_m_s(&self) -> Option<f64> {
        let target = self.previous_tas_m_s?;
        let kinetic_j = 0.5 * self.mass_kg * target * target - self.pending_kinetic_j;
        Some((2.0 * kinetic_j / self.mass_kg).max(0.0).sqrt())
    }

    /// Fly the speed change that opens `rungs[0]` (a climb or take-off rung that starts where the
    /// aeroplane is) when it is faster than the aeroplane by more than the minimum speed change.
    ///
    /// The change is a path piece that ends in the rung whose speed it ends at (`rungs` are the
    /// rungs from this one on); or, where the rating cannot climb the aeroplane at the minimum
    /// path-change rate, a level change at the rung's start. `None` when no change is flown here:
    /// no speed change, or a piece the model cannot fly either way (the change stays in the rung's
    /// kinetic budget).
    ///
    /// # Errors
    ///
    /// A propulsion or polar failure, a cancelled evaluation, or a piece whose end speed does not
    /// settle on a rung.
    pub(super) fn enter_climb_rung(
        &mut self,
        previous: Option<&Segment>,
        rungs: &[Segment],
    ) -> Result<Option<ClimbEntry>, FlyError> {
        let (Some(previous), Some(rung)) = (previous, rungs.first()) else {
            return Ok(None);
        };
        let climbs = |kind: SegmentKind| matches!(kind, SegmentKind::Takeoff | SegmentKind::Climb);
        if !climbs(previous.kind) || !climbs(rung.kind) {
            return Ok(None);
        }
        let Some(actual_m_s) = self.actual_speed_m_s() else {
            return Ok(None);
        };
        // Where the previous rung ends at the altitude this one starts at, the aeroplane has the
        // speed that rung's schedule gives there (not the mid-rung speed the rung is flown at).
        let from_m_s = Some(previous)
            .filter(|previous| (previous.end_altitude_m - rung.start_altitude_m).abs() < 1.0e-6)
            .map_or(actual_m_s, |previous| {
                self.model
                    .scheduled_speed_m_s(previous, rung.start_altitude_m)
            });
        if rung.tas_m_s - from_m_s <= MINIMUM_SPEED_CHANGE_M_S {
            return Ok(None);
        }
        let takeoff = rung.kind == SegmentKind::Takeoff;
        let rating = if takeoff {
            PropulsionRating::TakeoffGoAround
        } else {
            PropulsionRating::MaximumClimb
        };
        let plan = |to_m_s: f64| ClimbingAcceleration {
            altitude_m: rung.start_altitude_m,
            from_m_s,
            to_m_s,
            rating,
            high_lift: takeoff,
            planned_rate_m_s: rung.vertical_rate_m_s,
            minimum_rate_m_s: MINIMUM_PATH_CHANGE_RATE_M_S,
        };
        let mut target_m_s = rung.tas_m_s;
        let mut settled = None;
        let mut tried: Vec<f64> = Vec::with_capacity(RUNG_ITERATIONS);
        for _ in 0..RUNG_ITERATIONS {
            tried.push(target_m_s);
            let Some(piece) = self
                .model
                .climbing_acceleration(self.mass_kg, &plan(target_m_s))?
            else {
                return self.enter_level(rung, from_m_s, rating);
            };
            let end_m = piece.altitude_end_m;
            let Some(end_rung) = rungs.iter().find(|r| r.end_altitude_m > end_m) else {
                // The piece would run past the top of the ladder: the change stays in the rung.
                return Ok(None);
            };
            let next_m_s = self
                .model
                .scheduled_speed_m_s(end_rung, end_m)
                .max(from_m_s);
            if (next_m_s - target_m_s).abs() <= SETTLED_M_S {
                settled = Some(piece);
                break;
            }
            // The rung speeds step at the rung boundaries, so the end speed can have no fixed
            // point: the target ends the piece in a rung whose speed ends it in the rung below,
            // and the other way round (a 2-cycle across a boundary). The piece then ends at the
            // lower of the two speeds; the rest of the change is paid in the rung it opens.
            if tried.iter().any(|t| (t - next_m_s).abs() <= SETTLED_M_S) {
                let lower = next_m_s.min(target_m_s);
                if let Some(piece) = self
                    .model
                    .climbing_acceleration(self.mass_kg, &plan(lower))?
                {
                    target_m_s = lower;
                    settled = Some(piece);
                }
                break;
            }
            target_m_s = next_m_s;
        }
        let Some(piece) = settled else {
            return Err(FuelModelError::NotConverged(format!(
                "the climb entry acceleration from {from_m_s:.2} m/s at {:.0} m did not settle on a ladder rung in {RUNG_ITERATIONS} passes",
                rung.start_altitude_m
            ))
            .into());
        };
        let phase = if takeoff {
            StepPhase::Takeoff
        } else {
            StepPhase::Climb
        };
        self.book_acceleration(&piece, phase, rung.start_altitude_m, target_m_s, false);
        Ok(Some(ClimbEntry {
            distance_m: piece.distance_m,
            end_altitude_m: piece.altitude_end_m,
        }))
    }

    /// The level change at the rung's start (at the civil acceleration limit), where the rating
    /// cannot climb at the minimum rate. `None` where it cannot accelerate level either.
    fn enter_level(
        &mut self,
        rung: &Segment,
        from_m_s: f64,
        rating: PropulsionRating,
    ) -> Result<Option<ClimbEntry>, FlyError> {
        let takeoff = rung.kind == SegmentKind::Takeoff;
        let level = self.model.level_acceleration(
            self.mass_kg,
            LevelAcceleration {
                altitude_m: rung.start_altitude_m,
                from_m_s,
                to_m_s: rung.tas_m_s,
                rating,
                delta_cd: if takeoff {
                    self.model.phase_limits.high_lift_delta_cd
                } else {
                    0.0
                },
                high_lift: takeoff,
            },
        );
        let level = match level {
            Ok(level) => level,
            Err(FuelModelError::Cancelled) => return Err(FuelModelError::Cancelled.into()),
            // It cannot be flown level either: the change stays in the rung's kinetic budget.
            Err(_) => return Ok(None),
        };
        let phase = if takeoff {
            StepPhase::Takeoff
        } else {
            StepPhase::Climb
        };
        let mut level = level;
        level.altitude_end_m = rung.start_altitude_m;
        self.book_acceleration(&level, phase, rung.start_altitude_m, rung.tas_m_s, true);
        Ok(Some(ClimbEntry {
            distance_m: level.distance_m,
            end_altitude_m: rung.start_altitude_m,
        }))
    }

    /// Charge `acceleration` (flown from `start_altitude_m` and the aeroplane's speed to
    /// `target_m_s`) to the flight and leave the aeroplane at the target speed with nothing unpaid.
    fn book_acceleration(
        &mut self,
        acceleration: &Acceleration,
        phase: StepPhase,
        start_altitude_m: f64,
        target_m_s: f64,
        level: bool,
    ) {
        let start = (self.time_s, self.distance_m);
        let start_mass_kg = self.mass_kg;
        let mut shown = *acceleration;
        shown.v2_m_s = self.actual_speed_m_s().unwrap_or(acceleration.v2_m_s);
        self.mass_kg -= acceleration.fuel_kg;
        self.fuel_kg += acceleration.fuel_kg;
        self.time_s += acceleration.time_s;
        self.distance_m += acceleration.distance_m;
        self.ledger.propulsive_work_j += acceleration.ledger.propulsive_work_j;
        self.ledger.drag_work_j += acceleration.ledger.drag_work_j;
        self.ledger.potential_energy_j += acceleration.ledger.potential_energy_j;
        self.ledger.kinetic_energy_j += acceleration.ledger.kinetic_energy_j;
        self.pending_kinetic_j = 0.0;
        self.previous_tas_m_s = Some(target_m_s);
        let end_altitude_m = if level {
            start_altitude_m
        } else {
            shown.altitude_end_m
        };
        let forces = piece_forces(
            self.model,
            &shown.ledger,
            [start_altitude_m, end_altitude_m],
            shown.distance_m,
            [shown.v2_m_s, target_m_s],
            [start_mass_kg, self.mass_kg],
        );
        let step = if level {
            FlownStep::level_acceleration(
                phase,
                start_altitude_m,
                [start.0, self.time_s],
                [start.1, self.distance_m],
                [shown.v2_m_s, target_m_s],
                [start_mass_kg, self.mass_kg],
                self.model.isa_deviation_c,
            )
        } else {
            FlownStep::acceleration(
                phase,
                start_altitude_m,
                start,
                start_mass_kg,
                &shown,
                target_m_s,
            )
        };
        self.trace.push(step.with_forces(forces));
    }
}

impl Integrator<'_> {
    /// Fly the climb ladder of `plan` after the take-off climb (which ended at `takeoff_end_m` at
    /// `takeoff_tas_m_s`), and return the horizontal distance flown, m.
    ///
    /// After the take-off climb the band keeps its speed for a short stretch (a piece shorter than
    /// the junction logic's minimum is not cut off): the native flight raises the band's top to
    /// that height when its own does not reach it. The rungs below the height reached are already
    /// flown, a rung climbed into is flown from there, and the speed change that opens each rung
    /// is a path piece (or a level change) as the native flight flies it.
    ///
    /// # Errors
    ///
    /// A failure of a rung, tagged with the rung that owns an unpaid speed change.
    pub(super) fn fly_climb_ladder(
        &mut self,
        plan: &ProfilePlan,
        takeoff_end_m: f64,
        takeoff_tas_m_s: f64,
    ) -> Result<f64, FlyError> {
        let mut footprint_m = 0.0;
        let mut rungs = plan.climb.clone();
        if let (true, Some(first)) = (takeoff_end_m > 0.0, rungs.first().copied()) {
            if let Some(stretch) = Segment::vertical(
                first.kind,
                takeoff_end_m,
                takeoff_end_m + alas_config::mission::TAKEOFF_BAND_STRETCH_M,
                takeoff_tas_m_s,
                first.vertical_rate_m_s.abs(),
            )? {
                rungs.insert(0, stretch);
            }
        }
        let mut previous: Option<Segment> = None;
        // The highest altitude the flight has reached: the rungs below it are already flown.
        let mut reached_m = takeoff_end_m;
        let mut index = 0;
        let mut entries = 0;
        while index < rungs.len() {
            let mut segment = rungs[index];
            index += 1;
            if segment.start_altitude_m < reached_m {
                if segment.end_altitude_m <= reached_m {
                    continue;
                }
                segment = segment.starting_at(reached_m);
                // What is left of a rung the flight has climbed into is flown only if it is a
                // piece the steps can resolve; the speed change of a shorter one is paid in the
                // rung that follows.
                if segment.end_altitude_m - segment.start_altitude_m
                    < alas_config::mission::TAKEOFF_BAND_STRETCH_M
                {
                    continue;
                }
            }
            // A piece that gains height leaves the rungs it passes flown and the rung it ends in
            // to be flown from there.
            let ahead: Vec<Segment> = std::iter::once(segment)
                .chain(rungs[index..].iter().copied())
                .collect();
            if let Some(entry) = self.enter_climb_rung(previous.as_ref(), &ahead)? {
                footprint_m += entry.distance_m;
                entries += 1;
                if entry.end_altitude_m > segment.start_altitude_m + 1.0e-9 && entries < 64 {
                    reached_m = entry.end_altitude_m;
                    index -= 1;
                    continue;
                }
            }
            // An unpaid speed change belongs to the rung that held it.
            let owner = previous
                .filter(|_| self.carries_unpaid_change_to(segment.tas_m_s))
                .unwrap_or(segment);
            footprint_m += self
                .fly_segment(&segment)
                .map_err(|error| plan.tag_climb_failure(&owner, error))?;
            reached_m = reached_m.max(segment.end_altitude_m);
            previous = Some(segment);
        }
        Ok(footprint_m)
    }
}
