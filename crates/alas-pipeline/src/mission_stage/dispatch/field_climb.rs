// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A flight whose first leg cannot climb from where it starts.
//!
//! The leg a flight starts on (the first of its schedule, or the take-off of a plan stage) sets
//! its start state: the field elevation and the mass the dispatch loaded. The guidance ends or
//! removes a climb that cannot hold the residual climb of the service ceiling, and the leg after a
//! removed one goes on from where the removed one started. The leg a flight starts on has nothing
//! before it to go on from, and flying the rest of the schedule level at the field would be a
//! flight the aircraft cannot make. So that leg is never removed ([`remove_leg`]), nor is a climb
//! whose level speed change into it is where the flight starts
//! ([`end_climb_after_speed_change`]): the flight stops on it ([`stop_on`]), and the run reports it
//! ([`first_climb_refusal`]). The mass it could not climb at is the mass the fuel load gave it, so
//! a plan whose fuel closure ends on such a flight has not settled.

use alas_mission::segments::{Segment, SegmentKind, SegmentSpec};
use alas_mission::MissionResult;

use crate::mission_stage::flight::note_flown_level;
use crate::mission_stage::guidance::CLIMB_END_RESIDUAL_RATE_M_S;
use crate::mission_stage::rated_climb;
use crate::mission_stage::schedule::junction;
use crate::mission_stage::schedule::speed_change::{is_speed_change, SPEED_CHANGE_PREFIX};

/// Whether `tag` names the take-off leg of a plan stage (`takeoff@NN`; a piece cut from it
/// carries a suffix and starts part way up).
fn is_take_off(tag: &str) -> bool {
    tag.split_once('@').is_some_and(|(name, stage)| {
        name == "takeoff" && !stage.is_empty() && stage.bytes().all(|b| b.is_ascii_digit())
    })
}

/// Whether `spec`, at `index` of its schedule, is the first leg of a flight: the first of the
/// schedule, or the take-off leg of a plan stage.
pub(in crate::mission_stage) fn starts_a_flight(index: usize, spec: &SegmentSpec) -> bool {
    index == 0 || is_take_off(&spec.tag)
}

/// Whether `spec`, at `index`, is the level speed change a flight starts on: the first of the
/// schedule, or the speed change into a plan stage's take-off.
fn speed_change_starts_a_flight(index: usize, spec: &SegmentSpec) -> bool {
    is_speed_change(spec)
        && (index == 0
            || spec
                .tag
                .strip_prefix(SPEED_CHANGE_PREFIX)
                .is_some_and(is_take_off))
}

/// Whether the leg at `index` of `schedule` starts a flight, so that it may not be removed.
pub(in crate::mission_stage) fn keeps_start(schedule: &[SegmentSpec], index: usize) -> bool {
    schedule
        .get(index)
        .is_some_and(|spec| starts_a_flight(index, spec))
}

/// Whether a leg of the junction family `family`, or the level speed change into one, starts a
/// flight.
pub(in crate::mission_stage) fn family_starts_a_flight(
    schedule: &[SegmentSpec],
    family: &str,
) -> bool {
    schedule.iter().enumerate().any(|(index, spec)| {
        let leg = spec
            .tag
            .strip_prefix(SPEED_CHANGE_PREFIX)
            .unwrap_or(&spec.tag);
        junction::family(leg) == family && starts_a_flight(index, spec)
    })
}

/// Remove the leg at `index` of `schedule`; the leg after it then starts from the state the leg
/// before it ended in. A leg a flight starts on is kept, since nothing before it gives the next
/// leg a start: `None`, as for an index past the schedule.
pub(in crate::mission_stage) fn remove_leg(
    schedule: &mut Vec<SegmentSpec>,
    index: usize,
) -> Option<()> {
    if index >= schedule.len() || keeps_start(schedule, index) {
        return None;
    }
    schedule.remove(index);
    Some(())
}

