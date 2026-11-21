// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The plan engine: a user-defined mission flown stage by stage.
//!
//! A [`MissionPlan`] with stages replaces the standard mission's fixed
//! sequence. Each stage becomes legs of the same segment network
//! ([`legs`]), flown with the same guidance as the standard mission
//! ([`super::flight::fly_guided`]) with two additions: mass changes between
//! legs (a drop, a store, a refuelling, taxi fuel; [`events`]) and the
//! closure of distances after a revision ([`closure`]).
//!
//! The fuel loaded at brake release is closed to the plan: the fuel the stages
//! burn, the fuel given away less the fuel received, and the plan's
//! contingency on the fuel of the stages that are not reserves. That is a
//! fixed point (the burn depends on the mass the fuel makes), found by
//! iteration. A plan that states its fuel is flown as stated.

mod closure;
pub mod events;
mod fuel_closure;
mod legs;
pub(in crate::mission_stage) mod loiter;
mod trim_drag;

use std::sync::Arc;

use alas_config::airports::Airport;
use alas_config::{ActiveEngineModel, AlasConfig, MissionPlan};
use alas_mass::fuel_plan::FuelBurnModel;
use alas_mission::segments::SegmentSpec;
use alas_mission::{build_mission_request, MissionResult};

pub use events::{Applied, CarriedItem, ResolvedEvent};
pub use trim_drag::{TrimDragReport, TrimDragStatus};

use super::flight::descent_pieces::subdivide_descents;
use super::flight::{fly_guided, Hooks};
use super::schedule::speed_change::{set_junction_configuration, CruiseSpeedRule, SpeedMemory};
use super::{build_analyses, LoadCaseSelection, SelectedLoadCase};
use crate::feasibility::plan_fuel_loading;
use crate::full_analysis::AnalysisReport;
use closure::Closer;
use fuel_closure::{build_case, check_refuel_room, close_fuel, Budget};
pub(crate) use legs::stage_of_tag;
use legs::{advances_route, entry_on_route, Context};

pub(super) const METRES_PER_FOOT: f64 = 0.3048;

/// The excess of the ground distance flown over each distance the plan states, m, keyed by
/// closure group (`None`: the route; `Some(stage)`: that stage's own distance).
type ClosureExcesses = Vec<(Option<usize>, f64)>;

/// How the fuel closure ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanClosure {
    /// The loaded fuel equals what the plan needs, to the tolerance.
    Closed,
    /// The plan states its fuel; it was flown as stated.
    Stated,
    /// The iteration did not settle, or a flight could not be completed.
    NotClosed,
}

/// The load case a plan is flown at, and what is needed to read the flight back.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanLoadCase {
    /// The plan flown.
    pub plan: MissionPlan,
    /// Fuel loaded at the ramp, kg: what the flight burns, the taxi fuel, the
    /// net offload and the contingency. The fuel at brake release is
    /// [`PlanLoadCase::brake_release_fuel_kg`].
    pub ramp_fuel_kg: f64,
    /// Mass at brake release (after the taxi fuel before the first take-off), kg.
    pub takeoff_mass_kg: f64,
    /// Fuel the plan burns in flight, kg.
    pub flight_burn_kg: f64,
    /// Taxi fuel of the whole plan, kg.
    pub taxi_fuel_kg: f64,
    /// The part of it burned before the first take-off, kg.
    pub taxi_before_takeoff_kg: f64,
    /// Fuel given away in flight less fuel received, kg.
    pub net_offload_kg: f64,
    /// Fuel left at the end by the contingency, kg.
    pub contingency_kg: f64,
    /// How the closure ended.
    pub closure: PlanClosure,
    /// Flights flown by the closure.
    pub iterations: usize,
    /// Fuel the plan needs beyond the tanks or the take-off mass limit, kg.
    pub shortfall_kg: f64,
    /// The events, in flight order.
    pub events: Vec<ResolvedEvent>,
    /// The items carried from brake release.
    pub declared_items: Vec<CarriedItem>,
    /// The tag of every flown segment's stage, in flight order.
    pub stage_of_segment: Vec<Option<usize>>,
    /// The trim drag the plan pays for where its centre of gravity sits.
    pub trim_drag: TrimDragReport,
}

