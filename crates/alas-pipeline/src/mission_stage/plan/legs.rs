// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! From plan stages to the segments that fly them.
//!
//! Every stage becomes one or more legs of the kinds the segment network
//! solves (climb, cruise, descent), on the speed and rate schedule of the
//! aircraft's own [`MissionProfileConfig`]: a climb follows the profile's
//! climb rungs up the altitude as the standard mission's does, a descent
//! follows its descent ladder, a landing the landing speed and sink rate.
//! The tag of a leg is `<name>@<stage index>`, which is how a flown segment is
//! attributed to its stage after the guidance has split, merged or joined it.

use std::collections::HashMap;

use alas_config::mission::{MissionProfileConfig, SpeedReference};
use alas_config::{CruiseExtent, MissionPlan, StageKind};
use alas_mission::segments::SegmentSpec;

use crate::mission_stage::schedule::speed::apply_speed_caps;
use crate::mission_stage::schedule::speed_change::SpeedMemory;
use crate::mission_stage::schedule::{climb, cruise, descent};

use super::METRES_PER_FOOT;

/// The stage a leg tag belongs to: the digits after the last `@`.
pub(crate) fn stage_of_tag(tag: &str) -> Option<usize> {
    let (_, tail) = tag.rsplit_once('@')?;
    let digits: String = tail.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// A leg is a part of the route when its stage advances along it.
pub(super) fn advances_route(kind: &StageKind) -> bool {
    matches!(
        kind,
        StageKind::Takeoff { .. }
            | StageKind::Climb { .. }
            | StageKind::Cruise { .. }
            | StageKind::Descent { .. }
            | StageKind::Approach { .. }
            | StageKind::Landing { .. }
    )
}

/// A stage held at a point of the route (a loiter, a combat): its timed leg
/// returns to where it began, but the climb or descent to its altitude and the
/// speed change into it fly along the route.
pub(super) fn entry_on_route(kind: &StageKind) -> bool {
    matches!(kind, StageKind::Loiter { .. } | StageKind::Combat { .. })
}

/// How the cruise legs that close a distance are sized again after a revision.
#[derive(Debug, Clone)]
pub(super) struct ClosureGroup {
    /// Tags of the cruise legs that take the remainder.
    pub cruise_tags: Vec<String>,
    /// `None`: the route; `Some(stage)`: that stage's own distance.
    pub stage: Option<usize>,
    /// The distance to close, m.
    pub total_m: f64,
    /// Elevation of the field the legs end at, m.
    pub end_elevation_m: f64,
    /// Whether the altitude profile of the stage is lowered to fit the
    /// distance (a diversion); a cruise that states its distance keeps its
    /// altitudes and takes what the stage's other legs leave of it.
    pub fit: bool,
}

/// The segments of a plan, and what must be remembered about them.
pub(super) struct Built {
    /// The legs in flight order.
    pub specs: Vec<SegmentSpec>,
    /// Speed memory with the plan's cruise legs declared.
    pub memory: SpeedMemory,
    /// Legs flown for a time, s, by tag.
    pub time_s: HashMap<String, f64>,
    /// Distances to close.
    pub groups: Vec<ClosureGroup>,
    /// Field elevation the plan starts from, m.
    pub start_elevation_m: f64,
}

/// What building the legs needs from the aircraft and the run.
pub(super) struct Context<'a> {
    pub profile: &'a MissionProfileConfig,
    pub departure_elevation_m: f64,
    pub arrival_elevation_m: f64,
    pub isa_deviation_k: f64,
    pub route_distance_m: f64,
    /// Standard-day to flown-day speed ratio at the altitude the profile's
    /// cruise speeds are declared at.
    pub cruise_speed_ratio: f64,
    /// Loiter speed at an altitude and mass, m/s.
    pub loiter_speed_m_s: &'a dyn Fn(f64, f64) -> Option<f64>,
    /// The mass the aircraft is expected to have at the start of a stage, kg.
    pub mass_hint_kg: &'a dyn Fn(usize) -> f64,
    /// The cruise-speed rule of the priced route, when it is available.
    pub cruise_rule: Option<crate::mission_stage::schedule::speed_change::CruiseSpeedRule>,
}

fn ft(value: f64) -> f64 {
    value * METRES_PER_FOOT
}

pub(super) struct Rung {
    end_altitude_m: f64,
    speed_m_s: f64,
    rate_m_s: f64,
}

mod ramps;

struct Builder<'a, 'c> {
    ctx: &'c Context<'a>,
    specs: Vec<SegmentSpec>,
    memory: SpeedMemory,
    time_s: HashMap<String, f64>,
    groups: Vec<ClosureGroup>,
    stage: usize,
    altitude_m: f64,
    ground_m: f64,
    /// Field of the landing or approach that follows the current stage, m.
    next_field_m: Option<f64>,
    names: HashMap<String, usize>,
}

