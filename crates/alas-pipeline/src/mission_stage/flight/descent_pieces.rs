// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Descent rungs cut into pieces, each with a vertical rate of its own.
//!
//! A descent at (near) idle thrust has no free vertical rate: the engine's
//! flight-idle thrust fixes the steepest path the speed schedule allows, and that
//! limit changes along the rung (the air thickens, drag and idle thrust both
//! rise). One commanded rate per rung is the largest flyable at the *worst* point
//! of the rung, so the rest of it is flown with thrust above idle on a path
//! shallower than the aircraft can fly (A340-300 `descent_2_lower`: 7.8 m/s over
//! 29,300 to 18,700 ft, thrust 24 kN to 18 kN, where the priced model, which takes
//! every step at its own limit, flies 8.6 to 9.1 m/s at 12.6 kN: the native
//! descent was 11 % longer and burned 59 % more fuel in that band).
//!
//! The priced model resolves a CAS rung into `CAS_SPEED_SUBDIVISIONS` (8) equal sub-rungs, so
//! a sub-rung is the rung height over eight (about 1,050 ft in an 8,400 ft climb band, 2,900 ft
//! in a 23,000 ft descent band); the native schedule cuts a descent rung wider than
//! [`MAXIMUM_PIECE_M`] into equal pieces of the same altitude span and speed
//! schedule, and the rate pass (`rate_pass`) then finds the engine boundary of each
//! piece on its own.

use alas_mission::segments::{SegmentKind, SegmentSpec};

use crate::mission_stage::schedule::junction::PIECE_MARK;

/// The widest descent piece, m: 2,625 ft, a sub-rung of the 21,000 ft descent band the priced model
/// resolves into eight.
pub(super) const MAXIMUM_PIECE_M: f64 = 800.0;

/// Tag suffix of the pieces below the first of a cut rung.
pub(crate) const PIECE_SUFFIX: &str = "_piece";

/// Cut every descent of `schedule` taller than [`MAXIMUM_PIECE_M`] into equal
/// pieces. The first keeps the tag, the entry speed blend and the rate; the last
/// keeps the exit speed blend. Legs the speed joining cut (tags with
/// [`PIECE_MARK`]) are left alone.
pub(in crate::mission_stage) fn subdivide_descents(
    schedule: &mut Vec<SegmentSpec>,
    departure_elevation_m: f64,
) {
    let mut cut = Vec::with_capacity(schedule.len() + 16);
    let mut current_m = departure_elevation_m;
    for spec in schedule.drain(..) {
        match spec.kind {
            SegmentKind::Climb { altitude_end_m, .. } => {
                current_m = altitude_end_m;
                cut.push(spec);
            }
            SegmentKind::Cruise { .. } => cut.push(spec),
            SegmentKind::Descent {
                altitude_start_m,
                altitude_end_m,
                descent_rate_m_s,
            } => {
                let start_m = altitude_start_m.unwrap_or(current_m);
                current_m = altitude_end_m;
                let span_m = start_m - altitude_end_m;
                let pieces = (span_m / MAXIMUM_PIECE_M).ceil().max(1.0) as usize;
                if pieces <= 1 || spec.tag.contains(PIECE_MARK) {
                    cut.push(spec);
                    continue;
                }
                for piece in 0..pieces {
                    let upper_m = start_m - span_m * piece as f64 / pieces as f64;
                    let lower_m = start_m - span_m * (piece + 1) as f64 / pieces as f64;
                    let mut leg = spec.clone();
                    if piece > 0 {
                        leg.tag = format!("{}{PIECE_SUFFIX}_{piece}", spec.tag);
                        leg.entry_speed_m_s = 0.0;
                    }
                    if piece + 1 < pieces {
                        leg.exit_speed_m_s = 0.0;
                    }
                    leg.kind = SegmentKind::Descent {
                        altitude_start_m: Some(upper_m),
                        altitude_end_m: lower_m,
                        descent_rate_m_s,
                    };
                    cut.push(leg);
                }
            }
        }
    }
    *schedule = cut;
}

// A test asserts on schedules it just built, so a failed unwrap is the assertion failing.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::airports::get as get_airport;
    use alas_config::AlasConfig;
    use alas_mission::build_mission_request;

    /// Every descent is cut into pieces no taller than the limit that join without a gap, the
    /// route distance is unchanged by the cut, and the first piece of a rung keeps its tag and
    /// the last its exit.
    #[test]
    fn a_tall_descent_is_cut_into_contiguous_pieces() {
        let config = AlasConfig::default();
        let origin = get_airport(&config.departure_airport).unwrap();
        let destination = get_airport(&config.arrival_airport).unwrap();
        let request = build_mission_request(&config, origin, destination, 5_000_000.0);
        let schedule = crate::mission_stage::schedule::build_schedule(&request).unwrap();
        let mut cut = schedule.clone();
        subdivide_descents(&mut cut, request.departure_elevation_m);
        assert!(cut.len() > schedule.len());
        let mut previous_end: Option<f64> = None;
        for spec in &cut {
            if let SegmentKind::Descent {
                altitude_start_m: Some(start_m),
                altitude_end_m,
                ..
            } = spec.kind
            {
                if spec.tag.contains(PIECE_SUFFIX) {
                    assert!(start_m - altitude_end_m <= MAXIMUM_PIECE_M + 1.0e-9);
                    assert_eq!(previous_end, Some(start_m), "{}", spec.tag);
                    assert_eq!(spec.entry_speed_m_s, 0.0);
                }
                previous_end = Some(altitude_end_m);
            }
        }
        let flown = crate::mission_stage::schedule::schedule_horizontal_distance(
            &cut,
            request.departure_elevation_m,
        );
        let before = crate::mission_stage::schedule::schedule_horizontal_distance(
            &schedule,
            request.departure_elevation_m,
        );
        assert!((flown - before).abs() < 1.0e-6 * before);
        assert!(cut.iter().any(|spec| spec.tag == "descent_1"));
    }
}
