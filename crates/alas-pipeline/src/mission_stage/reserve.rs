// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The reserve program of the standard mission, flown after the destination.
//!
//! The fuel policy (`alas_mass::fuel_policy`) carries a diversion to the
//! alternate and a final-reserve hold beside the trip. This module builds the
//! legs that fly them on the aircraft's own profile and drag:
//!
//! 1. a go-around from the destination field (the profile's take-off band),
//!    the climb to the diversion level, the cruise over the alternate
//!    distance and the descent down to the holding altitude
//!    (`approach`, flown with the same guidance as the trip);
//! 2. the hold: a level leg for the policy's time at the holding altitude and
//!    the speed of minimum fuel flow (`plan::loiter`), at the mass the aircraft
//!    arrives with;
//! 3. the landing from the holding altitude to touchdown at the alternate
//!    (`landing`).
//!
//! The alternate is taken at the destination's elevation (the configuration
//! carries a distance, not an alternate airport), as the priced model does
//! (`alas-opt` `diversion_ladders`). The diversion cruises at the highest level
//! of the profile's own ladder, as the priced model does (`climb_bands`), with
//! the altitude fit lowering it for a short alternate distance.
//!
//! Schemes: EASA basic, the study convention and the FAA flag rule fly a hold
//! of `final_reserve_hold_min` at the alternate (with no alternate, EASA basic
//! and the study convention first fly the 15 min no-alternate hold at the
//! destination). The FAA domestic rule's 45 min is cruise fuel consumption, not
//! a hold, and the flag rule's two-hour no-alternate allowance is cruise time:
//! both stay fuel allowances (priced by the burn model), not flown phases.
//! Contingency stays a share of trip fuel (or the 5 min hold floor), as the
//! regulations state it.

use alas_config::{FuelPolicyConfig, FuelScheme};
use alas_mission::segments::{MissionAnalyses, SegmentKind, SegmentSpec};
use alas_mission::{Mission, MissionRequest, MissionResult};
use alas_units::{FOOT, NAUTICAL_MILE};

use super::flight::{fly_guided, fly_with_guidance, Hooks};
use super::schedule::speed_change::{CruiseSpeedRule, SpeedMemory};
use super::schedule::{build_schedule_with_memory, cruise, schedule_horizontal_distance};
use super::sequence::{flown_phases, reserve_leg_phases, Phase, PhaseKind, Scope};

/// Control-point altitude tolerance when finding the holding level in a
/// descent, m.
const LEVEL_TOLERANCE_M: f64 = 0.5;

/// A guided leg of the reserve program.
pub(super) struct Leg {
    /// The schedule, with the drag of its configuration still to assign.
    pub schedule: Vec<SegmentSpec>,
    /// The request that closes its distance.
    pub request: MissionRequest,
    /// What the planner remembers about its speed changes.
    pub memory: SpeedMemory,
}

/// A hold at the holding altitude.
#[derive(Debug, Clone, Copy)]
pub(super) struct Hold {
    /// The leg's tag in the flown result.
    pub tag: &'static str,
    /// Duration, min.
    pub minutes: f64,
}

/// The reserve legs of a route, before they are flown.
pub(super) struct ReserveProgram {
    /// Go-around, diversion climb and cruise, descent to the holding altitude.
    pub approach: Option<Leg>,
    /// Holds, in flight order, at the holding altitude.
    pub holds: Vec<Hold>,
    /// Landing from the holding altitude.
    pub landing: Option<Leg>,
    /// Holding altitude above mean sea level, m.
    pub hold_altitude_m: f64,
}

/// The holds a scheme flies, in flight order.
fn holds_of(policy: &FuelPolicyConfig) -> Vec<Hold> {
    let final_reserve = Hold {
        tag: "reserve_hold",
        minutes: policy.final_reserve_hold_min,
    };
    let mut holds = Vec::new();
    match policy.scheme {
        // CAT.OP.MPA.181(c)(4)(ii): 15 min at the destination instead of an alternate. The study
        // convention reads its terms as given, so with no alternate it holds the final reserve only.
        FuelScheme::EasaBasic if policy.alternate_distance_nmi <= 0.0 => {
            holds.push(Hold {
                tag: "reserve_hold_no_alternate",
                minutes: 15.0,
            });
            holds.push(final_reserve);
        }
        FuelScheme::EasaBasic | FuelScheme::StudyConvention | FuelScheme::FaaFlagSupplemental => {
            holds.push(final_reserve);
        }
        FuelScheme::FaaDomestic | FuelScheme::TripFuelOnly => {}
    }
    holds.retain(|hold| hold.minutes > 0.0);
    holds
}

