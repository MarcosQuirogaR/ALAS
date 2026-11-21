// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A climb the commanded rate asks too much thrust of is flown at the maximum-climb rating.
//!
//! The schedule's en-route climbs are flown at the rating from the start (`schedule::rated`).
//! Where a constant-rate climb (a leg of a mission plan, or a piece cut for a path change) runs
//! out of thrust, the part it could not fly is flown at
//! the maximum-climb rating on the same CAS/Mach schedule, and its vertical rate is what the
//! specific excess power gives (`SegmentSpec::rating_limited`, the total-energy climb of BADA
//! practice). Its ceiling is where that rate falls below the residual climb of the service
//! ceiling, [`CLIMB_END_RESIDUAL_RATE_M_S`]: the climb ends there and the run says so.

use alas_mission::segments::{Segment, SegmentKind, SegmentSpec};

use super::dispatch::field_climb;
use super::guidance::{first_saturated_altitude_m, CLIMB_END_RESIDUAL_RATE_M_S};

/// Suffix of the tag of the upper piece of a climb flown at the rating.
const RATED_TAG_SUFFIX: &str = "_rated";

/// Turn the climb at `index`, refused for thrust at its commanded rate, into a climb at the
/// maximum-climb rating: the part below the altitude the commanded rate last held keeps that
/// rate (and its tag); the rest is one rating-limited piece. A climb refused from its first
/// point, one that blends a speed in (its saturation at the start is the acceleration), or one
/// whose solve did not converge, is rated whole. Returns `false` when the leg is not a
/// commanded climb.
pub(super) fn rate_refused_climb(
    schedule: &mut Vec<SegmentSpec>,
    index: usize,
    failed: &Segment,
) -> bool {
    let Some(spec) = schedule.get(index) else {
        return false;
    };
    let SegmentKind::Climb {
        altitude_start_m,
        altitude_end_m,
        climb_rate_m_s,
    } = spec.kind
    else {
        return false;
    };
    if spec.rating_limited {
        return false;
    }
    let start_m = altitude_start_m.or_else(|| failed.conditions.altitude_m.first().copied());
    let blends = spec.entry_speed_m_s > 0.0 || spec.exit_speed_m_s > 0.0;
    // An unconverged solve says nothing about where the commanded rate saturates.
    let solved = failed.numerics.converged == Some(true);
    let split = (!blends && solved)
        .then_some(start_m)
        .flatten()
        .and_then(|start_m| {
            first_saturated_altitude_m(failed, start_m, altitude_end_m, climb_rate_m_s)
        });
    match split {
        Some(saturation_m) => {
            let mut upper = schedule[index].clone();
            upper.tag = unique_tag(schedule, &format!("{}{RATED_TAG_SUFFIX}", upper.tag));
            upper.rating_limited = true;
            upper.kind = SegmentKind::Climb {
                altitude_start_m: Some(saturation_m),
                altitude_end_m,
                climb_rate_m_s,
            };
            schedule[index].kind = SegmentKind::Climb {
                altitude_start_m: start_m,
                altitude_end_m: saturation_m,
                climb_rate_m_s,
            };
            schedule.insert(index + 1, upper);
        }
        None => schedule[index].rating_limited = true,
    }
    true
}

fn unique_tag(schedule: &[SegmentSpec], base: &str) -> String {
    let mut tag = base.to_owned();
    let mut copy = 1;
    while schedule.iter().any(|spec| spec.tag == tag) {
        copy += 1;
        tag = format!("{base}_{copy}");
    }
    tag
}

/// What a solved rating-limited climb comes to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Rated {
    /// Flown to its end at no less than the residual climb. A later flight of the leg settles
    /// on the rating again from its scheduled rate as the first guess.
    Flown,
    /// Its ceiling is inside it: it now ends at `reached_m`, the last altitude it climbed at the
    /// residual rate or better, after the leg's `last_rate_m_s` there.
    Ended {
        /// Altitude the climb now ends at, m.
        reached_m: f64,
        /// Rate it climbed at there, m/s.
        last_rate_m_s: f64,
    },
    /// It cannot climb at the residual rate from its start: it is removed, and the next leg
    /// starts from `reached_m`, where the leg before it ended.
    Removed {
        /// Altitude the climb started at, m.
        reached_m: f64,
    },
    /// It is the first leg of a flight and cannot climb at the residual rate from its start. It
    /// is kept: the legs after it have nothing else to start from, and flying them level at the
    /// field is not a flight the aircraft can make. The flight stops on it and the run reports it
    /// (`dispatch::field_climb`).
    FirstLegRefused {
        /// Altitude the climb starts at, m.
        start_m: f64,
    },
}

