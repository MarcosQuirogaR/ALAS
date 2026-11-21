// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Closing the distances of a plan after the legs have been revised.
//!
//! A guidance revision changes a climb or descent rate, which changes the
//! footprint of the leg. A cruise that was asked to close a distance (the
//! route, or a diversion) takes what the other legs leave; a leg flown for a
//! time is as long as its speed makes it. Speed changes between legs are
//! joined again, as the standard mission's closure does.

use std::collections::HashMap;

use alas_mission::segments::{SegmentKind, SegmentSpec};

use crate::mission_stage::schedule::junction::{join_speeds, merge_pieces};
use crate::mission_stage::schedule::speed_change::{is_speed_change, SpeedMemory};
use crate::mission_stage::schedule::{fit_altitude_profile, schedule_horizontal_distance};

use super::legs::{stage_of_tag, ClosureGroup};

/// Horizontal still-air distance of each leg of `specs`, m, walking the
/// altitude from `initial_altitude_m`.
pub(super) fn horizontal_distances(specs: &[SegmentSpec], initial_altitude_m: f64) -> Vec<f64> {
    let mut altitude_m = initial_altitude_m;
    specs
        .iter()
        .map(|spec| {
            let distance = schedule_horizontal_distance(std::slice::from_ref(spec), altitude_m);
            if let SegmentKind::Climb { altitude_end_m, .. }
            | SegmentKind::Descent { altitude_end_m, .. } = spec.kind
            {
                altitude_m = altitude_end_m;
            }
            distance
        })
        .collect()
}

/// What the plan remembers between revisions of its schedule.
pub(super) struct Closer {
    /// Legs flown for a time, s, by tag.
    pub time_s: HashMap<String, f64>,
    /// Distances to close.
    pub groups: Vec<ClosureGroup>,
    /// Whether each stage advances along the route (a stage that is not a
    /// reserve and moves forward).
    pub on_route: Vec<bool>,
    /// Whether the legs that bring each stage to its level and speed fly along
    /// the route although its timed leg does not (a loiter or combat that is
    /// not a reserve, see [`super::legs::entry_on_route`]).
    pub entry_on_route: Vec<bool>,
    /// Field elevation the plan starts from, m.
    pub start_elevation_m: f64,
    /// The id of each stage, for messages.
    pub stage_ids: Vec<String>,
    /// A stage the last closing found shorter than its own legs.
    pub shortfall: std::cell::RefCell<Option<String>>,
}

impl Closer {
    /// Whether `spec` covers part of the route's distance: every leg of a stage
    /// that advances along it, and of a held stage every leg but the timed one
    /// (the climb or descent to its level and the speed change into it).
    fn covers_route(&self, spec: &SegmentSpec) -> bool {
        let flag = |flags: &[bool], stage: usize| flags.get(stage).copied().unwrap_or(false);
        stage_of_tag(&spec.tag).is_some_and(|stage| {
            flag(&self.on_route, stage)
                || (flag(&self.entry_on_route, stage) && !self.time_s.contains_key(&spec.tag))
        })
    }

    /// Ground distance a flown `result` covers over the distance of a closure group, m: for
    /// the route (`None`) the legs that cover it, for a stage its own legs.
    pub(super) fn flown_distance_m(
        &self,
        result: &alas_mission::MissionResult,
        stage: Option<usize>,
    ) -> f64 {
        result
            .segments
            .iter()
            .filter(|segment| match stage {
                None => self.covers_route(&segment.spec),
                Some(_) => stage_of_tag(&segment.spec.tag) == stage,
            })
            .filter_map(|segment| {
                let range = &segment.conditions.aircraft_range_m;
                Some(range.last()? - range.first()?)
            })
            .sum()
    }

    /// The stage the last closing found shorter than its own legs, if any: an
    /// error of the plan when it is the closing of the schedule that was flown.
    pub(super) fn take_shortfall(&self) -> Option<String> {
        self.shortfall.borrow_mut().take()
    }

    /// Lower the altitude profile of the legs `indices` (consecutive legs that
    /// are not speed changes) until they fit `distance_m` with the profile's
    /// minimum share left for cruise, as the standard mission does for a short
    /// route.
    fn fit(
        &self,
        schedule: &mut Vec<SegmentSpec>,
        indices: &[usize],
        end_elevation_m: f64,
        distance_m: f64,
        memory: &SpeedMemory,
    ) -> Result<(), String> {
        let (Some(&first), Some(&last)) = (indices.first(), indices.last()) else {
            return Ok(());
        };
        let start_altitude_m = altitude_before(schedule, first, self.start_elevation_m);
        let nominal: Vec<SegmentSpec> = indices.iter().map(|&i| schedule[i].clone()).collect();
        let fitted = fit_altitude_profile(
            &nominal,
            start_altitude_m,
            end_elevation_m,
            distance_m,
            memory,
        )?;
        schedule.splice(first..=last, fitted);
        Ok(())
    }

