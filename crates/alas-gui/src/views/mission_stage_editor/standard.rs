// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The standard mission as the pipeline plans and flies it.
//!
//! A case without stages is flown from `mission.profile`. The Mission page
//! draws that flight from the phase list the pipeline builds for the route
//! (`alas_pipeline::planned_standard_mission`), not from a generic five-stage
//! template: the taxi, the cruise legs, the step climbs between them, the
//! descent, the landing and the reserve program shown are the ones the run is
//! built from, in the order it flies them. The physics stays in `alas-mission`
//! and `alas-pipeline`; this module only reads the phases and turns them into
//! the stage list the plot and the list draw.

use alas_config::mission::MissionProfileConfig;
use alas_config::{AlasConfig, CruiseExtent, MissionPlan, PlanStage, StageKind};
use alas_pipeline::{Phase, PhaseKind, PlannedMission, ReachCause};

use crate::views::{tr_fields, tr_label};

const METRES_PER_FOOT: f64 = 0.3048;

/// What a leg of the planned flight is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegKind {
    TaxiOut,
    Takeoff,
    Climb,
    StepClimb,
    Cruise,
    /// The descent rungs down to the landing configuration, as one leg.
    Descent,
    Landing,
    TaxiIn,
    /// Reserve: the go-around from the destination.
    MissedApproach,
    /// Reserve: the climb to the diversion level.
    DiversionClimb,
    /// Reserve: the cruise to the alternate.
    DiversionCruise,
    /// Reserve: the descent to the holding altitude.
    DiversionDescent,
    /// Reserve: the final-reserve hold.
    Hold,
    /// Reserve: the landing at the alternate.
    DiversionLanding,
}

impl LegKind {
    fn of(kind: PhaseKind) -> Self {
        match kind {
            PhaseKind::TaxiOut => Self::TaxiOut,
            PhaseKind::Takeoff => Self::Takeoff,
            PhaseKind::Climb => Self::Climb,
            PhaseKind::StepClimb => Self::StepClimb,
            PhaseKind::Cruise => Self::Cruise,
            PhaseKind::Descent => Self::Descent,
            PhaseKind::Landing => Self::Landing,
            PhaseKind::TaxiIn => Self::TaxiIn,
            PhaseKind::MissedApproach => Self::MissedApproach,
            PhaseKind::DiversionClimb => Self::DiversionClimb,
            PhaseKind::DiversionCruise => Self::DiversionCruise,
            PhaseKind::DiversionDescent => Self::DiversionDescent,
            PhaseKind::Hold => Self::Hold,
            PhaseKind::DiversionLanding => Self::DiversionLanding,
        }
    }

    /// Whether the leg is a level leg held for a distance (a cruise).
    fn is_cruise(self) -> bool {
        matches!(self, Self::Cruise | Self::DiversionCruise)
    }
}

/// One leg of the planned flight.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Leg {
    pub kind: LegKind,
    pub start_m: f64,
    pub end_m: f64,
    /// Ground distance of a cruise leg, m; zero for the others.
    pub distance_m: f64,
    /// Ground distance of the leg, m, whatever its kind (the phase's own figure).
    pub ground_m: f64,
    /// Duration, s (the estimate of the plan).
    pub duration_s: f64,
    /// Fuel the leg burned, kg, when the flight was flown (none for a planned leg).
    pub fuel_kg: Option<f64>,
    /// Whether the leg belongs to the reserve program.
    pub reserve: bool,
    /// The phase editor that owns this leg's parameters, when it has one.
    pub phase: Option<&'static str>,
}

/// The planned flight of the standard mission, ready to draw.
#[derive(Debug, Clone)]
pub(crate) struct Standard {
    pub legs: Vec<Leg>,
    /// The stage list the plot is drawn from (one stage per leg).
    pub plan: MissionPlan,
    /// The row text of each leg.
    pub rows: Vec<String>,
    /// The hover text of each row (metres, and a cruise leg's distance).
    pub tips: Vec<String>,
    /// Cruise legs flown (read by the flown-plan tests).
    #[cfg_attr(not(test), allow(dead_code))]
    pub cruise_legs: usize,
    /// What the route forces the plan to differ from the configuration by.
    pub warnings: Vec<String>,
    /// Whether this is the nominal schematic shown while the flight of the case is computed.
    pub computing: bool,
}