/// Whether a specific excess power, m/s, holds the residual climb of the service ceiling.
pub(super) fn holds_residual(rate_m_s: f64) -> bool {
    rate_m_s.is_finite() && rate_m_s >= CLIMB_END_RESIDUAL_RATE_M_S
}

/// How many points of a solved rating-limited climb, from its start, held the residual climb
/// (`rates` is its specific excess power): `None` when it converged and held it throughout. Where
/// the excess power first falls below the residual climb (it falls with altitude at the rating),
/// the climb ends at the point before it; an unconverged solve ends at its start.
fn held_points(rates: &[f64], converged: bool) -> Option<usize> {
    if converged && rates.iter().copied().all(holds_residual) {
        return None;
    }
    Some(if converged {
        rates
            .iter()
            .position(|rate| !holds_residual(*rate))
            .unwrap_or(rates.len())
    } else {
        0
    })
}

/// Whether the solved rating-limited climb `segment` cannot climb at the residual rate from its
/// start: the climb [`settle`] removes, or keeps as a refused first leg.
pub(super) fn cannot_leave_start(segment: &Segment, converged: bool) -> bool {
    held_points(&segment.specific_excess_power_m_s(), converged).is_some_and(|held| held < 2)
}

/// Settle the solved rating-limited climb at `index` against its ceiling.
pub(super) fn settle(
    schedule: &mut Vec<SegmentSpec>,
    index: usize,
    segment: &Segment,
    converged: bool,
) -> Rated {
    // The residual climb of a service ceiling is a steady climb: a leg that also accelerates is
    // judged on its specific excess power (its rate over the energy share), not on the part of it
    // that goes into height.
    let rates = segment.specific_excess_power_m_s();
    let altitudes = &segment.conditions.altitude_m;
    let start_m = altitudes.first().copied().unwrap_or(f64::NAN);
    let Some(held) = held_points(&rates, converged) else {
        return Rated::Flown;
    };
    if held >= 2 {
        let reached_m = altitudes[held - 1];
        let last_rate_m_s = rates[held - 1];
        if let SegmentKind::Climb {
            altitude_start_m,
            altitude_end_m,
            ..
        } = &mut schedule[index].kind
        {
            *altitude_start_m = Some(altitude_start_m.unwrap_or(start_m));
            *altitude_end_m = reached_m;
        }
        return Rated::Ended {
            reached_m,
            last_rate_m_s,
        };
    }
    if field_climb::keeps_start(schedule, index) {
        return Rated::FirstLegRefused { start_m };
    }
    schedule.remove(index);
    Rated::Removed { reached_m: start_m }
}

/// The sentence for a climb at the maximum-climb rating that met its ceiling, with the altitude
/// it ended at (nothing for a flown one, nor for a refused first leg: that one is not flown level,
/// the flight stops on it). `last_rate_m_s` is the last point that still held the residual climb,
/// so it is at or above the limit: the next point is the one that fell below.
fn ceiling_sentence(outcome: Rated) -> Option<(f64, String)> {
    let (reached_m, where_) = match outcome {
        Rated::Flown | Rated::FirstLegRefused { .. } => return None,
        Rated::Ended {
            reached_m,
            last_rate_m_s,
        } => (
            reached_m,
            format!(
                "falls to the {CLIMB_END_RESIDUAL_RATE_M_S:.3} m/s residual climb of the service ceiling (last point held: {last_rate_m_s:.3} m/s)"
            ),
        ),
        Rated::Removed { reached_m } => (
            reached_m,
            format!(
                "is below the {CLIMB_END_RESIDUAL_RATE_M_S:.3} m/s residual climb of the service ceiling from its start"
            ),
        ),
    };
    Some((
        reached_m,
        format!(
            "at the maximum-climb rating the specific excess power {where_}: the climb ends at {reached_m:.0} m"
        ),
    ))
}