/// The climb at the maximum-climb rating after the level speed change at `index` of `schedule`,
/// which lengthening has not made flyable (`change` as flown, needing `peak_throttle` times the
/// rating): the climb's speed is not sustainable at this level, so the specific excess power there
/// is below the residual climb of the service ceiling and the climb ends here. The whole leg goes,
/// its junction pieces included (they are put back into it at the next joining), and the run says
/// so. A climb a flight starts on is kept: there is no revision (`None`), the flight stops on the
/// speed change, and [`first_climb_refusal`] reports it.
pub(in crate::mission_stage) fn end_climb_after_speed_change(
    schedule: &mut Vec<SegmentSpec>,
    index: usize,
    change: Option<&Segment>,
    peak_throttle: f64,
    flown_level: &mut Vec<String>,
    revision: usize,
) -> Option<()> {
    let climb_tag = junction::family(&schedule.get(index + 1)?.tag).to_owned();
    if family_starts_a_flight(schedule, &climb_tag) {
        tracing::info!(
            segment = %climb_tag,
            revision,
            "kept the first climb of a flight, whose speed is not sustainable at its start"
        );
        return None;
    }
    let reached_m = change
        .and_then(|change| change.conditions.altitude_m.first().copied())
        .unwrap_or(f64::NAN);
    schedule.retain(|spec| junction::family(&spec.tag) != climb_tag);
    note_flown_level(
        flown_level,
        &climb_tag,
        &format!(
            "the level speed change to its speed needs {peak_throttle:.2} times the maximum-climb \
             rating at {reached_m:.0} m, so the specific excess power is below the \
             {CLIMB_END_RESIDUAL_RATE_M_S:.3} m/s residual climb of the service ceiling there: \
             the climb ends at {reached_m:.0} m"
        ),
    );
    tracing::info!(
        segment = %climb_tag,
        revision,
        reached_m,
        "ended a climb at the rating whose speed is not sustainable at its start"
    );
    Some(())
}

/// A climb at the maximum-climb rating that cannot climb at the residual rate from its start.
fn refused_at_rating(segment: &Segment, converged: bool) -> bool {
    segment.is_rating_limited() && rated_climb::cannot_leave_start(segment, converged)
}

/// The index of the first leg of a flight in `result` that is flown at the maximum-climb rating
/// and cannot climb at the residual rate from its start: the leg the flight stops on.
pub(in crate::mission_stage) fn refused_first_climb(result: &MissionResult) -> Option<usize> {
    result
        .segments
        .iter()
        .zip(&result.solutions)
        .enumerate()
        .find_map(|(index, (segment, solution))| {
            let refused = refused_at_rating(segment, solution.converged);
            (refused && starts_a_flight(index, &segment.spec)).then_some(index)
        })
}

/// `result` as a flight that stops on its leg at `index`: the legs after it are not flown.
pub(in crate::mission_stage) fn stop_on(mut result: MissionResult, index: usize) -> MissionResult {
    result.segments.truncate(index + 1);
    result.solutions.truncate(index + 1);
    if result
        .fuel_exhaustion
        .as_ref()
        .is_some_and(|exhaustion| exhaustion.segment_index > index)
    {
        result.fuel_exhaustion = None;
    }
    result
}

/// A flight that stopped on its first leg: a climb it could not fly from where it starts, or the
/// level speed change into that climb.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FirstClimbRefusal {
    /// The tag of the leg the flight stopped on.
    pub(crate) tag: String,
    /// What the run says, in words.
    pub(crate) message: String,
    /// The specific excess power where the leg fell below the residual climb, at the
    /// maximum-climb rating, m/s; `None` when its solve did not converge or it was not refused at
    /// the rating.
    pub(crate) excess_power_m_s: Option<f64>,
    /// The residual climb of the service ceiling it is judged against, m/s.
    pub(crate) residual_climb_m_s: f64,
}