/// The editor name of the `ordinal`-th active entry of the three-slot profile
/// ladder (`names`), given which slots `active` marks.
fn ladder_phase(
    active: [bool; 3],
    ordinal: usize,
    names: [&'static str; 3],
) -> Option<&'static str> {
    active
        .iter()
        .enumerate()
        .filter_map(|(slot, on)| on.then_some(slot))
        .nth(ordinal)
        .map(|slot| names[slot])
}

/// The legs of a planned mission.
pub(crate) fn legs_of(planned: &PlannedMission, profile: &MissionProfileConfig) -> Vec<Leg> {
    let active = [
        profile.cruise_1_distance_fraction > 0.0,
        profile.cruise_2_distance_fraction > 0.0,
        profile.cruise_3_distance_fraction > 0.0,
    ];
    let (mut cruises, mut step_climbs) = (0, 0);
    planned
        .phases
        .iter()
        .map(|phase: &Phase| {
            let kind = LegKind::of(phase.kind);
            let phase_name = match phase.kind {
                PhaseKind::Takeoff => Some("takeoff"),
                PhaseKind::Climb => Some("initial_climb"),
                PhaseKind::StepClimb => {
                    step_climbs += 1;
                    // `step_climb_1` is flown when the second cruise leg is active, `step_climb_2`
                    // when the third is.
                    ladder_phase(
                        [active[1], active[2], false],
                        step_climbs - 1,
                        ["step_climb_1", "step_climb_2", "step_climb_2"],
                    )
                }
                PhaseKind::Cruise => {
                    cruises += 1;
                    ladder_phase(active, cruises - 1, ["cruise_1", "cruise_2", "cruise_3"])
                }
                PhaseKind::Descent => Some("descent_1"),
                PhaseKind::Landing => Some("final_approach"),
                _ => None,
            };
            Leg {
                kind,
                start_m: phase.start_altitude_m,
                end_m: phase.end_altitude_m,
                distance_m: if kind.is_cruise() {
                    phase.distance_m
                } else {
                    0.0
                },
                ground_m: phase.distance_m,
                duration_s: phase.duration_s,
                fuel_kg: phase.fuel_kg,
                reserve: phase.is_reserve(),
                phase: phase_name,
            }
        })
        .collect()
}

/// The planned flight of the case on its route, from the pipeline's own
/// phase list.
///
/// # Errors
///
/// The airports do not resolve or the pipeline refuses the profile on this
/// route, as text.
pub(crate) fn planned(config: &AlasConfig, route_distance_m: f64) -> Result<Standard, String> {
    let (origin, destination) =
        crate::views::mission_profile_inputs::resolved_route_airports(config)
            .ok_or_else(|| "the airports do not resolve".to_owned())?;
    let mission =
        alas_pipeline::planned_standard_mission(config, &origin, &destination, route_distance_m)?;
    let mut standard = display(&legs_of(&mission, &config.mission.profile));
    standard.warnings = mission.warnings;
    Ok(standard)
}

/// The highest cruise level of the trip (reserves excluded), m.
fn trip_top_m(legs: &[Leg]) -> f64 {
    legs.iter()
        .filter(|leg| leg.kind == LegKind::Cruise)
        .map(|leg| leg.end_m)
        .fold(f64::NEG_INFINITY, f64::max)
}

/// Whether the diversion cruises a flight level or more below the trip top: the alternate's climb
/// and descent footprint did not fit under it.
fn diversion_fitted(diversion_m: f64, trip_top_m: f64) -> bool {
    trip_top_m.is_finite() && trip_top_m - diversion_m >= 100.0 * METRES_PER_FOOT
}

/// Flight level of an altitude in metres, as "FL370".
pub(crate) fn flight_level(altitude_m: f64) -> String {
    format!("FL{:.0}", (altitude_m / METRES_PER_FOOT / 100.0).max(0.0))
}

fn feet(altitude_m: f64) -> f64 {
    altitude_m / METRES_PER_FOOT
}

fn minutes(leg: &Leg) -> String {
    format!("{:.0}", leg.duration_s / 60.0)
}