/// The reserve program of `request` under `policy`, or `None` when the scheme
/// carries nothing beyond the trip.
///
/// # Errors
///
/// The diversion schedule cannot be built or split at the holding altitude.
pub(super) fn build(
    request: &MissionRequest,
    policy: &FuelPolicyConfig,
    rule: Option<CruiseSpeedRule>,
) -> Result<Option<ReserveProgram>, String> {
    if !policy.scheme.carries_reserves() {
        return Ok(None);
    }
    let hold_altitude_m = request.arrival_elevation_m + policy.holding_altitude_ft * FOOT;
    let alternate_m = policy.alternate_distance_nmi * NAUTICAL_MILE;
    let holds = holds_of(policy);
    let (approach, landing) = if alternate_m > 0.0 {
        let (approach, landing) = diversion_legs(request, alternate_m, hold_altitude_m, rule)?;
        (Some(approach), Some(landing))
    } else {
        (None, None)
    };
    if approach.is_none() && holds.is_empty() {
        return Ok(None);
    }
    Ok(Some(ReserveProgram {
        approach,
        holds,
        landing,
        hold_altitude_m,
    }))
}

/// The diversion from the destination field to the holding altitude over the
/// alternate, and the landing below it.
fn diversion_legs(
    request: &MissionRequest,
    alternate_m: f64,
    hold_altitude_m: f64,
    rule: Option<CruiseSpeedRule>,
) -> Result<(Leg, Leg), String> {
    let mut profile = request.profile.clone();
    let shares = [
        profile.cruise_1_distance_fraction,
        profile.cruise_2_distance_fraction,
        profile.cruise_3_distance_fraction,
    ];
    // The ladder's top level carries the whole cruise (`alas-opt` `climb_bands`).
    let top = shares.iter().rposition(|share| *share > 0.0).unwrap_or(0);
    let top_share = |index| if index == top { 1.0 } else { 0.0 };
    profile.cruise_1_distance_fraction = top_share(0);
    profile.cruise_2_distance_fraction = top_share(1);
    profile.cruise_3_distance_fraction = top_share(2);
    let diversion = MissionRequest {
        mission_tag: format!("{}_diversion", request.mission_tag),
        cruise_altitude_m: request.cruise_altitude_m,
        departure_elevation_m: request.arrival_elevation_m,
        arrival_elevation_m: request.arrival_elevation_m,
        departure_isa_deviation_c: request.departure_isa_deviation_c,
        route_distance_m: alternate_m,
        profile,
    };
    let (schedule, memory) = build_schedule_with_memory(&diversion, rule, None, None)?;
    let (approach, landing) =
        split_at_hold(schedule, request.arrival_elevation_m, hold_altitude_m)?;
    let landing_distance_m = schedule_horizontal_distance(&landing, hold_altitude_m);
    let approach_distance_m = schedule_horizontal_distance(&approach, request.arrival_elevation_m);
    let landing_request = MissionRequest {
        mission_tag: format!("{}_landing", diversion.mission_tag),
        departure_elevation_m: hold_altitude_m,
        route_distance_m: landing_distance_m,
        ..diversion.clone()
    };
    // The landing has no cruise: the altitude fit must not reserve cruise share.
    let mut landing_memory = memory.clone();
    landing_memory.follow_priced_cruise(0.0);
    Ok((
        Leg {
            schedule: approach,
            request: MissionRequest {
                arrival_elevation_m: hold_altitude_m,
                route_distance_m: approach_distance_m,
                ..diversion
            },
            memory,
        },
        Leg {
            schedule: landing,
            request: landing_request,
            memory: landing_memory,
        },
    ))
}

/// Split the diversion schedule at the first descent through `hold_m`: the
/// approach ends at the holding altitude and the landing starts there.
fn split_at_hold(
    schedule: Vec<SegmentSpec>,
    start_elevation_m: f64,
    hold_m: f64,
) -> Result<(Vec<SegmentSpec>, Vec<SegmentSpec>), String> {
    let mut level_m = start_elevation_m;
    for (index, spec) in schedule.iter().enumerate() {
        match spec.kind {
            SegmentKind::Climb { altitude_end_m, .. } => level_m = altitude_end_m,
            SegmentKind::Cruise { altitude_m, .. } => level_m = altitude_m.unwrap_or(level_m),
            SegmentKind::Descent {
                altitude_start_m,
                altitude_end_m,
                descent_rate_m_s,
            } => {
                let start_m = altitude_start_m.unwrap_or(level_m);
                level_m = altitude_end_m;
                if start_m < hold_m - LEVEL_TOLERANCE_M
                    || altitude_end_m > hold_m + LEVEL_TOLERANCE_M
                {
                    continue;
                }
                let mut approach = schedule.clone();
                let mut landing = approach.split_off(index);
                if (start_m - hold_m).abs() <= LEVEL_TOLERANCE_M {
                    // The descent starts at the holding level.
                } else if (altitude_end_m - hold_m).abs() <= LEVEL_TOLERANCE_M {
                    approach.push(landing.remove(0));
                } else {
                    // The descent crosses the holding level: its upper part is a
                    // descent to it, its lower part the landing.
                    // Both parts keep the tag of the leg: they are flown by different legs of
                    // the program, and the tag carries the leg's configuration and speeds.
                    let mut upper = landing[0].clone();
                    upper.kind = SegmentKind::Descent {
                        altitude_start_m: Some(start_m),
                        altitude_end_m: hold_m,
                        descent_rate_m_s,
                    };
                    approach.push(upper);
                    landing[0].kind = SegmentKind::Descent {
                        altitude_start_m: Some(hold_m),
                        altitude_end_m,
                        descent_rate_m_s,
                    };
                }
                if landing.is_empty() {
                    return Err(
                        "the diversion has no landing below the holding altitude".to_owned()
                    );
                }
                return Ok((approach, landing));
            }
        }
    }
    Err("the diversion never descends through the holding altitude".to_owned())
}

