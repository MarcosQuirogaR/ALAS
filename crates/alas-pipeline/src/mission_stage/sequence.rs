// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The phases of the standard mission as one ordered list.
//!
//! The same list describes the plan (built from the segment schedules the run
//! is built from, before anything is flown) and the flight (read back from the
//! flown segments), so the Mission page and the run cannot disagree about what
//! the mission is: [`planned_standard_mission`] and the flown
//! [`flown_phases`] group segments by the same rule.
//!
//! Units: altitudes are metres above mean sea level, distances are still-air
//! ground distances in metres, durations are seconds, fuel is kilograms. The
//! hold has a duration but no planned distance: its ground distance is the
//! endurance speed (which depends on the mass and drag it is flown at) times
//! its duration, known only once it is flown.

use alas_mission::segments::{SegmentKind, SegmentSpec};
use alas_mission::MissionResult;

use super::reserve::ReserveProgram;
use super::schedule::junction::family;
use super::schedule::schedule_horizontal_distance;

/// What a phase of the standard mission is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseKind {
    /// Ground time at idle before brake release (not a flown segment).
    TaxiOut,
    /// Lift-off and the climb to the acceleration height.
    Takeoff,
    /// The initial climb to the first cruise level.
    Climb,
    /// A step climb between two cruise levels.
    StepClimb,
    /// Level cruise at one level (including its level speed changes).
    Cruise,
    /// The descent from the last cruise level to the landing configuration.
    Descent,
    /// The landing configuration, from its height to touchdown.
    Landing,
    /// Ground time at idle after landing (not a flown segment).
    TaxiIn,
    /// Reserve: the go-around climb from the destination.
    MissedApproach,
    /// Reserve: the climb to the diversion level.
    DiversionClimb,
    /// Reserve: the cruise to the alternate.
    DiversionCruise,
    /// Reserve: the descent to the holding level at the alternate.
    DiversionDescent,
    /// Reserve: the final-reserve hold at the holding altitude.
    Hold,
    /// Reserve: the landing at the alternate.
    DiversionLanding,
}

impl PhaseKind {
    /// Whether the phase belongs to the reserve program (flown after the
    /// destination, never part of the nominal trip).
    #[must_use]
    pub const fn is_reserve(self) -> bool {
        matches!(
            self,
            Self::MissedApproach
                | Self::DiversionClimb
                | Self::DiversionCruise
                | Self::DiversionDescent
                | Self::Hold
                | Self::DiversionLanding
        )
    }

    /// Whether the phase is on the ground and not a flown segment.
    #[must_use]
    pub const fn is_ground(self) -> bool {
        matches!(self, Self::TaxiOut | Self::TaxiIn)
    }

    /// Whether consecutive phases of this kind are one phase to a pilot (the
    /// rungs of a descent, the pieces of a climb).
    const fn merges(self) -> bool {
        matches!(
            self,
            Self::Takeoff
                | Self::Climb
                | Self::StepClimb
                | Self::Descent
                | Self::Landing
                | Self::MissedApproach
                | Self::DiversionClimb
                | Self::DiversionDescent
                | Self::DiversionLanding
        )
    }
}

/// One phase of the standard mission.
#[derive(Debug, Clone, PartialEq)]
pub struct Phase {
    /// What it is.
    pub kind: PhaseKind,
    /// Altitude at its start, m.
    pub start_altitude_m: f64,
    /// Altitude at its end, m.
    pub end_altitude_m: f64,
    /// Ground distance, m (zero on the ground).
    pub distance_m: f64,
    /// Duration, s (an estimate for a planned phase, the flown time for a
    /// flown one).
    pub duration_s: f64,
    /// Fuel burned, kg: `None` for a planned phase.
    pub fuel_kg: Option<f64>,
}

impl Phase {
    /// Whether the phase is a reserve phase.
    #[must_use]
    pub const fn is_reserve(&self) -> bool {
        self.kind.is_reserve()
    }
}

/// The standard mission before it is flown.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedMission {
    /// Every phase in flight order: taxi-out, the nominal trip, taxi-in, then
    /// the reserve program (flagged by [`Phase::is_reserve`]).
    pub phases: Vec<Phase>,
    /// What the route or the configuration forces the plan to differ from the
    /// request by: a shortened level, a step climb that does not fit.
    pub warnings: Vec<String>,
}

/// Which part of the mission a list of segments belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The nominal trip.
    Trip,
    /// The reserve program from the go-around to the holding altitude.
    Approach,
    /// The reserve program from the holding altitude to touchdown.
    Landing,
}