impl PlanLoadCase {
    /// Fuel left at the end of the plan, kg.
    #[must_use]
    pub fn final_fuel_kg(&self) -> f64 {
        self.ramp_fuel_kg - self.flight_burn_kg - self.taxi_fuel_kg - self.net_offload_kg
    }

    /// Fuel on board at the first brake release, kg: the ramp fuel less the taxi
    /// fuel burned before it.
    #[must_use]
    pub fn brake_release_fuel_kg(&self) -> f64 {
        self.ramp_fuel_kg - self.taxi_before_takeoff_kg
    }
}

struct Flown {
    result: MissionResult,
    takeoff_mass_kg: f64,
    /// Path changes flown level for want of a constant descent rate.
    flown_level: Vec<String>,
}

/// Everything one flight of the plan reads.
struct Run<'a> {
    config: &'a AlasConfig,
    plan: &'a MissionPlan,
    request: alas_mission::MissionRequest,
    zero_fuel_mass_kg: f64,
    analyses: alas_mission::segments::MissionAnalyses,
    events: Vec<ResolvedEvent>,
    items: Vec<CarriedItem>,
    cruise_rule: CruiseSpeedRule,
    turboprop: bool,
    /// Increment of the trimmed induced drag to add to each leg family, drag
    /// coefficient on the reference area (empty until a flight has shown where
    /// the centre of gravity goes).
    trim_delta_cd: std::collections::HashMap<String, f64>,
    /// Per closure group (`None`: the route; `Some(stage)`: that stage's own distance), the
    /// amount the closing target has been moved by so that the flight covers the stated
    /// distance, m. Kept between flights: the next one starts from it.
    closing_shift_m: std::collections::HashMap<Option<usize>, f64>,
}

/// Closing passes of a plan flight's distances against the ground distance flown.
const MAX_CLOSING_PASSES: usize = 6;

/// Tolerance between a distance the plan states (the route, a diversion) and the ground
/// distance flown over it, m: a threshold chosen up front (about 2e-5 of a 500 km route, a
/// few grams of fuel), not a published value. Looser than the standard mission's 1 m, whose
/// route is closed by a plain re-targeting, because the altitude fit moves with the target here.
const CLOSING_TOLERANCE_M: f64 = 10.0;

/// The shifts of one closure group's target tried in a closing, and the excess each left.
#[derive(Default)]
struct ClosingHistory {
    /// The last shift and its excess.
    last: Option<(f64, f64)>,
    /// The shift tried with the largest negative excess, and the one with the smallest positive
    /// one (index 0 and 1), as shift and excess. An excess kept while the other end stays
    /// the same is halved (Illinois), so a stale end cannot hold the step back.
    bracket: [Option<(f64, f64)>; 2],
    /// Which end (0 negative, 1 positive) the last excess replaced.
    last_side: Option<usize>,
}

impl ClosingHistory {
    /// The shift to try after `shift` left `excess` (the excess is the flown distance less the
    /// stated one, so a positive excess asks for a smaller shift). Before the root is enclosed:
    /// the secant through the last two shifts, whose slope is taken from 0.2 to 5 and else 1.
    /// Once it is enclosed: the Illinois step, which stays between the two ends.
    fn next_shift(&mut self, shift: f64, excess: f64) -> f64 {
        let side = usize::from(excess > 0.0);
        if self.last_side == Some(side) {
            if let Some((_, kept)) = self.bracket[1 - side].as_mut() {
                *kept *= 0.5;
            }
        }
        self.last_side = Some(side);
        self.bracket[side] = Some((shift, excess));
        let previous = self.last.replace((shift, excess));
        if let [Some((low_shift, low_excess)), Some((high_shift, high_excess))] = self.bracket {
            let span = high_excess - low_excess;
            if span > 0.0 && span.is_finite() {
                return low_shift - low_excess * (high_shift - low_shift) / span;
            }
        }
        let slope = previous
            .filter(|(old_shift, _)| (shift - old_shift).abs() > 1.0)
            .map(|(old_shift, old_excess)| (excess - old_excess) / (shift - old_shift))
            .filter(|slope| slope.is_finite() && (0.2..=5.0).contains(slope))
            .unwrap_or(1.0);
        shift - excess / slope
    }
}