/// Why the flight `result` stopped on its first leg, when that leg is a climb it could not fly or
/// the level speed change into one; `None` for a flight that completed, ran out of fuel, or
/// stopped on a later leg. Only a climb refused at the maximum-climb rating "cannot climb" from
/// where it starts; any other refusal of the first leg is said with its own reason.
pub(crate) fn first_climb_refusal(result: &MissionResult) -> Option<FirstClimbRefusal> {
    if result.fuel_exhaustion.is_some() || result.completed_summary().is_some() {
        return None;
    }
    let index = result.segments.len().checked_sub(1)?;
    let segment = result.segments.get(index)?;
    let solution = result.solutions.get(index)?;
    let tag = &segment.spec.tag;
    let climb = matches!(segment.spec.kind, SegmentKind::Climb { .. })
        && starts_a_flight(index, &segment.spec);
    if !climb && !speed_change_starts_a_flight(index, &segment.spec) {
        return None;
    }
    let at_rating = climb && refused_at_rating(segment, solution.converged);
    let flown = solution.converged && !solution.throttle_limited && !solution.idle_floor_limited;
    if flown && !at_rating {
        return None;
    }
    let conditions = &segment.conditions;
    let first = |values: &[f64]| values.first().copied().unwrap_or(f64::NAN);
    let start_m = first(&conditions.altitude_m);
    let mass_kg = first(&conditions.total_mass_kg);
    let refusal = || {
        result
            .completion_refusal()
            .unwrap_or_else(|| "its solve was refused".to_owned())
    };
    // Where the excess power first falls below the residual climb, on a converged solve.
    let fell = (at_rating && solution.converged)
        .then(|| {
            segment
                .specific_excess_power_m_s()
                .into_iter()
                .zip(conditions.altitude_m.iter().copied())
                .find(|(rate, _)| !rated_climb::holds_residual(*rate))
        })
        .flatten();
    let message = if at_rating {
        let why = match fell {
            Some((rate_m_s, altitude_m)) => format!(
                "at the maximum-climb rating its specific excess power is {rate_m_s:.3} m/s at \
                 {altitude_m:.0} m, below the {CLIMB_END_RESIDUAL_RATE_M_S:.3} m/s residual \
                 climb of the service ceiling"
            ),
            None => refusal(),
        };
        format!(
            "the first leg of the flight, {tag}, cannot climb from {start_m:.0} m at \
             {mass_kg:.0} kg: {why}; the flight stops on it"
        )
    } else if climb {
        format!(
            "the first leg of the flight, {tag}, a climb from {start_m:.0} m at {mass_kg:.0} kg, \
             was not flown: {}; the flight stops on it",
            refusal()
        )
    } else {
        format!(
            "the first leg of the flight, {tag}, the level speed change into its climb at \
             {start_m:.0} m and {mass_kg:.0} kg, was not flown: {}; the flight stops on it",
            refusal()
        )
    };
    Some(FirstClimbRefusal {
        tag: tag.clone(),
        message,
        excess_power_m_s: fell.map(|(rate_m_s, _)| rate_m_s),
        residual_climb_m_s: CLIMB_END_RESIDUAL_RATE_M_S,
    })
}

/// Climbs, level legs and flights built by hand for the tests of the guidance sites that keep a
/// flight's first leg, and of the findings that report it.
#[cfg(test)]
pub(crate) mod test_support {
    use alas_config::mission::{SpeedCaps, SpeedReference};
    use alas_math::hybrd::Status;
    use alas_mission::segments::{Segment, SegmentKind, SegmentSpec};
    use alas_mission::{MissionResult, SegmentSolution};

    /// True airspeed of the hand-built legs, m/s.
    const SPEED_M_S: f64 = 120.0;

    /// Mass of the hand-built flown legs, kg.
    const MASS_KG: f64 = 60_000.0;

    /// A 2 m/s climb `tag` from `start_m` to `end_m`, at the maximum-climb rating when `rated`.
    pub(crate) fn climb(tag: &str, start_m: Option<f64>, end_m: f64, rated: bool) -> SegmentSpec {
        SegmentSpec {
            tag: tag.to_owned(),
            kind: SegmentKind::Climb {
                altitude_start_m: start_m,
                altitude_end_m: end_m,
                climb_rate_m_s: 2.0,
            },
            air_speed_m_s: SPEED_M_S,
            air_speed_reference: SpeedReference::TrueAirspeed,
            speed_caps: SpeedCaps::default(),
            true_course_rad: 0.0,
            temperature_deviation_k: 0.0,
            number_control_points: 16,
            takeoff_rated: false,
            rating_limited: rated,
            configuration_delta_cd: 0.0,
            load_factor: 1.0,
            entry_speed_m_s: 0.0,
            exit_speed_m_s: 0.0,
            speed_profile: Vec::new(),
        }
    }

    /// A 1 km level leg `tag` (a speed change when its tag says so) at `altitude_m`, or at the
    /// altitude the leg before it ends at when `None`.
    pub(crate) fn level(tag: &str, altitude_m: Option<f64>) -> SegmentSpec {
        SegmentSpec {
            kind: SegmentKind::Cruise {
                altitude_m,
                distance_m: 1_000.0,
            },
            ..climb(tag, None, 0.0, false)
        }
    }