/// A group of segments of one leg, before phases are merged.
#[derive(Debug, Clone)]
struct Piece {
    tag: String,
    start_m: f64,
    end_m: f64,
    distance_m: f64,
    duration_s: f64,
    fuel_kg: Option<f64>,
}

/// The phase kind of a leg tag, for the nominal trip.
fn nominal_kind(tag: &str) -> Option<PhaseKind> {
    let tag = family(tag);
    if tag.starts_with("takeoff") {
        Some(PhaseKind::Takeoff)
    } else if tag.starts_with("initial_climb") {
        Some(PhaseKind::Climb)
    } else if tag.starts_with("step_climb") {
        Some(PhaseKind::StepClimb)
    } else if tag.starts_with("cruise") {
        Some(PhaseKind::Cruise)
    } else if tag.starts_with("descent") {
        Some(PhaseKind::Descent)
    } else if tag.starts_with("final_landing") {
        Some(PhaseKind::Landing)
    } else {
        None
    }
}

/// The reserve counterpart of a nominal kind.
const fn reserve_kind(kind: PhaseKind) -> PhaseKind {
    match kind {
        PhaseKind::Takeoff => PhaseKind::MissedApproach,
        PhaseKind::Climb | PhaseKind::StepClimb => PhaseKind::DiversionClimb,
        PhaseKind::Cruise => PhaseKind::DiversionCruise,
        PhaseKind::Descent => PhaseKind::DiversionDescent,
        PhaseKind::Landing => PhaseKind::DiversionLanding,
        other => other,
    }
}

/// Group `pieces` into phases. A level piece that is no leg of its own (a speed
/// change) joins the phase before it; consecutive pieces of a kind that
/// [`PhaseKind::merges`] are one phase.
fn group(pieces: &[Piece], scope: Scope) -> Vec<Phase> {
    let mut phases: Vec<Phase> = Vec::new();
    let mut last_family: Option<&str> = None;
    let mut cruised = false;
    for piece in pieces {
        // A climb rung flown before any cruise leg (a speed change of the climb schedule) is part
        // of the climb; only a climb between two cruise levels is a step climb.
        let kind = nominal_kind(&piece.tag).map(|kind| match kind {
            PhaseKind::StepClimb if !cruised => PhaseKind::Climb,
            PhaseKind::Cruise => {
                cruised = true;
                kind
            }
            other => other,
        });
        let kind = kind.map(|kind| {
            match scope {
                Scope::Trip => kind,
                // The diversion ends its descent at the holding altitude: the landing
                // below it is the leg after the hold.
                Scope::Approach if kind == PhaseKind::Landing => PhaseKind::DiversionDescent,
                Scope::Approach | Scope::Landing => reserve_kind(kind),
            }
        });
        let joins = match (kind, phases.last()) {
            (None, Some(_)) => true,
            // The pieces cut from one leg (path entry and exit) are that leg.
            (Some(kind), Some(last)) => {
                (kind == last.kind && kind.merges()) || last_family == Some(family(&piece.tag))
            }
            _ => false,
        };
        let fuel = |a: Option<f64>, b: Option<f64>| match (a, b) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(0.0) + b.unwrap_or(0.0)),
        };
        if joins {
            if let Some(last) = phases.last_mut() {
                last.end_altitude_m = piece.end_m;
                last.distance_m += piece.distance_m;
                last.duration_s += piece.duration_s;
                last.fuel_kg = fuel(last.fuel_kg, piece.fuel_kg);
            }
        } else if let Some(kind) = kind {
            phases.push(Phase {
                kind,
                start_altitude_m: piece.start_m,
                end_altitude_m: piece.end_m,
                distance_m: piece.distance_m,
                duration_s: piece.duration_s,
                fuel_kg: piece.fuel_kg,
            });
        }
        if nominal_kind(&piece.tag).is_some() {
            last_family = Some(family(&piece.tag));
        }
    }
    phases
}

/// The pieces of a schedule that has not been flown: altitudes carried from
/// `start_altitude_m`, distances from the leg kinematics, durations from the
/// planned rates and speeds.
fn pieces_of_specs(specs: &[SegmentSpec], start_altitude_m: f64) -> Vec<Piece> {
    let mut level_m = start_altitude_m;
    let mut pieces = Vec::with_capacity(specs.len());
    for spec in specs {
        let (start_m, end_m, duration_s) = match spec.kind {
            SegmentKind::Climb {
                altitude_start_m,
                altitude_end_m,
                climb_rate_m_s: rate,
            }
            | SegmentKind::Descent {
                altitude_start_m,
                altitude_end_m,
                descent_rate_m_s: rate,
            } => {
                let start = altitude_start_m.unwrap_or(level_m);
                (
                    start,
                    altitude_end_m,
                    (altitude_end_m - start).abs() / rate.abs().max(f64::MIN_POSITIVE),
                )
            }
            SegmentKind::Cruise {
                altitude_m,
                distance_m,
            } => {
                let at = altitude_m.unwrap_or(level_m);
                (
                    at,
                    at,
                    distance_m / spec.air_speed_m_s.max(f64::MIN_POSITIVE),
                )
            }
        };
        let distance_m = schedule_horizontal_distance(std::slice::from_ref(spec), start_m);
        level_m = end_m;
        pieces.push(Piece {
            tag: spec.tag.clone(),
            start_m,
            end_m,
            distance_m,
            duration_s,
            fuel_kg: None,
        });
    }
    pieces
}