impl Run<'_> {
    /// Fly the plan with `fuel_kg` loaded; `start_mass` gives the mass a stage is expected to
    /// start at (it sets the speed of a hold).
    ///
    /// The closure sizes each cruise that closes a distance against the footprint it computes
    /// for the climbs and descents, which the flight does not fly exactly (a climb at the
    /// maximum-climb rating has the rate the force balance gives, not the one it was sized
    /// with). As the standard mission does ([`super::flight::fly_with_guidance`]), the closing
    /// target of each distance is moved by the excess the flight covered over the stated
    /// distance, until the flight covers it.
    fn fly(&mut self, fuel_kg: f64, start_mass: &dyn Fn(usize) -> f64) -> Result<Flown, String> {
        // Per group, the shifts tried and the excess each left: the secant through the last two
        // is the slope of the flown distance against the target (above 1 where fitting the
        // altitudes to the target moves the climbs and descents with it), and once two shifts
        // enclose the root the update is a regula falsi step that cannot leave them.
        let mut history: std::collections::HashMap<Option<usize>, ClosingHistory> =
            std::collections::HashMap::new();
        for pass in 1..=MAX_CLOSING_PASSES {
            let (flown, excess_m) = self.fly_pass(fuel_kg, start_mass)?;
            if flown.result.completed_summary().is_none()
                || excess_m
                    .iter()
                    .all(|(_, excess)| excess.abs() <= CLOSING_TOLERANCE_M)
            {
                return Ok(flown);
            }
            if pass == MAX_CLOSING_PASSES {
                let (group, worst) = excess_m
                    .iter()
                    .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
                    .map_or((None, 0.0), |(group, excess)| (*group, *excess));
                let distance = group.map_or_else(
                    || "the route".to_owned(),
                    |stage| format!("the distance stage {stage} states"),
                );
                return Err(format!(
                    "the native flight of the plan covers {worst:.1} m more than {distance} \
                     after {pass} closing passes (tolerance {CLOSING_TOLERANCE_M} m)"
                ));
            }
            for (group, excess) in excess_m {
                let shift = self.closing_shift_m.entry(group).or_insert(0.0);
                *shift = history.entry(group).or_default().next_shift(*shift, excess);
            }
        }
        Err("the plan closing loop has no passes".to_owned())
    }

    /// One flight of the plan at the closing targets now held, and the excess of the ground
    /// distance it covered over each distance the plan states, m, by closure group.
    fn fly_pass(
        &mut self,
        fuel_kg: f64,
        start_mass: &dyn Fn(usize) -> f64,
    ) -> Result<(Flown, ClosureExcesses), String> {
        let config = self.config;
        let mut built = {
            let request = &self.request;
            let loiter = |altitude_m: f64, mass_kg: f64| {
                loiter::endurance_speed_m_s(
                    &self.analyses,
                    altitude_m,
                    mass_kg,
                    request.departure_isa_deviation_c,
                    self.turboprop,
                    config.performance.cl_max_clean,
                )
            };
            let ratio = super::schedule::speed::standard_day_speed_ratio(
                request.cruise_altitude_m,
                request.departure_isa_deviation_c,
            )?;
            let context = Context {
                profile: &config.mission.profile,
                departure_elevation_m: request.departure_elevation_m,
                arrival_elevation_m: request.arrival_elevation_m,
                isa_deviation_k: request.departure_isa_deviation_c,
                route_distance_m: request.route_distance_m,
                cruise_speed_ratio: ratio,
                loiter_speed_m_s: &loiter,
                mass_hint_kg: start_mass,
                cruise_rule: Some(self.cruise_rule.clone()),
            };
            legs::build(self.plan, &context)?
        };
        let area_to_cd = 1.0 / self.analyses.reference_area_m2;
        let carried = events::carried_drag_area_m2(&built.specs, &self.events, &self.items);
        self.assign_configuration_drag(&mut built.specs, &carried, area_to_cd);
        let stated_groups = built.groups.clone();
        let mut closing_groups = built.groups.clone();
        for group in &mut closing_groups {
            group.total_m += self
                .closing_shift_m
                .get(&group.stage)
                .copied()
                .unwrap_or(0.0);
        }
        let closer = Closer {
            time_s: built.time_s.clone(),
            groups: closing_groups,
            on_route: self
                .plan
                .stages
                .iter()
                .map(|stage| advances_route(&stage.kind) && !stage.reserve)
                .collect(),
            entry_on_route: self
                .plan
                .stages
                .iter()
                .map(|stage| entry_on_route(&stage.kind) && !stage.reserve)
                .collect(),
            start_elevation_m: built.start_elevation_m,
            stage_ids: self.plan.stages.iter().map(|s| s.id.clone()).collect(),
            shortfall: std::cell::RefCell::new(None),
        };
        closer.close(&mut built.specs, &built.memory)?;
        let start = events::changes(&built.specs, &self.events)
            .first()
            .copied()
            .unwrap_or_default();
        let takeoff_mass_kg = self.zero_fuel_mass_kg + fuel_kg + start.dry_kg + start.fuel_kg;
        self.analyses.takeoff_mass_kg = takeoff_mass_kg;
        self.analyses.minimum_mass_kg = Some(self.zero_fuel_mass_kg + start.dry_kg);
        let mut reclose =
            |schedule: &mut Vec<SegmentSpec>, memory: &SpeedMemory| closer.close(schedule, memory);
        let events = &self.events;
        let mut flown_level = Vec::new();
        let hooks = Hooks {
            reclose: &mut reclose,
            changes: &|schedule| events::changes(schedule, events),
            rate_passes: true,
            adapt_idle_limited: true,
            flown_level: &mut flown_level,
        };
        let mut schedule = built.specs;
        // The descents are cut into pieces as the standard mission's are (`fly_with_guidance`),
        // after the closure above: a cut keeps the tag stage and the distance of each leg, and
        // the mass events and closure groups of the plan are read from the schedule as it stands.
        subdivide_descents(&mut schedule, built.start_elevation_m);
        let result = fly_guided(&mut schedule, &self.analyses, built.memory, hooks)?;
        if let Some(message) = closer.take_shortfall() {
            return Err(message);
        }
        let excess_m = stated_groups
            .iter()
            .map(|group| {
                (
                    group.stage,
                    closer.flown_distance_m(&result, group.stage) - group.total_m,
                )
            })
            .collect();
        Ok((
            Flown {
                result,
                takeoff_mass_kg,
                flown_level,
            },
            excess_m,
        ))
    }

    /// Flap and gear drag of the legs flown in a high-lift configuration, the
    /// added drag of the stage and of what is carried, as drag coefficients.
    fn assign_configuration_drag(
        &self,
        specs: &mut [SegmentSpec],
        carried_area_m2: &[f64],
        area_to_cd: f64,
    ) {
        let performance = &self.config.performance;
        let climb_speed = self.config.mission.profile.initial_climb_air_speed_m_s;
        for (spec, carried) in specs.iter_mut().zip(carried_area_m2) {
            let name = spec.tag.split('@').next().unwrap_or_default();
            let configuration = match name {
                "takeoff" if spec.air_speed_m_s < 0.95 * climb_speed => {
                    performance.oei_climb_delta_cd
                }
                "go_around" => performance.oei_climb_delta_cd,
                n if n.starts_with("approach") || n.starts_with("final_landing") => {
                    performance.landing_flap_delta_cd + performance.landing_gear_delta_cd
                }
                _ => 0.0,
            };
            let stage_area = stage_of_tag(&spec.tag)
                .and_then(|stage| self.plan.stages.get(stage))
                .map_or(0.0, |stage| stage.extra_drag_area_m2);
            let trim = self
                .trim_delta_cd
                .get(trim_drag::leg_key(&spec.tag))
                .copied()
                .unwrap_or(0.0);
            spec.configuration_delta_cd =
                configuration + (stage_area + carried) * area_to_cd + trim;
        }
        set_junction_configuration(specs);
    }
}

