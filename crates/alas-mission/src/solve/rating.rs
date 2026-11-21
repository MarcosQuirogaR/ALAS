// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A climb flown at the maximum-climb rating (`SegmentSpec::rating_limited`): its vertical rate
//! is settled point by point on the rating after the first solve of [`super::converge_root`].

use alas_math::hybrd::{self, Settings};

use super::{MissionError, SegmentSolution};
use crate::envelope::ENVELOPE_TOLERANCE;
use crate::segments::{MissionAnalyses, Segment};

/// Most passes of the total-energy update a rating-limited climb is given.
const MAX_RATING_PASSES: usize = 40;

/// How close, as a fraction of the rate, a converged pass that leaves the rating unspent must come
/// to the upper end a failed solve left before that end is lifted: the bisection towards it then
/// stops short of it by no more than this.
const REFUSED_LIFT_FRACTION: f64 = 1.0e-3;

/// The rest of [`converge_root`] for a rating-limited climb: the vertical rate at each point is
/// moved until the command the force balance asks for is the rating there, or the point holds
/// the floor `MIN_RATED_CLIMB_RATE_M_S` above it (the caller's ceiling). The leg's scheduled
/// rate is only the first guess: a climb at the maximum-climb rating climbs at whatever rate
/// its excess power gives, below 95 % of the airspeed (a flight path angle under 72 deg).
///
/// The command is a signed fraction of the maximum-climb force, so at a solved command `t` with
/// force `F` the rating's force is `|F| / |t|`, and the rate the surplus buys is its specific
/// excess power times the BADA energy share, `f (F/t - F) V / (m g)` with
/// `f = [1 + (V / g) dV/dh]^-1` along the leg's speed schedule (BADA 3.8 User Manual Sec. 3.2,
/// eq. 3.2-5). That step is safeguarded per point by the bracket of rates known to need less
/// (below the rating) and more (above it) than the rating: a step out of the bracket, or a pass
/// whose solve did not converge (or asks for reverse thrust), bisects it instead. A failed solve
/// only bounds the rates softly: a converged pass that measures the rating unspent within
/// `REFUSED_LIFT_FRACTION` of that bound lifts it (a first guess the trim could not start from
/// had held the C295 path-entry climb at its scheduled 12 m/s and 0.89 of the rating). Each pass
/// re-solves the throttle and body angle at the new rates from the last converged unknowns.
pub(super) fn settle_on_rating(
    segment: &mut Segment,
    analyses: &MissionAnalyses,
    first: hybrd::Solution,
    initial: &[f64],
    settings: &Settings,
) -> Result<SegmentSolution, MissionError> {
    let floor = crate::segments::MIN_RATED_CLIMB_RATE_M_S;
    let points = segment.rate_profile_m_s.len();
    let mut below = vec![floor; points];
    let mut above = vec![f64::INFINITY; points];
    // Upper ends left by failed solves: a failed solve says the rates asked too much of the trim
    // as a whole, not at which point, so a later converged pass that measures the rating unspent
    // at that rate lifts the point's end.
    let mut refused = vec![f64::INFINITY; points];
    let mut solution = first;
    let mut last_converged: Option<(Vec<f64>, hybrd::Solution)> = None;
    let mut settled = false;
    for _ in 0..MAX_RATING_PASSES {
        segment.iterate(analyses);
        let conditions = &segment.conditions;
        let mut rates = segment.rate_profile_m_s.clone();
        // A climb at the rating that asks for reverse thrust anywhere has not found the climb:
        // the rates are too steep to trim (a root of the residuals with the aircraft decelerating
        // against its own thrust), and the pass treats them as a failed solve.
        let converged = solution.status.is_converged()
            && segment
                .throttle
                .iter()
                .all(|command| command.is_finite() && *command > 0.0);
        let mut largest_miss: f64 = 0.0;
        if converged {
            last_converged = Some((segment.pack_unknowns(), solution.clone()));
            let shares = segment.energy_shares();
            for (point, rate) in rates.iter_mut().enumerate() {
                let command = segment.throttle[point];
                let force = norm(conditions.thrust_force_vector_n[point]);
                let weight = conditions.total_mass_kg[point] * conditions.gravity_m_s2[point];
                let speed = conditions.velocity_m_s[point];
                let ceiling = 0.95 * speed;
                if command < 1.0 {
                    below[point] = below[point].max(*rate);
                    if refused[point] <= *rate * (1.0 + REFUSED_LIFT_FRACTION) {
                        refused[point] = f64::INFINITY;
                    }
                } else {
                    above[point] = above[point].min(*rate);
                }
                let on_cap = *rate >= ceiling && command < 1.0;
                let on_floor = *rate <= floor && command > 1.0;
                if !(on_cap || on_floor) {
                    largest_miss = largest_miss.max((command - 1.0).abs());
                }
                let usable = command.is_finite()
                    && command.abs() > f64::EPSILON.sqrt()
                    && force.is_finite()
                    && weight > 0.0;
                let step = if usable {
                    *rate + shares[point] * force / command.abs() * (1.0 - command) * speed / weight
                } else {
                    f64::NAN
                };
                let missing = (command - 1.0).abs() > ENVELOPE_TOLERANCE && !(on_cap || on_floor);
                let mut high = above[point].min(refused[point]).min(ceiling);
                let collapsed = high.partial_cmp(&below[point])
                    != Some(std::cmp::Ordering::Greater)
                    || high - below[point] <= 1.0e-6 * high.max(1.0);
                if missing && collapsed {
                    // A bracket left by the coupling of neighbouring points no longer holds the
                    // root: keep only this measurement.
                    below[point] = floor;
                    above[point] = f64::INFINITY;
                    refused[point] = f64::INFINITY;
                    if command < 1.0 {
                        below[point] = *rate;
                    } else {
                        above[point] = *rate;
                    }
                    high = above[point].min(ceiling);
                }
                let low = below[point];
                let bisect = high.is_finite() && high > low;
                *rate = if !step.is_finite() {
                    if bisect {
                        0.5 * (low + high)
                    } else {
                        *rate
                    }
                } else if step >= high {
                    // Past the ceiling with no measured bound under it: the climb flies it.
                    if above[point].min(refused[point]) >= ceiling || !bisect {
                        ceiling
                    } else {
                        0.5 * (low + high)
                    }
                } else if step <= low {
                    if low <= floor || !bisect {
                        floor
                    } else {
                        0.5 * (low + high)
                    }
                } else {
                    step
                }
                .clamp(floor, ceiling);
            }
        } else {
            // The solve failed at these rates (or found reverse thrust): they ask for more than
            // the leg can be trimmed to fly, so each is an upper end of its bracket, and the pass
            // restarts at the bracket's middle from the last converged unknowns
            for (point, rate) in rates.iter_mut().enumerate() {
                refused[point] = refused[point].min(*rate);
                *rate = 0.5 * (below[point] + above[point].min(refused[point]));
            }
            // (the leg's own first guess until a solve has found the climb).
            match &last_converged {
                Some((unknowns, _)) => segment.unpack_unknowns(unknowns),
                None => segment.unpack_unknowns(initial),
            }
        }
        if converged && largest_miss <= ENVELOPE_TOLERANCE {
            settled = true;
            break;
        }
        segment.rate_profile_m_s = rates;
        let start = segment.pack_unknowns();
        solution = {
            let segment = &mut *segment;
            hybrd::solve(
                |point, residual| {
                    segment.unpack_unknowns(point);
                    segment.iterate(analyses);
                    residual.copy_from_slice(&segment.pack_residuals());
                },
                &start,
                settings,
            )
        }
        .map_err(|reason| MissionError::Solver {
            tag: segment.spec.tag.clone(),
            reason,
        })?;
        segment.unpack_unknowns(&solution.x);
    }
    Ok(rating_solution(segment, analyses, &solution, settled))
}

fn norm(vector: [f64; 3]) -> f64 {
    vector.iter().map(|value| value * value).sum::<f64>().sqrt()
}

/// The solution of a rating-limited climb: the command published is the rating (clamped to it,
/// within the envelope tolerance the settling stopped at).
fn rating_solution(
    segment: &mut Segment,
    analyses: &MissionAnalyses,
    solution: &hybrd::Solution,
    settled: bool,
) -> SegmentSolution {
    for throttle in &mut segment.throttle {
        *throttle = throttle.min(1.0);
    }
    segment.iterate(analyses);
    let converged = settled;
    segment.numerics.converged = Some(converged);
    if !converged {
        tracing::warn!(
            segment = %segment.spec.tag,
            status = ?solution.status,
            evaluations = solution.evaluations,
            "rating-limited climb did not converge on a climb"
        );
    }
    SegmentSolution {
        converged,
        status: solution.status,
        evaluations: solution.evaluations,
        throttle_limited: !converged,
        peak_throttle: segment
            .throttle
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max),
        minimum_throttle: segment
            .throttle
            .iter()
            .copied()
            .fold(f64::INFINITY, f64::min),
        available_throttle_floor: f64::NAN,
        idle_floor_limited: false,
        sub_idle_points: 0,
    }
}