/// The phases of a flown result (`reserve` selects the reserve kinds).
#[must_use]
pub fn flown_phases(result: &MissionResult, scope: Scope) -> Vec<Phase> {
    let pieces: Vec<Piece> = result
        .segments
        .iter()
        .filter_map(|segment| {
            let conditions = &segment.conditions;
            let (first, last) = (
                conditions.altitude_m.first().copied()?,
                conditions.altitude_m.last().copied()?,
            );
            let span = |values: &[f64]| Some(values.last().copied()? - values.first().copied()?);
            Some(Piece {
                tag: segment.spec.tag.clone(),
                start_m: first,
                end_m: last,
                distance_m: span(&conditions.aircraft_range_m)?,
                duration_s: span(&conditions.time_s)?,
                fuel_kg: span(&conditions.total_mass_kg).map(|change| -change),
            })
        })
        .collect();
    group(&pieces, scope)
}

/// The phases of the planned trip schedule and the planned reserve program,
/// with the ground phases around the trip.
pub(super) fn planned_phases(
    trip: &[SegmentSpec],
    departure_elevation_m: f64,
    arrival_elevation_m: f64,
    taxi_out_s: f64,
    taxi_in_s: f64,
    reserve: Option<&ReserveProgram>,
) -> Vec<Phase> {
    let ground = |kind, elevation_m, duration_s| Phase {
        kind,
        start_altitude_m: elevation_m,
        end_altitude_m: elevation_m,
        distance_m: 0.0,
        duration_s,
        fuel_kg: None,
    };
    let mut phases = Vec::new();
    if taxi_out_s > 0.0 {
        phases.push(ground(
            PhaseKind::TaxiOut,
            departure_elevation_m,
            taxi_out_s,
        ));
    }
    phases.extend(group(
        &pieces_of_specs(trip, departure_elevation_m),
        Scope::Trip,
    ));
    if taxi_in_s > 0.0 {
        phases.push(ground(PhaseKind::TaxiIn, arrival_elevation_m, taxi_in_s));
    }
    if let Some(program) = reserve {
        phases.extend(program.planned_phases(arrival_elevation_m));
    }
    phases
}

/// The phases of one planned reserve leg (`reserve` kinds).
pub(super) fn reserve_leg_phases(
    specs: &[SegmentSpec],
    start_altitude_m: f64,
    scope: Scope,
) -> Vec<Phase> {
    group(&pieces_of_specs(specs, start_altitude_m), scope)
}

/// What the route forces the plan to differ from the configuration by.
pub(super) fn warnings(
    phases: &[Phase],
    configured_cruise_legs: usize,
    configured_top_m: f64,
) -> Vec<String> {
    let trip: Vec<&Phase> = phases.iter().filter(|phase| !phase.is_reserve()).collect();
    let mut notes = Vec::new();
    let top_m = trip
        .iter()
        .filter(|phase| !phase.kind.is_ground())
        .map(|phase| phase.end_altitude_m.max(phase.start_altitude_m))
        .fold(f64::NEG_INFINITY, f64::max);
    if top_m.is_finite() && top_m < configured_top_m - 5.0 {
        notes.push(format!(
            "the route is too short for the configured levels: the top level is {top_m:.0} m instead of {configured_top_m:.0} m"
        ));
    }
    let cruise_legs = trip
        .iter()
        .filter(|phase| phase.kind == PhaseKind::Cruise && phase.distance_m > 1.0)
        .count();
    if cruise_legs < configured_cruise_legs {
        notes.push(format!(
            "the route cannot carry the configured {configured_cruise_legs} cruise legs: {cruise_legs} are flown"
        ));
    }
    let steps = trip
        .iter()
        .filter(|phase| phase.kind == PhaseKind::StepClimb)
        .count();
    if steps + 1 < configured_cruise_legs {
        notes.push(format!(
            "the route cannot carry the configured {} step climbs: {steps} are flown",
            configured_cruise_legs - 1
        ));
    }
    notes
}