/// Fuel burned by the segments of `result` that belong to non-reserve stages,
/// and by all of them, kg.
fn burn_by_reserve(result: &MissionResult, plan: &MissionPlan) -> (f64, f64) {
    let (mut trip, mut all) = (0.0, 0.0);
    for segment in &result.segments {
        let mass = &segment.conditions.total_mass_kg;
        let (Some(first), Some(last)) = (mass.first(), mass.last()) else {
            continue;
        };
        let burn = first - last;
        all += burn;
        let reserve = stage_of_tag(&segment.spec.tag)
            .and_then(|stage| plan.stages.get(stage))
            .is_some_and(|stage| stage.reserve);
        if !reserve {
            trip += burn;
        }
    }
    (trip, all)
}

/// The mass at the start of each stage's first leg of a flown `result`, kg.
fn stage_start_mass(result: &MissionResult, stage: usize, fallback_kg: f64) -> f64 {
    result
        .segments
        .iter()
        .find(|segment| stage_of_tag(&segment.spec.tag).is_some_and(|s| s >= stage))
        .and_then(|segment| segment.conditions.total_mass_kg.first().copied())
        .unwrap_or(fallback_kg)
}

/// Passes of the trim-drag correction (each flies the plan once more).
const MAX_TRIM_PASSES: usize = 3;