impl ReserveProgram {
    /// The planned reserve phases, after the trip.
    pub(super) fn planned_phases(&self, destination_elevation_m: f64) -> Vec<Phase> {
        let mut phases = Vec::new();
        if let Some(approach) = &self.approach {
            phases.extend(reserve_leg_phases(
                &approach.schedule,
                destination_elevation_m,
                Scope::Approach,
            ));
        }
        for hold in &self.holds {
            phases.push(Phase {
                kind: PhaseKind::Hold,
                start_altitude_m: self.hold_altitude_m,
                end_altitude_m: self.hold_altitude_m,
                distance_m: 0.0,
                duration_s: hold.minutes * 60.0,
                fuel_kg: None,
            });
        }
        if let Some(landing) = &self.landing {
            phases.extend(reserve_leg_phases(
                &landing.schedule,
                self.hold_altitude_m,
                Scope::Landing,
            ));
        }
        phases
    }
}

/// What flying the reserve program needs beyond its legs.
pub(super) struct FlyContext<'a> {
    /// Assign the drag of the configuration of a leg (flaps, gear, stores).
    pub finish: &'a dyn Fn(&mut Vec<SegmentSpec>),
    /// Speed of minimum fuel flow at an altitude and mass, m/s.
    pub hold_speed_m_s: &'a dyn Fn(&MissionAnalyses, f64, f64) -> Option<f64>,
    /// ISA temperature deviation of the flown day, K.
    pub isa_deviation_k: f64,
}

/// The reserve program as flown.
#[derive(Debug, Clone, PartialEq)]
pub struct FlownReserves {
    /// The flown phases, in flight order.
    pub phases: Vec<Phase>,
    /// Fuel of the missed approach, diversion and landing at the alternate, kg.
    pub alternate_fuel_kg: f64,
    /// Fuel of each hold, kg, in flight order, with its duration, s.
    pub holds: Vec<(f64, f64)>,
    /// Mass the program started at (touchdown at the destination), kg.
    pub start_mass_kg: f64,
    /// Mass at the start of the first hold, kg (zero without a hold).
    pub first_hold_mass_kg: f64,
    /// Share of the cruise altitude the diversion was flown at: one unless the
    /// aircraft could not fly the profile's top level at the mass it lands with.
    pub level_scale: f64,
    /// The path changes of the reserve legs flown level, each with its leg and why.
    pub flown_level: Vec<String>,
}

impl FlownReserves {
    /// Fuel of the holds, kg.
    #[must_use]
    pub fn hold_fuel_kg(&self) -> f64 {
        self.holds.iter().map(|(fuel_kg, _)| fuel_kg).sum()
    }
}

fn completed(result: MissionResult, what: &str) -> Result<(MissionResult, f64), String> {
    let end_mass_kg = result
        .completed_summary()
        .map(|summary| summary.landing_mass_kg)
        .ok_or_else(|| {
            result.completion_refusal().map_or_else(
                || format!("the reserve {what} did not complete"),
                |refusal| format!("the reserve {what} did not complete: {refusal}"),
            )
        })?;
    Ok((result, end_mass_kg))
}