/// The stage list and texts for a set of legs.
pub(crate) fn display(legs: &[Leg]) -> Standard {
    let cruise_legs = legs
        .iter()
        .filter(|leg| leg.kind == LegKind::Cruise && leg.distance_m > 0.0)
        .count();
    let cruise_total_m: f64 = legs
        .iter()
        .filter(|leg| leg.kind == LegKind::Cruise)
        .map(|leg| leg.distance_m)
        .sum();
    let mut stages = Vec::with_capacity(legs.len());
    let mut rows = Vec::with_capacity(legs.len());
    let mut tips = Vec::with_capacity(legs.len());
    let trip_top_m = trip_top_m(legs);
    let mut cruise_seen = 0;
    for (index, leg) in legs.iter().enumerate() {
        let id = format!("standard-{}", index + 1);
        let level = flight_level(leg.end_m);
        let altitude_text = format!("{:.0} ft", feet(leg.end_m));
        let climb = |text: String| {
            (
                StageKind::Climb {
                    altitude_ft: feet(leg.end_m),
                    agl: false,
                    rate_m_s: None,
                },
                text,
            )
        };
        let descent = |text: String| {
            (
                StageKind::Descent {
                    altitude_ft: feet(leg.end_m),
                    agl: false,
                },
                text,
            )
        };
        let cruise = |text: String| {
            (
                StageKind::Cruise {
                    extent: CruiseExtent::DistanceKm((leg.distance_m / 1000.0).max(0.001)),
                    altitude_ft: Some(feet(leg.end_m)),
                    agl: false,
                    speed_m_s: None,
                },
                text,
            )
        };
        let (kind, text) = match leg.kind {
            LegKind::TaxiOut => (
                StageKind::Taxi {
                    time_min: leg.duration_s / 60.0,
                },
                tr_fields(
                    "Taxi out \u{b7} {minutes} min",
                    &[("minutes", minutes(leg))],
                ),
            ),
            LegKind::TaxiIn => (
                StageKind::Taxi {
                    time_min: leg.duration_s / 60.0,
                },
                tr_fields("Taxi in \u{b7} {minutes} min", &[("minutes", minutes(leg))]),
            ),
            LegKind::Takeoff => (StageKind::Takeoff { field: None }, tr_label("Takeoff")),
            LegKind::Climb => climb(tr_fields("Climb \u{2192} {level}", &[("level", level)])),
            LegKind::StepClimb => climb(tr_fields(
                "Step climb \u{2192} {level}",
                &[("level", level)],
            )),
            LegKind::Cruise => {
                cruise_seen += 1;
                cruise(if cruise_legs > 1 {
                    tr_fields(
                        "Cruise {n} \u{b7} {level}",
                        &[("n", cruise_seen.to_string()), ("level", level)],
                    )
                } else {
                    tr_fields("Cruise \u{b7} {level}", &[("level", level)])
                })
            }
            LegKind::Descent => descent(tr_fields(
                "Descent \u{2192} {altitude}",
                &[("altitude", altitude_text)],
            )),
            LegKind::Landing => (
                StageKind::Landing {
                    field_elevation_ft: None,
                },
                tr_label("Landing"),
            ),
            LegKind::MissedApproach => (
                StageKind::GoAround {
                    altitude_ft: feet(leg.end_m),
                    agl: false,
                },
                tr_label("Missed approach"),
            ),
            LegKind::DiversionClimb => climb(tr_fields(
                "Diversion climb \u{2192} {level}",
                &[("level", level)],
            )),
            LegKind::DiversionCruise => cruise(if diversion_fitted(leg.end_m, trip_top_m) {
                tr_fields(
                    "Diversion cruise \u{b7} {level} (fitted)",
                    &[("level", level)],
                )
            } else {
                tr_fields("Diversion cruise \u{b7} {level}", &[("level", level)])
            }),
            LegKind::DiversionDescent => descent(tr_fields(
                "Diversion descent \u{2192} {altitude}",
                &[("altitude", altitude_text)],
            )),
            LegKind::Hold => (
                StageKind::Loiter {
                    time_min: leg.duration_s / 60.0,
                    altitude_ft: Some(feet(leg.end_m)),
                    agl: false,
                    speed_m_s: None,
                },
                tr_fields(
                    "Hold \u{b7} {minutes} min at {altitude}",
                    &[("minutes", minutes(leg)), ("altitude", altitude_text)],
                ),
            ),
            LegKind::DiversionLanding => (
                StageKind::Landing {
                    field_elevation_ft: None,
                },
                tr_label("Landing at the alternate"),
            ),
        };
        let tip = match leg.kind {
            LegKind::Cruise => tr_fields(
                "{altitude} m \u{b7} {distance} km ({share} % of the cruise)",
                &[
                    ("altitude", format!("{:.0}", leg.end_m)),
                    ("distance", format!("{:.0}", leg.distance_m / 1000.0)),
                    (
                        "share",
                        format!("{:.0}", 100.0 * leg.distance_m / cruise_total_m.max(1.0)),
                    ),
                ],
            ),
            LegKind::DiversionCruise => {
                let fitted = if diversion_fitted(leg.end_m, trip_top_m) {
                    format!(" \u{b7} {}", tr_label("fitted to the alternate"))
                } else {
                    String::new()
                };
                format!(
                    "{:.0} m \u{b7} {:.0} km{fitted}",
                    leg.end_m,
                    leg.distance_m / 1000.0
                )
            }
            LegKind::TaxiOut | LegKind::TaxiIn => format!("{:.0} min", leg.duration_s / 60.0),
            _ => format!("{:.0} m", leg.end_m),
        };
        let mut stage = PlanStage::new(&id, kind);
        stage.reserve = leg.reserve;
        stages.push(stage);
        rows.push(if leg.reserve {
            format!("{}. {}  ({})", index + 1, text, tr_label("Reserve"))
        } else {
            format!("{}. {}", index + 1, text)
        });
        tips.push(tip);
    }
    Standard {
        legs: legs.to_vec(),
        plan: MissionPlan {
            name: "Standard mission".to_owned(),
            stages,
            ..MissionPlan::default()
        },
        rows,
        tips,
        cruise_legs,
        warnings: Vec::new(),
        computing: false,
    }
}