    /// Fit the altitude profile of every stage with its own distance, and of
    /// the route block when its legs run without a break.
    fn fit_profiles(
        &self,
        schedule: &mut Vec<SegmentSpec>,
        memory: &SpeedMemory,
    ) -> Result<(), String> {
        for group in self
            .groups
            .iter()
            .filter(|group| group.stage.is_some() && group.fit)
        {
            let indices: Vec<usize> = schedule
                .iter()
                .enumerate()
                .filter(|(_, spec)| {
                    stage_of_tag(&spec.tag) == group.stage && !is_speed_change(spec)
                })
                .map(|(index, _)| index)
                .collect();
            self.fit(
                schedule,
                &indices,
                group.end_elevation_m,
                group.total_m,
                memory,
            )?;
        }
        let Some(route) = self.groups.iter().find(|group| group.stage.is_none()) else {
            return Ok(());
        };
        let on_route = |spec: &SegmentSpec| {
            stage_of_tag(&spec.tag)
                .is_some_and(|stage| self.on_route.get(stage).copied().unwrap_or(false))
        };
        let indices: Vec<usize> = schedule
            .iter()
            .enumerate()
            .filter(|(_, spec)| on_route(spec) && !is_speed_change(spec))
            .map(|(index, _)| index)
            .collect();
        let (Some(&first), Some(&last)) = (indices.first(), indices.last()) else {
            return Ok(());
        };
        // A level leg between route legs (a hold at the cruise level) takes no
        // part in the fit: it follows the altitude it finds. A climb or descent
        // that belongs to another stage carries altitudes the plan states, which
        // the fit must not move: the block is then flown as stated, and a route
        // too short for it is an error of the closure.
        let level = |spec: &SegmentSpec| matches!(spec.kind, SegmentKind::Cruise { .. });
        let unbroken = schedule[first..=last]
            .iter()
            .all(|spec| on_route(spec) || is_speed_change(spec) || level(spec));
        if !unbroken {
            return Ok(());
        }
        let block: Vec<usize> = (first..=last)
            .filter(|&index| !is_speed_change(&schedule[index]))
            .collect();
        let route_tags: Vec<&String> = self
            .groups
            .iter()
            .filter(|group| group.stage.is_none())
            .flat_map(|group| &group.cruise_tags)
            .collect();
        let fixed_m: f64 = indices
            .iter()
            .filter_map(|&index| match schedule[index].kind {
                SegmentKind::Cruise { distance_m, .. }
                    if !route_tags.contains(&&schedule[index].tag) =>
                {
                    Some(distance_m)
                }
                _ => None,
            })
            .sum();
        self.fit(
            schedule,
            &block,
            route.end_elevation_m,
            (route.total_m - fixed_m).max(0.0),
            memory,
        )
    }