impl Builder<'_, '_> {
    fn tag(&mut self, name: &str) -> String {
        let count = self
            .names
            .entry(format!("{name}@{}", self.stage))
            .or_insert(0);
        *count += 1;
        if *count == 1 {
            format!("{name}@{:02}", self.stage)
        } else {
            format!("{name}_{count}@{:02}", self.stage)
        }
    }

    fn reference(&self) -> SpeedReference {
        self.ctx.profile.climb_descent_speed_reference
    }

    fn push_climb(&mut self, name: &str, end_m: f64, speed_m_s: f64, rate_m_s: f64, start: bool) {
        let tag = self.tag(name);
        let start_m = start.then_some(self.altitude_m);
        let spec = climb(
            &tag,
            start_m,
            end_m,
            speed_m_s,
            self.reference(),
            rate_m_s,
            self.ctx.isa_deviation_k,
        );
        self.specs.push(spec);
        self.altitude_m = end_m;
    }

    fn push_descent(&mut self, name: &str, end_m: f64, speed_m_s: f64, rate_m_s: f64) {
        let tag = self.tag(name);
        let spec = descent(
            &tag,
            None,
            end_m,
            speed_m_s,
            self.reference(),
            rate_m_s,
            self.ctx.isa_deviation_k,
        );
        self.specs.push(spec);
        self.altitude_m = end_m;
    }

    fn declared_cruise_speed(&self) -> f64 {
        self.ctx.profile.cruise_3_air_speed_m_s * self.ctx.cruise_speed_ratio
    }

    /// A level leg for a time or a distance.
    fn level(&mut self, name: &str, extent: CruiseExtent, speed: Option<f64>, load_factor: f64) {
        let tag = self.tag(name);
        let declared = speed.is_none().then(|| self.declared_cruise_speed());
        let speed_m_s = speed.unwrap_or_else(|| self.declared_cruise_speed());
        let distance_m = match extent {
            CruiseExtent::DistanceKm(km) => {
                self.groups.push(ClosureGroup {
                    cruise_tags: vec![tag.clone()],
                    stage: Some(self.stage),
                    total_m: km * 1000.0,
                    end_elevation_m: self.altitude_m,
                    fit: false,
                });
                km * 1000.0
            }
            CruiseExtent::TimeMin(min) => {
                self.time_s.insert(tag.clone(), min * 60.0);
                speed_m_s * min * 60.0
            }
            CruiseExtent::Remainder => {
                self.groups.push(ClosureGroup {
                    cruise_tags: vec![tag.clone()],
                    stage: None,
                    total_m: self.ctx.route_distance_m,
                    end_elevation_m: self.ctx.arrival_elevation_m,
                    fit: false,
                });
                0.0
            }
        };
        if let Some(declared) = declared {
            self.memory.declare_cruise(&tag, declared);
        }
        let mut spec = cruise(&tag, None, distance_m, speed_m_s, self.ctx.isa_deviation_k);
        spec.load_factor = load_factor;
        self.specs.push(spec);
    }

    /// A hold flown at a constant speed: the stage's own, else the speed of
    /// minimum fuel flow at the mean of the masses the hold starts and ends
    /// with (the next stage's start mass). The best speed falls with the
    /// square root of the mass, so a long hold drifts from it by about a
    /// quarter of its fractional mass loss either side of the mean.
    fn hold(&mut self, time_min: f64, speed: Option<f64>) -> Result<(), String> {
        let speed = match speed {
            Some(speed) => speed,
            None => {
                let start = (self.ctx.mass_hint_kg)(self.stage);
                let end = (self.ctx.mass_hint_kg)(self.stage + 1).min(start);
                (self.ctx.loiter_speed_m_s)(self.altitude_m, 0.5 * (start + end)).ok_or_else(
                    || {
                        "a hold without a speed needs the aircraft's own drag table to find the speed of minimum fuel flow; give the stage a speed".to_owned()
                    },
                )?
            }
        };
        self.level("loiter", CruiseExtent::TimeMin(time_min), Some(speed), 1.0);
        Ok(())
    }

    fn landing_leg(&mut self, name: &str, end_m: f64) {
        let p = self.ctx.profile;
        let speed = p.landing_air_speed_m_s;
        let rate = p.landing_descent_rate_m_s;
        self.push_descent(name, end_m, speed, rate);
    }

    fn field_m(&self, elevation_ft: Option<f64>) -> f64 {
        elevation_ft.map_or(self.ctx.arrival_elevation_m, ft)
    }