    /// `spec` (a climb with a declared start) as solved at a constant speed, climbing at
    /// `rate_m_s` at every point.
    pub(crate) fn climbing_at(spec: &SegmentSpec, rate_m_s: f64) -> Segment {
        let mut segment =
            Segment::new(spec.clone(), None).unwrap_or_else(|error| panic!("segment: {error}"));
        let SegmentKind::Climb {
            altitude_start_m: Some(start_m),
            altitude_end_m: end_m,
            ..
        } = spec.kind
        else {
            panic!("a climb with a declared start");
        };
        let points = segment.conditions.altitude_m.len();
        let conditions = &mut segment.conditions;
        conditions.altitude_m = (0..points)
            .map(|point| start_m + (end_m - start_m) * point as f64 / (points - 1) as f64)
            .collect();
        conditions.velocity_m_s = vec![SPEED_M_S; points];
        conditions.velocity_vector_m_s = vec![[SPEED_M_S, 0.0, -rate_m_s]; points];
        conditions.gravity_m_s2 = vec![9.806_65; points];
        conditions.total_mass_kg = vec![MASS_KG; points];
        segment
    }

    /// `spec` (a level leg with a declared altitude) as solved at that altitude.
    pub(crate) fn holding(spec: &SegmentSpec) -> Segment {
        let mut segment =
            Segment::new(spec.clone(), None).unwrap_or_else(|error| panic!("segment: {error}"));
        let points = segment.conditions.altitude_m.len();
        segment.conditions.total_mass_kg = vec![MASS_KG; points];
        segment
    }

    /// How a hand-built segment's solve came out: converged, or refused at the rating.
    pub(crate) fn solution(converged: bool) -> SegmentSolution {
        SegmentSolution {
            converged,
            status: if converged {
                Status::Converged
            } else {
                Status::MaxEvaluations
            },
            evaluations: 1,
            throttle_limited: !converged,
            peak_throttle: 1.0,
            minimum_throttle: 1.0,
            available_throttle_floor: f64::NAN,
            idle_floor_limited: false,
            sub_idle_points: 0,
        }
    }