    /// Join the speeds, set the timed legs to their speed, and size the cruise
    /// legs that close a distance.
    ///
    /// # Errors
    ///
    /// A message when the route is shorter than what the other legs need.
    pub(super) fn close(
        &self,
        schedule: &mut Vec<SegmentSpec>,
        memory: &SpeedMemory,
    ) -> Result<(), String> {
        *self.shortfall.borrow_mut() = None;
        // Pieces cut by an earlier joining go back into their legs, and every
        // climb and descent starts where the leg before it ends: the legs of one
        // stage are refitted, the others must follow.
        let merged = merge_pieces(
            std::mem::take(schedule)
                .into_iter()
                .filter(|spec| !is_speed_change(spec))
                .collect(),
        );
        *schedule = merged;
        let mut altitude_m = self.start_elevation_m;
        for spec in schedule.iter_mut() {
            match &mut spec.kind {
                SegmentKind::Climb {
                    altitude_start_m,
                    altitude_end_m,
                    ..
                }
                | SegmentKind::Descent {
                    altitude_start_m,
                    altitude_end_m,
                    ..
                } => {
                    if altitude_start_m.is_some() {
                        *altitude_start_m = Some(altitude_m);
                    }
                    altitude_m = *altitude_end_m;
                }
                SegmentKind::Cruise { .. } => {}
            }
        }
        self.fit_profiles(schedule, memory)?;
        join_speeds(schedule, self.start_elevation_m, memory);
        for spec in schedule.iter_mut() {
            if let (Some(seconds), SegmentKind::Cruise { altitude_m, .. }) =
                (self.time_s.get(&spec.tag), spec.kind)
            {
                spec.kind = SegmentKind::Cruise {
                    altitude_m,
                    distance_m: spec.air_speed_m_s * seconds,
                };
            }
        }
        let distances = horizontal_distances(schedule, self.start_elevation_m);
        let route_cruise: Vec<&str> = self
            .groups
            .iter()
            .filter(|group| group.stage.is_none())
            .flat_map(|group| group.cruise_tags.iter().map(String::as_str))
            .collect();
        let mut sized: Vec<(String, f64)> = Vec::new();
        // The stages with a distance of their own are sized first: a cruise of one of them that
        // advances along the route (a return leg of a stated length) is part of what the route
        // closure leaves to the route's cruise, at the length it is flown with, not the one the
        // schedule held before this revision (which moves with every revision of the climbs).
        for group in self.groups.iter().filter(|group| group.stage.is_some()) {
            let others: f64 = schedule
                .iter()
                .zip(&distances)
                .filter(|(spec, _)| {
                    stage_of_tag(&spec.tag) == group.stage && !group.cruise_tags.contains(&spec.tag)
                })
                .map(|(_, distance)| distance)
                .sum();
            let remainder = group.total_m - others;
            if remainder < 0.0 {
                // Midway through the guidance a leg can be momentarily long (a
                // level change integrated at a state the next revision moves),
                // so the stage is only refused if its legs of the final flight
                // still leave it no room: see [`Closer::take_shortfall`].
                let id = group
                    .stage
                    .and_then(|stage| self.stage_ids.get(stage))
                    .map_or("?", String::as_str);
                let legs: Vec<String> = schedule
                    .iter()
                    .zip(&distances)
                    .filter(|(spec, distance)| {
                        stage_of_tag(&spec.tag) == group.stage
                            && !group.cruise_tags.contains(&spec.tag)
                            && **distance > 1.0e3
                    })
                    .map(|(spec, distance)| format!("{} {:.0} km", spec.tag, distance / 1.0e3))
                    .collect();
                *self.shortfall.borrow_mut() = Some(format!(
                    "stage '{id}' covers {:.0} km, less than the {:.0} km of its own climbs and descents ({})",
                    group.total_m / 1000.0,
                    others / 1000.0,
                    legs.join(", ")
                ));
            }
            let remainder = remainder.max(0.0);
            for tag in &group.cruise_tags {
                sized.push((tag.clone(), remainder / group.cruise_tags.len() as f64));
            }
        }
        if !route_cruise.is_empty() {
            let total = self
                .groups
                .iter()
                .find(|group| group.stage.is_none())
                .map_or(0.0, |group| group.total_m);
            let others: f64 = schedule
                .iter()
                .zip(&distances)
                .filter(|(spec, _)| {
                    self.covers_route(spec) && !route_cruise.contains(&spec.tag.as_str())
                })
                .map(|(spec, distance)| {
                    sized
                        .iter()
                        .find(|(tag, _)| *tag == spec.tag)
                        .map_or(*distance, |(_, sized_m)| *sized_m)
                })
                .sum();
            let remainder = total - others;
            if remainder < 0.0 {
                return Err(format!(
                    "the route of {:.0} km is shorter than the {:.0} km the other stages of the plan cover",
                    total / 1000.0,
                    others / 1000.0
                ));
            }
            for tag in &route_cruise {
                sized.push(((*tag).to_owned(), remainder / route_cruise.len() as f64));
            }
        }
        for spec in schedule.iter_mut().filter(|spec| !is_speed_change(spec)) {
            if let (Some((_, distance_m)), SegmentKind::Cruise { altitude_m, .. }) =
                (sized.iter().find(|(tag, _)| *tag == spec.tag), spec.kind)
            {
                spec.kind = SegmentKind::Cruise {
                    altitude_m,
                    distance_m: *distance_m,
                };
            }
        }
        Ok(())
    }
}

/// The altitude the aircraft is at before leg `index`, m.
fn altitude_before(schedule: &[SegmentSpec], index: usize, initial_altitude_m: f64) -> f64 {
    let mut altitude_m = initial_altitude_m;
    for spec in &schedule[..index] {
        if let SegmentKind::Climb { altitude_end_m, .. }
        | SegmentKind::Descent { altitude_end_m, .. } = spec.kind
        {
            altitude_m = altitude_end_m;
        }
    }
    altitude_m
}