/// Fly the reserve program from `start_mass_kg` at the destination.
///
/// # Errors
///
/// A leg of the program that does not complete.
pub(super) fn fly(
    program: &ReserveProgram,
    analyses: &mut MissionAnalyses,
    start_mass_kg: f64,
    context: &FlyContext<'_>,
) -> Result<FlownReserves, String> {
    let mut flown = FlownReserves {
        phases: Vec::new(),
        alternate_fuel_kg: 0.0,
        holds: Vec::new(),
        start_mass_kg,
        first_hold_mass_kg: 0.0,
        level_scale: 1.0,
        flown_level: Vec::new(),
    };
    let mut mass_kg = start_mass_kg;
    let leg =
        |leg: &Leg, mass_kg: f64, analyses: &mut MissionAnalyses, what: &str, closing: bool| {
            let mut schedule = leg.schedule.clone();
            (context.finish)(&mut schedule);
            analyses.takeoff_mass_kg = mass_kg;
            // A reserve leg's level path changes are reported with the trip's
            // (`FlownReserves::flown_level`).
            let mut flown_level = Vec::new();
            let result = if closing {
                fly_with_guidance(
                    schedule,
                    &leg.request,
                    analyses,
                    leg.memory.clone(),
                    true,
                    &mut flown_level,
                )
            } else {
                // The landing has no cruise leg to absorb a revised footprint: it is flown
                // as built, with the guidance of the leg only.
                let mut reclose = |_: &mut Vec<SegmentSpec>, _: &SpeedMemory| Ok(());
                let hooks = Hooks {
                    reclose: &mut reclose,
                    changes: &|_| Vec::new(),
                    rate_passes: false,
                    adapt_idle_limited: false,
                    flown_level: &mut flown_level,
                };
                fly_guided(&mut schedule, analyses, leg.memory.clone(), hooks)
            }
            .map_err(|error| format!("the reserve {what} leg: {error}"))?;
            let notes: Vec<String> = flown_level
                .into_iter()
                .map(|note| format!("reserve {what} leg, {note}"))
                .collect();
            completed(result, what).map(|(result, end_kg)| (result, end_kg, notes))
        };
    if let Some(approach) = &program.approach {
        let (result, end_kg, notes) = leg(approach, mass_kg, analyses, "diversion", true)?;
        flown.flown_level.extend(notes);
        flown.alternate_fuel_kg += mass_kg - end_kg;
        flown.phases.extend(flown_phases(&result, Scope::Approach));
        mass_kg = end_kg;
    }
    for (index, hold) in program.holds.iter().enumerate() {
        if index == 0 {
            flown.first_hold_mass_kg = mass_kg;
        }
        let speed_m_s = (context.hold_speed_m_s)(analyses, program.hold_altitude_m, mass_kg)
            .ok_or_else(|| "the hold speed cannot be resolved for this drag model".to_owned())?;
        let duration_s = hold.minutes * 60.0;
        let mut spec = cruise(
            hold.tag,
            Some(program.hold_altitude_m),
            speed_m_s * duration_s,
            speed_m_s,
            context.isa_deviation_k,
        );
        let mut one = vec![spec.clone()];
        (context.finish)(&mut one);
        if let Some(finished) = one.pop() {
            spec = finished;
        }
        analyses.takeoff_mass_kg = mass_kg;
        let result = Mission {
            schedule: vec![spec],
        }
        .evaluate(analyses)
        .map_err(|error| format!("the reserve hold failed: {error}"))?;
        let (result, end_kg) = completed(result, "hold")?;
        let time_s = result.block_time_s();
        flown.holds.push((mass_kg - end_kg, time_s));
        for segment in &result.segments {
            let c = &segment.conditions;
            let (Some(first), Some(last)) = (c.aircraft_range_m.first(), c.aircraft_range_m.last())
            else {
                continue;
            };
            flown.phases.push(Phase {
                kind: PhaseKind::Hold,
                start_altitude_m: program.hold_altitude_m,
                end_altitude_m: program.hold_altitude_m,
                distance_m: last - first,
                duration_s: time_s,
                fuel_kg: Some(mass_kg - end_kg),
            });
        }
        mass_kg = end_kg;
    }
    if let Some(landing) = &program.landing {
        let (result, end_kg, notes) = leg(landing, mass_kg, analyses, "landing", false)?;
        flown.flown_level.extend(notes);
        flown.alternate_fuel_kg += mass_kg - end_kg;
        flown.phases.extend(flown_phases(&result, Scope::Landing));
    }
    Ok(flown)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_study_convention_without_an_alternate_holds_only_its_final_reserve() {
        let study = FuelPolicyConfig {
            scheme: FuelScheme::StudyConvention,
            alternate_distance_nmi: 0.0,
            ..Default::default()
        };
        let holds = holds_of(&study);
        assert_eq!(holds.len(), 1);
        assert_eq!(holds[0].tag, "reserve_hold");
        assert_eq!(holds[0].minutes, study.final_reserve_hold_min);
        // EASA basic keeps its 15 min no-alternate hold ahead of the final reserve.
        let easa = FuelPolicyConfig {
            alternate_distance_nmi: 0.0,
            ..Default::default()
        };
        let tags: Vec<_> = holds_of(&easa).iter().map(|hold| hold.tag).collect();
        assert_eq!(tags, ["reserve_hold_no_alternate", "reserve_hold"]);
    }
}