    fn stage(&mut self, kind: &StageKind) -> Result<(), String> {
        let p = self.ctx.profile;
        match kind {
            StageKind::Taxi { .. } => {}
            StageKind::Takeoff { .. } => {
                self.ground_m = self.altitude_m;
                let end = self.altitude_m + p.takeoff_altitude_gain_m;
                let tag = self.tag("takeoff");
                let spec = climb(
                    &tag,
                    Some(self.altitude_m),
                    end,
                    p.takeoff_air_speed_m_s,
                    self.reference(),
                    p.takeoff_climb_rate_m_s,
                    self.ctx.isa_deviation_k,
                );
                self.specs.push(spec);
                self.altitude_m = end;
            }
            StageKind::Climb {
                altitude_ft,
                agl,
                rate_m_s,
            } => {
                let target = ft(*altitude_ft) + if *agl { self.ground_m } else { 0.0 };
                if target <= self.altitude_m + 1.0 {
                    return Err(format!(
                        "the climb to {altitude_ft:.0} ft starts at {:.0} ft: it must go up",
                        self.altitude_m / METRES_PER_FOOT
                    ));
                }
                self.climb_to(target, *rate_m_s);
            }
            StageKind::Cruise {
                extent,
                altitude_ft,
                agl,
                speed_m_s,
            } => {
                self.reach(*altitude_ft, *agl);
                self.level("cruise", *extent, *speed_m_s, 1.0);
            }
            StageKind::Loiter {
                time_min,
                altitude_ft,
                agl,
                speed_m_s,
            } => {
                self.reach(*altitude_ft, *agl);
                self.hold(*time_min, *speed_m_s)?;
            }
            StageKind::Combat {
                time_min,
                load_factor,
                altitude_ft,
                agl,
                speed_m_s,
            } => {
                self.reach(*altitude_ft, *agl);
                self.level(
                    "combat",
                    CruiseExtent::TimeMin(*time_min),
                    *speed_m_s,
                    *load_factor,
                );
            }
            StageKind::Descent { altitude_ft, agl } => {
                let target = ft(*altitude_ft) + if *agl { self.ground_m } else { 0.0 };
                // A rung not above the field the flight is landing on is not flown,
                // as in the standard schedule: the landing stage takes the descent.
                if self.next_field_m.is_some_and(|field_m| target <= field_m) {
                    return Ok(());
                }
                if target >= self.altitude_m - 1.0 {
                    return Err(format!(
                        "the descent to {:.0} ft above mean sea level starts at {:.0} ft: it must go down; lower the stage's altitude_ft below {:.0} ft",
                        target / METRES_PER_FOOT,
                        self.altitude_m / METRES_PER_FOOT,
                        (self.altitude_m - if *agl { self.ground_m } else { 0.0 }) / METRES_PER_FOOT
                    ));
                }
                self.descend_to(target);
            }
            StageKind::Approach {
                decision_height_ft,
                field_elevation_ft,
            } => {
                self.ground_m = self.field_m(*field_elevation_ft);
                let decision = self.ground_m + ft(*decision_height_ft);
                self.descend_to(self.ladder_end_m(self.ground_m, decision));
                if self.altitude_m > decision + 1.0 {
                    self.landing_leg("approach", decision);
                }
            }
            StageKind::GoAround { altitude_ft, agl } => {
                let target = ft(*altitude_ft) + if *agl { self.ground_m } else { 0.0 };
                if target <= self.altitude_m + 1.0 {
                    return Err(format!(
                        "the go-around to {:.0} ft above mean sea level starts at {:.0} ft: it must go up; raise the stage's altitude_ft above {:.0} ft or set it above the field with agl",
                        target / METRES_PER_FOOT,
                        self.altitude_m / METRES_PER_FOOT,
                        (self.altitude_m - if *agl { self.ground_m } else { 0.0 }) / METRES_PER_FOOT
                    ));
                }
                let first = target.min(self.altitude_m + p.takeoff_altitude_gain_m);
                self.push_climb(
                    "go_around",
                    first,
                    p.takeoff_air_speed_m_s,
                    p.takeoff_climb_rate_m_s,
                    false,
                );
                self.climb_to(target, None);
            }
            StageKind::Diversion {
                distance_km,
                cruise_altitude_ft,
                field_elevation_ft,
            } => self.diversion(*distance_km, *cruise_altitude_ft, *field_elevation_ft),
            StageKind::Landing { field_elevation_ft } => {
                self.ground_m = self.field_m(*field_elevation_ft);
                self.descend_to(self.ladder_end_m(self.ground_m, self.ground_m));
                if self.altitude_m > self.ground_m + 1.0 {
                    self.landing_leg("final_landing", self.ground_m);
                }
            }
        }
        Ok(())
    }
}