/// Say where a climb at the maximum-climb rating met its ceiling (nothing for a flown one).
/// `None` for a refused first leg: it is kept, so there is no revision, and the flight stops on
/// it (`dispatch::field_climb`).
pub(super) fn note_ceiling(flown_level: &mut Vec<String>, tag: &str, outcome: Rated) -> Option<()> {
    if let Rated::FirstLegRefused { start_m } = outcome {
        tracing::info!(
            segment = %tag,
            start_m,
            "kept the first leg of a flight, which cannot climb from its start at the rating"
        );
        return None;
    }
    if let Some((_, sentence)) = ceiling_sentence(outcome) {
        super::flight::note_flown_level(flown_level, tag, &sentence);
    }
    Some(())
}

// A failed unwrap here is the assertion failing.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod ceiling_sentence_tests {
    use super::*;

    #[test]
    fn the_ceiling_message_states_the_true_relation_of_rate_and_limit() {
        let ended = ceiling_sentence(Rated::Ended {
            reached_m: 11_366.0,
            last_rate_m_s: 0.771,
        })
        .unwrap()
        .1;
        assert!(
            ended.contains("falls to the 0.508 m/s residual climb"),
            "{ended}"
        );
        assert!(ended.contains("last point held: 0.771 m/s"), "{ended}");
        assert!(
            !ended.contains("below"),
            "a rate above the limit is not below it: {ended}"
        );
        assert!(ended.ends_with("the climb ends at 11366 m"), "{ended}");
        let removed = ceiling_sentence(Rated::Removed { reached_m: 500.0 })
            .unwrap()
            .1;
        assert!(
            removed.contains("is below the 0.508 m/s residual climb"),
            "{removed}"
        );
        assert!(ceiling_sentence(Rated::Flown).is_none());
        assert!(ceiling_sentence(Rated::FirstLegRefused { start_m: 500.0 }).is_none());
    }
}

#[cfg(test)]
mod settle_tests {
    use super::*;
    use crate::mission_stage::dispatch::field_climb::test_support::{climb, climbing_at};

    fn tags(schedule: &[SegmentSpec]) -> Vec<&str> {
        schedule.iter().map(|spec| spec.tag.as_str()).collect()
    }

    /// The take-off of a plan at the rating, below the 0.508 m/s residual climb from its start
    /// (converged or not): it is the leg the flight starts on, so it is kept and refused, and the
    /// climb after it, which takes its start from it, still has one.
    #[test]
    fn a_first_leg_that_cannot_climb_is_kept_and_refused() {
        let first = climb("takeoff@00", Some(100.0), 600.0, true);
        for converged in [true, false] {
            let mut schedule = vec![
                first.clone(),
                climb("initial_climb@01", None, 3_000.0, false),
            ];
            let outcome = settle(&mut schedule, 0, &climbing_at(&first, 0.2), converged);
            assert_eq!(outcome, Rated::FirstLegRefused { start_m: 100.0 });
            assert_eq!(tags(&schedule), ["takeoff@00", "initial_climb@01"]);
        }
        assert!(cannot_leave_start(&climbing_at(&first, 0.2), true));
        assert!(!cannot_leave_start(&climbing_at(&first, 3.0), true));
    }

    /// The same climb after another leg is removed as before: the leg after it goes on from the
    /// state the leg before ended in.
    #[test]
    fn a_later_climb_that_cannot_climb_is_removed() {
        let climb_leg = climb("climb@02", Some(600.0), 3_000.0, true);
        let mut schedule = vec![
            climb("takeoff@00", Some(100.0), 600.0, true),
            climb_leg.clone(),
            climb("climb_2@02", None, 6_000.0, true),
        ];
        let outcome = settle(&mut schedule, 1, &climbing_at(&climb_leg, 0.2), true);
        assert_eq!(outcome, Rated::Removed { reached_m: 600.0 });
        assert_eq!(tags(&schedule), ["takeoff@00", "climb_2@02"]);
    }

    /// A first leg that holds the residual climb is flown.
    #[test]
    fn a_first_leg_that_climbs_is_flown() {
        let first = climb("takeoff@00", Some(100.0), 600.0, true);
        let mut schedule = vec![first.clone()];
        assert_eq!(
            settle(&mut schedule, 0, &climbing_at(&first, 3.0), true),
            Rated::Flown
        );
        assert_eq!(schedule.len(), 1);
    }
}