    /// A flight of `scheduled` legs whose first legs, `flown`, were solved as given.
    pub(crate) fn flight(
        flown: Vec<(Segment, SegmentSolution)>,
        scheduled: usize,
    ) -> MissionResult {
        let (segments, solutions) = flown.into_iter().unzip();
        MissionResult {
            segments,
            solutions,
            scheduled_segment_count: scheduled,
            fuel_exhaustion: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use alas_mission::FuelExhaustion;

    use super::test_support::{climb, climbing_at, flight, holding, level, solution};
    use super::*;

    fn tags(schedule: &[SegmentSpec]) -> Vec<&str> {
        schedule.iter().map(|spec| spec.tag.as_str()).collect()
    }

    #[test]
    fn a_flight_starts_on_its_first_leg_and_on_each_plan_take_off() {
        let any = climb("initial_climb@01", None, 3_000.0, false);
        assert!(starts_a_flight(0, &any));
        assert!(!starts_a_flight(1, &any));
        assert!(starts_a_flight(
            5,
            &climb("takeoff@04", Some(0.0), 300.0, false)
        ));
        // A piece cut from the take-off starts part way up; the standard schedule's
        // `takeoff` leg after its take-off climb is not a start either.
        assert!(!starts_a_flight(
            6,
            &climb("takeoff@04_rated", Some(150.0), 300.0, true)
        ));
        assert!(!starts_a_flight(
            1,
            &climb("takeoff", Some(10.0), 300.0, false)
        ));
        let schedule = vec![
            climb("takeoff@00", Some(0.0), 300.0, false),
            climb("initial_climb@01", None, 3_000.0, false),
        ];
        assert!(keeps_start(&schedule, 0));
        assert!(!keeps_start(&schedule, 1));
        assert!(!keeps_start(&schedule, 2));
        assert!(family_starts_a_flight(&schedule, "takeoff@00"));
        assert!(!family_starts_a_flight(&schedule, "initial_climb@01"));
        // A climb whose level speed change into it is the first leg starts the flight with it.
        let schedule = vec![
            level("speed_change_to_climb@01", Some(0.0)),
            climb("climb@01", None, 3_000.0, true),
            climb("climb@01_path_exit", None, 3_100.0, false),
        ];
        assert!(family_starts_a_flight(&schedule, "climb@01"));
        assert!(speed_change_starts_a_flight(0, &schedule[0]));
        assert!(speed_change_starts_a_flight(
            3,
            &level("speed_change_to_takeoff@02", None)
        ));
        assert!(!speed_change_starts_a_flight(
            3,
            &level("speed_change_to_climb@02", None)
        ));
    }

    /// A leg the guidance cannot fly is removed after another leg, and kept as the first leg of a
    /// flight, where the leg after it would have nothing to start from.
    #[test]
    fn a_leg_is_removed_unless_a_flight_starts_on_it() {
        let mut schedule = vec![
            climb("climb@01", Some(0.0), 3_000.0, false),
            level("climb_2@01~entry", None),
            climb("climb_2@01", None, 6_000.0, false),
        ];
        assert_eq!(remove_leg(&mut schedule, 1), Some(()));
        assert_eq!(tags(&schedule), ["climb@01", "climb_2@01"]);

        let mut schedule = vec![
            level("climb@01~entry", Some(0.0)),
            climb("climb@01", None, 3_000.0, false),
        ];
        assert_eq!(remove_leg(&mut schedule, 0), None);
        assert_eq!(remove_leg(&mut schedule, 2), None);
        assert_eq!(tags(&schedule), ["climb@01~entry", "climb@01"]);
    }

    /// The first leg, at the rating, climbing at 0.2 m/s (below the 0.508 m/s residual climb)
    /// on a converged solve: the flight stops on it, and the refusal names the leg, where it
    /// starts, the mass and the excess power against the residual climb.
    #[test]
    fn a_first_climb_below_the_residual_rate_stops_the_flight_and_is_reported() {
        let first = climb("takeoff@00", Some(100.0), 600.0, true);
        let next = climb("initial_climb@01", Some(600.0), 3_000.0, false);
        let result = flight(
            vec![
                (climbing_at(&first, 0.2), solution(true)),
                (climbing_at(&next, 5.0), solution(true)),
            ],
            4,
        );
        assert_eq!(refused_first_climb(&result), Some(0));
        let stopped = stop_on(result, 0);
        assert_eq!(stopped.segments.len(), 1);
        assert_eq!(stopped.solutions.len(), 1);
        assert_eq!(stopped.scheduled_segment_count, 4);
        assert!(stopped.completed_summary().is_none());
        let refusal = first_climb_refusal(&stopped).unwrap_or_else(|| panic!("a refusal"));
        assert_eq!(refusal.tag, "takeoff@00");
        assert!(
            refusal
                .message
                .contains("takeoff@00, cannot climb from 100 m at 60000 kg"),
            "{}",
            refusal.message
        );
        assert!(
            refusal.message.contains("0.200 m/s at 100 m"),
            "{}",
            refusal.message
        );
        assert!((refusal.excess_power_m_s.unwrap_or(f64::NAN) - 0.2).abs() < 1.0e-9);
        assert_eq!(refusal.residual_climb_m_s, CLIMB_END_RESIDUAL_RATE_M_S);
    }

    /// An unconverged first climb at the rating stops the flight; the refusal quotes the solve.
    #[test]
    fn an_unconverged_first_climb_at_the_rating_is_reported_with_its_solve() {
        let first = climb("takeoff@00", Some(100.0), 600.0, true);
        let result = flight(vec![(climbing_at(&first, 0.2), solution(false))], 4);
        assert_eq!(refused_first_climb(&result), Some(0));
        let refusal = first_climb_refusal(&result).unwrap_or_else(|| panic!("a refusal"));
        assert_eq!(refusal.excess_power_m_s, None);
        assert!(
            refusal.message.contains("cannot climb from 100 m")
                && refusal.message.contains("segment 1 of 4 (takeoff@00)"),
            "{}",
            refusal.message
        );
    }

    /// A first climb flown at a commanded rate whose solve failed is not said to be unable to
    /// climb: the refusal carries the solve's own reason.
    #[test]
    fn a_first_climb_refused_off_the_rating_carries_its_own_reason() {
        let first = climb("takeoff@00", Some(100.0), 600.0, false);
        let result = flight(vec![(climbing_at(&first, 2.0), solution(false))], 4);
        assert_eq!(refused_first_climb(&result), None);
        let refusal = first_climb_refusal(&result).unwrap_or_else(|| panic!("a refusal"));
        assert_eq!(refusal.excess_power_m_s, None);
        assert!(
            !refusal.message.contains("cannot climb"),
            "{}",
            refusal.message
        );
        assert!(
            refusal.message.contains(
                "takeoff@00, a climb from 100 m at 60000 kg, was not flown: segment 1 of 4 \
                 (takeoff@00)"
            ),
            "{}",
            refusal.message
        );
    }

    /// The level speed change a flight starts on, into a climb at the rating whose speed it cannot
    /// reach: the climb is kept and there is no revision, so the flight stops on the speed change,
    /// and the run reports that stop with its own reason. After another leg the climb ends there
    /// and is flown level, as before.
    #[test]
    fn a_speed_change_into_the_first_climb_stops_the_flight_and_is_reported() {
        let mut schedule = vec![
            level("speed_change_to_takeoff@00", Some(100.0)),
            climb("takeoff@00", Some(100.0), 600.0, true),
            climb("initial_climb@01", None, 3_000.0, false),
        ];
        let before = schedule.clone();
        let change = holding(&schedule[0]);
        let mut notes = Vec::new();
        assert_eq!(
            end_climb_after_speed_change(&mut schedule, 0, Some(&change), 1.44, &mut notes, 0),
            None
        );
        assert_eq!(schedule, before);
        assert!(notes.is_empty());
        let result = flight(vec![(change, solution(false))], 3);
        let refusal = first_climb_refusal(&result).unwrap_or_else(|| panic!("a refusal"));
        assert_eq!(refusal.tag, "speed_change_to_takeoff@00");
        assert_eq!(refusal.excess_power_m_s, None);
        assert!(
            refusal.message.starts_with(
                "the first leg of the flight, speed_change_to_takeoff@00, the level speed change \
                 into its climb at 100 m and 60000 kg, was not flown: segment 1 of 3 \
                 (speed_change_to_takeoff@00)"
            ),
            "{}",
            refusal.message
        );

        let mut schedule = vec![
            climb("takeoff@00", Some(0.0), 600.0, false),
            level("speed_change_to_climb@01", Some(600.0)),
            climb("climb@01", None, 3_000.0, true),
            climb("climb@01_path_exit", None, 3_100.0, false),
            climb("climb_2@01", None, 6_000.0, false),
        ];
        let change = holding(&schedule[1]);
        assert_eq!(
            end_climb_after_speed_change(&mut schedule, 1, Some(&change), 1.44, &mut notes, 0),
            Some(())
        );
        assert_eq!(
            tags(&schedule),
            ["takeoff@00", "speed_change_to_climb@01", "climb_2@01"]
        );
        assert_eq!(notes.len(), 1);
        assert!(
            notes[0].starts_with(
                "climb@01: the level speed change to its speed needs 1.44 times the \
                 maximum-climb rating at 600 m"
            ),
            "{}",
            notes[0]
        );
        // A speed change stopped on after another leg is not a refused first climb.
        let later = flight(
            vec![
                (climbing_at(&schedule[0], 3.0), solution(true)),
                (change, solution(false)),
            ],
            3,
        );
        assert_eq!(first_climb_refusal(&later), None);
    }

    /// A first climb that holds the residual climb, a stop on a later leg, and a flight that ran
    /// out of fuel are not a refused first climb.
    #[test]
    fn other_stops_are_not_a_refused_first_climb() {
        let first = climb("takeoff@00", Some(100.0), 600.0, true);
        let next = climb("initial_climb@01", Some(600.0), 3_000.0, true);
        let holds = flight(vec![(climbing_at(&first, 3.0), solution(true))], 4);
        assert_eq!(refused_first_climb(&holds), None);
        assert_eq!(first_climb_refusal(&holds), None);
        let later = flight(
            vec![
                (climbing_at(&first, 3.0), solution(true)),
                (climbing_at(&next, 0.2), solution(false)),
            ],
            4,
        );
        assert_eq!(refused_first_climb(&later), None);
        assert_eq!(first_climb_refusal(&later), None);
        let mut dry = flight(vec![(climbing_at(&first, 0.2), solution(true))], 4);
        dry.fuel_exhaustion = Some(FuelExhaustion {
            segment_index: 0,
            segment_tag: "takeoff@00".to_owned(),
            available_fuel_kg: 1.0,
            burned_fuel_kg: 2.0,
            minimum_mass_kg: 50_000.0,
        });
        assert_eq!(first_climb_refusal(&dry), None);
    }
}