/// Build the legs of every stage of `plan`.
pub(super) fn build(plan: &MissionPlan, ctx: &Context<'_>) -> Result<Built, String> {
    let mut builder = Builder {
        ctx,
        specs: Vec::new(),
        memory: SpeedMemory::for_profile(ctx.profile, ctx.cruise_rule.clone()).following_nominal(),
        time_s: HashMap::new(),
        groups: Vec::new(),
        stage: 0,
        altitude_m: ctx.departure_elevation_m,
        ground_m: ctx.departure_elevation_m,
        next_field_m: None,
        names: HashMap::new(),
    };
    let mut route_end_m = None;
    for (index, stage) in plan.stages.iter().enumerate() {
        builder.stage = index;
        builder.next_field_m = plan.stages.get(index + 1).and_then(|next| match next.kind {
            StageKind::Approach {
                field_elevation_ft, ..
            }
            | StageKind::Landing { field_elevation_ft } => {
                Some(builder.field_m(field_elevation_ft))
            }
            _ => None,
        });
        builder
            .stage(&stage.kind)
            .map_err(|message| format!("stage '{}': {message}", stage.id))?;
        if !stage.reserve
            && matches!(
                stage.kind,
                StageKind::Approach { .. } | StageKind::Landing { .. }
            )
        {
            route_end_m = Some(builder.ground_m);
        }
    }
    for group in builder
        .groups
        .iter_mut()
        .filter(|group| group.stage.is_none())
    {
        group.end_elevation_m = route_end_m.unwrap_or(ctx.arrival_elevation_m);
    }
    apply_speed_caps(&mut builder.specs, ctx.profile);
    Ok(Built {
        specs: builder.specs,
        memory: builder.memory,
        time_s: builder.time_s,
        groups: builder.groups,
        start_elevation_m: ctx.departure_elevation_m,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod high_field_tests {
    use super::*;
    use alas_config::mission::LANDING_CONFIGURATION_HEIGHT_M;
    use alas_mission::segments::SegmentKind;

    const FIELD_M: f64 = 2_230.0; // 7,316 ft, above the 3,000 ft last rung.

    fn legs_of(plan: &MissionPlan, arrival_m: f64) -> Vec<SegmentSpec> {
        let profile = MissionProfileConfig::default();
        let loiter = |_: f64, _: f64| Some(120.0);
        let mass = |_: usize| 60_000.0;
        let context = Context {
            profile: &profile,
            departure_elevation_m: 0.0,
            arrival_elevation_m: arrival_m,
            isa_deviation_k: 0.0,
            route_distance_m: 1.0e6,
            cruise_speed_ratio: 1.0,
            loiter_speed_m_s: &loiter,
            mass_hint_kg: &mass,
            cruise_rule: None,
        };
        build(plan, &context).unwrap().specs
    }

    /// The lowest altitude a descent ends at, and the start of the last leg, m.
    fn descent_ends(legs: &[SegmentSpec]) -> Vec<(String, f64)> {
        legs.iter()
            .filter_map(|leg| match leg.kind {
                SegmentKind::Descent { altitude_end_m, .. } => {
                    Some((leg.tag.clone(), altitude_end_m))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_landing_above_the_last_rung_keeps_its_leg_and_stays_above_the_ground() {
        let plan = MissionPlan::from_profile(&MissionProfileConfig::default(), 35_000.0);
        let ends = descent_ends(&legs_of(&plan, FIELD_M));
        assert!(
            ends.iter().all(|(_, end_m)| *end_m >= FIELD_M - 1.0e-6),
            "{ends:?}"
        );
        let (last, end_m) = ends.last().unwrap();
        assert!(last.starts_with("final_landing"), "{ends:?}");
        assert!((end_m - FIELD_M).abs() < 1.0e-6);
        // The 10,000 ft rung lies within the configuration height of this field.
        let (_, before_m) = &ends[ends.len() - 2];
        assert!(
            (before_m - 10_000.0 * METRES_PER_FOOT).abs() < 1.0e-6,
            "{ends:?}"
        );
    }

    #[test]
    fn a_landing_well_below_the_next_rung_starts_at_the_configuration_height() {
        let plan = MissionPlan::from_profile(&MissionProfileConfig::default(), 35_000.0);
        let field_m = 1_900.0;
        let ends = descent_ends(&legs_of(&plan, field_m));
        let (_, before_m) = &ends[ends.len() - 2];
        assert!(
            (before_m - (field_m + LANDING_CONFIGURATION_HEIGHT_M)).abs() < 1.0e-6,
            "{ends:?}"
        );
    }

    #[test]
    fn a_landing_below_the_last_rung_starts_at_the_rung() {
        let plan = MissionPlan::from_profile(&MissionProfileConfig::default(), 35_000.0);
        let ends = descent_ends(&legs_of(&plan, 0.0));
        let (_, before_m) = &ends[ends.len() - 2];
        assert!(
            (before_m - 3_000.0 * METRES_PER_FOOT).abs() < 1.0e-6,
            "{ends:?}"
        );
    }
}