// Tests assert on schedules they just built, so a failed unwrap is the
// assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use alas_config::mission::{SpeedCaps, SpeedReference};

    use super::*;

    fn leg(tag: &str, kind: SegmentKind) -> SegmentSpec {
        SegmentSpec {
            tag: tag.to_owned(),
            kind,
            air_speed_m_s: 200.0,
            air_speed_reference: SpeedReference::TrueAirspeed,
            speed_caps: SpeedCaps::default(),
            true_course_rad: 0.0,
            temperature_deviation_k: 0.0,
            number_control_points: 16,
            configuration_delta_cd: 0.0,
            load_factor: 1.0,
            entry_speed_m_s: 0.0,
            exit_speed_m_s: 0.0,
            speed_profile: Vec::new(),
            takeoff_rated: false,
            rating_limited: false,
        }
    }

    fn level(tag: &str) -> SegmentSpec {
        leg(
            tag,
            SegmentKind::Cruise {
                altitude_m: None,
                distance_m: 0.0,
            },
        )
    }

    fn descent(tag: &str, end_m: f64) -> SegmentSpec {
        leg(
            tag,
            SegmentKind::Descent {
                altitude_start_m: None,
                altitude_end_m: end_m,
                descent_rate_m_s: 8.0,
            },
        )
    }

    /// Raymer's commercial loiter (p. 32) sits at 10,000 ft below the cruise:
    /// the descent the loiter stage inserts to reach it, and the deceleration
    /// to the holding speed, fly along the route and are closed with it, while
    /// the hold itself returns to its fix and is not. Before LOITER-DIST the
    /// route cruise also took those legs' distance, and the flight overshot the
    /// route by that much.
    #[test]
    fn the_legs_into_a_trip_loiter_are_part_of_the_route_and_the_hold_is_not() {
        let route_m = 500.0e3;
        let hold_speed_m_s = 120.0;
        let closer = Closer {
            time_s: HashMap::from([("loiter@01".to_owned(), 1200.0)]),
            groups: vec![ClosureGroup {
                cruise_tags: vec!["cruise@00".to_owned()],
                stage: None,
                total_m: route_m,
                end_elevation_m: 0.0,
                fit: false,
            }],
            on_route: vec![true, false, true],
            entry_on_route: vec![false, true, false],
            start_elevation_m: 10_000.0,
            stage_ids: vec!["cruise".into(), "loiter".into(), "descent".into()],
            shortfall: std::cell::RefCell::new(None),
        };
        let mut hold = level("loiter@01");
        hold.air_speed_m_s = hold_speed_m_s;
        let mut schedule = vec![
            level("cruise@00"),
            descent("descent_1@01", 3048.0),
            hold,
            descent("descent_4@02", 0.0),
        ];
        closer
            .close(&mut schedule, &SpeedMemory::default())
            .expect("the route is long enough");
        let distances = horizontal_distances(&schedule, closer.start_elevation_m);
        let legs: Vec<(&str, f64)> = schedule
            .iter()
            .zip(&distances)
            .map(|(spec, distance)| (spec.tag.as_str(), *distance))
            .collect();
        let distance_of = |tag: &str| legs.iter().find(|(t, _)| *t == tag).unwrap().1;
        assert!(distance_of("descent_1@01") > 100.0e3, "{legs:?}");
        assert!((distance_of("loiter@01") - hold_speed_m_s * 1200.0).abs() < 1.0e-6);
        // The descent and the deceleration into the hold (here a piece cut
        // from the descent; a level speed change where the descent cannot
        // hold it) are on the route.
        let into_hold: Vec<&SegmentSpec> = schedule
            .iter()
            .filter(|spec| stage_of_tag(&spec.tag) == Some(1) && spec.tag != "loiter@01")
            .collect();
        assert!(into_hold.len() >= 2, "the hold is entered slower: {legs:?}");
        assert!(into_hold.iter().all(|spec| closer.covers_route(spec)));
        // Everything but the hold is the route, to the closure's own estimate.
        let off_route: f64 = schedule
            .iter()
            .zip(&distances)
            .filter(|(spec, _)| !closer.covers_route(spec))
            .map(|(_, distance)| distance)
            .sum();
        assert_eq!(off_route, distance_of("loiter@01"));
        let flown: f64 = distances.iter().sum::<f64>() - off_route;
        assert!(
            (flown - route_m).abs() < 1.0e-6,
            "the route legs cover {flown} m of {route_m} m: {legs:?}"
        );
    }
}