/// The flight the pipeline's mission stage flies for the case (see `planned_flight`), as the
/// legs the page draws, with the reason when the top level was lowered to what the aircraft holds.
pub(crate) fn from_flight(config: &AlasConfig, flight: &super::planned_flight::Flown) -> Standard {
    let mut standard = display(&legs_of(
        &PlannedMission {
            phases: flight.phases.clone(),
            warnings: Vec::new(),
        },
        &config.mission.profile,
    ));
    if let Some(sizing) = flight.sizing {
        if !sizing.converged {
            standard.warnings.push(tr_fields(
                "Fuel sizing did not converge (mass {mass} kg, fuel {fuel} kg short)",
                &[
                    ("mass", format!("{:.0}", sizing.mass_residual_kg)),
                    (
                        "fuel",
                        format!("{:.0}", (-sizing.fuel_residual_kg).max(0.0)),
                    ),
                ],
            ));
        }
        if sizing.clamped {
            standard.warnings.push(tr_label(
                "Flown fuel differs from the priced model beyond its limit",
            ));
        }
    }
    if let Some(note) = flight.reach_note {
        let template = match note.cause {
            ReachCause::Mass => "Top level {flown} (set {configured}): not holdable at this mass",
            ReachCause::RouteFit => "Top level {flown} (set {configured}): fitted to the route",
        };
        standard.warnings.push(tr_fields(
            template,
            &[
                ("flown", flight_level(note.flown_m)),
                ("configured", flight_level(note.configured_m)),
            ],
        ));
    }
    let trip_top = trip_top_m(&standard.legs);
    let fitted = standard
        .legs
        .iter()
        .filter(|leg| leg.kind == LegKind::DiversionCruise && diversion_fitted(leg.end_m, trip_top))
        .map(|leg| leg.end_m)
        .fold(f64::INFINITY, f64::min);
    if fitted.is_finite() {
        standard.warnings.push(tr_fields(
            "Diversion {flown} (trip {trip}): fitted to the alternate",
            &[
                ("flown", flight_level(fitted)),
                ("trip", flight_level(trip_top)),
            ],
        ));
    }
    standard
}