/// Change of a leg's trim-drag increment, drag coefficient, below which the
/// correction has settled: a fifth of a drag count.
const TRIM_DRAG_SETTLED_CD: f64 = 2.0e-5;

/// Fly `config.mission.plan` and close the fuel to it.
///
/// # Errors
///
/// A message when the plan is invalid, the aircraft cannot be set up for the
/// mission, or a stage cannot be built (for example a climb that does not go
/// up). A flight that cannot be completed is not an error: its partial result
/// is returned with a load case that says the plan was not closed.
pub(super) fn evaluate(
    config: &AlasConfig,
    report: &AnalysisReport,
    origin: &Airport,
    destination: &Airport,
    route_distance_m: f64,
) -> Result<(Option<MissionResult>, SelectedLoadCase), String> {
    let plan = &config.mission.plan;
    plan.validate()
        .map_err(|error| format!("mission plan: {error}"))?;
    plan.validate_items(&config.cabin.payload_items)
        .map_err(|error| format!("mission plan: {error}"))?;
    let request = build_mission_request(config, origin, destination, route_distance_m);
    let fuel_loading = plan_fuel_loading(config, &report.design, report);
    let zero_fuel_mass_kg = fuel_loading.zero_fuel_mass_kg;
    let capacity_kg = fuel_loading.usable_capacity.capacity_kg;
    let analyses = build_analyses(config, report)?;
    let model = Arc::new(crate::fuel_model::report_mission_model(config, report)?);
    let taxi_flow = model
        .taxi_fuel_flow_kg_s()
        .map_err(|error| format!("taxi fuel flow: {error}"))?;
    let rule_model = Arc::clone(&model);
    let cruise_rule = CruiseSpeedRule::new(move |level_m, declared_m_s| {
        rule_model.cruise_speed_at_level(level_m, declared_m_s).ok()
    });
    let items = events::declared_items(report.payload_layout.as_ref());
    let resolved = events::resolve(plan, &items, taxi_flow)?;
    let mut run = Run {
        config,
        plan,
        request,
        zero_fuel_mass_kg,
        analyses,
        events: resolved,
        items,
        cruise_rule,
        turboprop: matches!(
            config.geometry.engine.active_model(),
            Ok(ActiveEngineModel::Turboprop(_))
        ),
        trim_delta_cd: std::collections::HashMap::new(),
        closing_shift_m: std::collections::HashMap::new(),
    };
    let taxi_total_kg: f64 = run
        .events
        .iter()
        .map(|event| match event.applied {
            Applied::Taxi { fuel_kg } => fuel_kg,
            _ => 0.0,
        })
        .sum();
    let first_takeoff = plan
        .stages
        .iter()
        .position(|stage| matches!(stage.kind, alas_config::StageKind::Takeoff { .. }))
        .unwrap_or(0);
    let taxi_before_takeoff_kg: f64 = run
        .events
        .iter()
        .filter(|event| event.stage < first_takeoff)
        .map(|event| match event.applied {
            Applied::Taxi { fuel_kg } => fuel_kg,
            _ => 0.0,
        })
        .sum();
    let received_kg: f64 = run
        .events
        .iter()
        .map(|event| match event.applied {
            Applied::Refuel { mass_kg } => mass_kg,
            _ => 0.0,
        })
        .sum();

    let budget = Budget {
        stated: plan.takeoff_fuel_kg,
        zero_fuel_mass_kg,
        taxi_total_kg,
        taxi_before_takeoff_kg,
        received_kg,
    };
    let mut closed = close_fuel(&mut run, &budget, None)?;
    // The drag was trimmed at one centre of gravity; the flight moves it. The
    // first flight shows where it goes, the legs then pay the trim drag of
    // their own centre of gravity and the fuel is closed again.
    let mut tables: Option<alas_opt::mdo::cg_trim_drag::CgTrimDrag> = None;
    let mut trim_report = TrimDragReport::default();
    let mut trim_settled = false;
    let mut trim_passes = 0;
    for _ in 0..MAX_TRIM_PASSES {
        if closed.flown.result.completed_summary().is_none() {
            break;
        }
        let probe = build_case(
            &run,
            &budget,
            &closed,
            capacity_kg,
            config,
            TrimDragReport::default(),
        );
        let Some(leg_cg) = crate::feasibility::plan_leg_cg(
            config,
            &report.design,
            report,
            &closed.flown.result,
            &probe,
        ) else {
            break;
        };
        let correction =
            trim_drag::correction(config, report, &mut tables, &closed.flown.result, &leg_cg);
        trim_report = correction.report;
        trim_passes += 1;
        if trim_report.status != TrimDragStatus::Applied {
            // A pass that finds the plan within the reference station or cannot
            // trim it says nothing about the increments the flight before it
            // carried: fly again without them so the report describes the flight.
            if !run.trim_delta_cd.is_empty() {
                run.trim_delta_cd.clear();
                closed = close_fuel(&mut run, &budget, Some(&closed))?;
            }
            break;
        }
        // The increments are keyed by the leg as the plan built it (the guidance
        // pieces of a leg share its key), so every key of a flight is one that
        // the flight before assigned a value to, or none (the first pass: the
        // leg flew with no increment, which is what it is compared with).
        let settled = correction.by_family.iter().all(|(family, increment)| {
            let before = run.trim_delta_cd.get(family).copied().unwrap_or(0.0);
            (increment - before).abs() < TRIM_DRAG_SETTLED_CD
        });
        let flown_increments = std::mem::replace(&mut run.trim_delta_cd, correction.by_family);
        if settled {
            trim_settled = true;
            trim_report.increments = flown_increments;
            break;
        }
        closed = close_fuel(&mut run, &budget, Some(&closed))?;
    }
    trim_report.passes = trim_passes;
    if !trim_settled {
        trim_report.increments = run.trim_delta_cd.clone();
    }
    trim_report.unsettled = trim_report.status == TrimDragStatus::Applied && !trim_settled;
    check_refuel_room(&run, &closed, capacity_kg)?;
    let case = build_case(&run, &budget, &closed, capacity_kg, config, trim_report);
    let takeoff_mass_kg = closed.flown.takeoff_mass_kg;
    let mut flown = closed.flown;
    let native_error = flown
        .result
        .completion_refusal()
        .map(|reason| format!("the plan was not flown to its end: {reason}"));
    let propulsion_validity =
        super::deck_validity::PropulsionDeckValidity::of_flight(&mut flown.result, &run.analyses);
    Ok((
        Some(flown.result),
        SelectedLoadCase {
            takeoff_mass_kg,
            zero_fuel_mass_kg,
            selection: LoadCaseSelection::Plan(Box::new(case)),
            native_error,
            native_supplement: None,
            flown_level_path_changes: flown.flown_level,
            propulsion_validity,
        },
    ))
}

/// Add the drag of the declared items the aircraft carries (a drone, a pod, a
/// pallet) to every leg of the standard mission's schedule, as a drag
/// coefficient on the reference area. Nothing declared leaves the schedule as it
/// was, bit for bit.
pub(super) fn add_carried_drag(
    schedule: &mut [SegmentSpec],
    report: &AnalysisReport,
    reference_area_m2: f64,
) {
    let area_m2: f64 = events::declared_items(report.payload_layout.as_ref())
        .iter()
        .map(|item| item.drag_area_m2)
        .sum();
    if area_m2 > 0.0 {
        for spec in schedule {
            spec.configuration_delta_cd += area_m2 / reference_area_m2;
        }
    }
}
