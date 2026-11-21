// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Closing the fuel of a plan: the fixed point of the fuel it loads and the fuel
//! its flight burns.

use alas_config::AlasConfig;
use alas_mission::MissionResult;

use super::events::{trigger_legs, Applied, ResolvedEvent};
use super::{
    burn_by_reserve, stage_of_tag, stage_start_mass, Flown, PlanClosure, PlanLoadCase, Run,
    TrimDragReport,
};

/// Iterations of the fuel closure before it is reported as not closed.
const MAX_CLOSURE_ITERATIONS: usize = 10;

/// Relative change of the loaded fuel below which the closure has settled.
const CLOSURE_TOLERANCE: f64 = 5.0e-4;

/// Growth of the fuel guess when a flight ran out of fuel.
const EXHAUSTION_GROWTH: f64 = 1.5;

/// Share of the zero-fuel mass the first guess of the loaded fuel is.
const FIRST_FUEL_GUESS: f64 = 0.25;

/// What the fuel of a plan is closed against.
pub(super) struct Budget {
    pub(super) stated: Option<f64>,
    pub(super) zero_fuel_mass_kg: f64,
    pub(super) taxi_total_kg: f64,
    pub(super) taxi_before_takeoff_kg: f64,
    pub(super) received_kg: f64,
}

/// One closure of the fuel: the last flight and what it says.
pub(super) struct Closed {
    pub(super) flown: Flown,
    /// The fuel that flight was loaded with, kg (not the next guess).
    pub(super) fuel_kg: f64,
    pub(super) all_burn_kg: f64,
    pub(super) contingency_kg: f64,
    pub(super) iterations: usize,
    pub(super) closure: PlanClosure,
}

/// Close the fuel of `run` by fixed-point iteration, from `previous` when a
/// closure of the same plan is at hand (its fuel and its masses start the
/// iteration), else from the first guess.
pub(super) fn close_fuel(
    run: &mut Run<'_>,
    budget: &Budget,
    previous: Option<&Closed>,
) -> Result<Closed, String> {
    let plan = run.plan;
    let stated = budget.stated;
    let mut fuel_kg = stated
        .or_else(|| previous.map(|closed| closed.fuel_kg))
        .unwrap_or(FIRST_FUEL_GUESS * budget.zero_fuel_mass_kg);
    let mut hint: Box<dyn Fn(usize) -> f64> = match previous {
        Some(closed) => {
            let result = closed.flown.result.clone();
            let fallback = closed.flown.takeoff_mass_kg;
            Box::new(move |stage| stage_start_mass(&result, stage, fallback))
        }
        None => {
            let mass = budget.zero_fuel_mass_kg + 0.6 * fuel_kg;
            Box::new(move |_| mass)
        }
    };
    let mut last: Option<Closed> = None;
    let mut previous_complete = false;
    // The burn is not a smooth function of the load (a heavier aircraft can end a
    // climb lower and fly a different ladder), so the iteration keeps a bracket:
    // the largest load that ran dry and the smallest complete flight its load
    // covers. A guess outside the bracket is replaced by its midpoint, and the
    // smallest covering flight is returned when the iterations run out.
    let mut dry_kg = 0.0_f64;
    let mut covering: Option<Closed> = None;
    for iteration in 1..=MAX_CLOSURE_ITERATIONS {
        let flown = run.fly(fuel_kg, &*hint)?;
        let complete = flown.result.completed_summary().is_some();
        let (trip_burn, all_burn) = burn_by_reserve(&flown.result, plan);
        let contingency_kg = plan.contingency_fraction * trip_burn;
        // What the flight needs at the end (burn, taxi, what is given away, less
        // what is received, and the contingency), and at its worst instant: fuel
        // received late does not help the aircraft that runs dry before it.
        let needed_kg = (all_burn + budget.taxi_total_kg - budget.received_kg + contingency_kg)
            .max(peak_use_kg(&flown.result, &run.events));
        let result_summary = (complete, flown.result.fuel_exhaustion.is_some());
        let closed = |flown: Flown, closure: PlanClosure| Closed {
            flown,
            fuel_kg,
            all_burn_kg: all_burn,
            contingency_kg,
            iterations: iteration,
            closure,
        };
        let covers = complete && fuel_kg >= needed_kg;
        let covered_kg = covering.as_ref().map(|closed| closed.fuel_kg);
        let next = match (stated, result_summary) {
            (Some(_), _) => {
                last = Some(closed(flown, PlanClosure::Stated));
                break;
            }
            (None, (true, _)) => {
                let guess = needed_kg * (1.0 + CLOSURE_TOLERANCE) + 1.0;
                if covers && guess <= dry_kg {
                    0.5 * (dry_kg + fuel_kg)
                } else {
                    guess
                }
            }
            (None, (false, true)) => {
                dry_kg = dry_kg.max(fuel_kg);
                match covered_kg {
                    Some(covered_kg) => 0.5 * (fuel_kg + covered_kg),
                    None => {
                        fuel_kg
                            * if previous_complete {
                                1.0 + 20.0 * CLOSURE_TOLERANCE
                            } else {
                                EXHAUSTION_GROWTH
                            }
                    }
                }
            }
            (None, (false, false)) => {
                last = Some(closed(flown, PlanClosure::NotClosed));
                break;
            }
        };
        let band_kg = 3.0 * CLOSURE_TOLERANCE * fuel_kg + 1.0;
        // Settled: the load covers the flight to the band, or it is the smallest
        // covering load within the band of one that ran dry.
        let settled = covers && (fuel_kg - needed_kg <= band_kg || fuel_kg - dry_kg <= band_kg);
        previous_complete = complete;
        let result_for_hint = flown.result.clone();
        let fallback = flown.takeoff_mass_kg;
        let this = closed(
            flown,
            if settled {
                PlanClosure::Closed
            } else {
                PlanClosure::NotClosed
            },
        );
        if settled {
            last = Some(this);
            break;
        }
        if covers && covered_kg.is_none_or(|covered_kg| fuel_kg < covered_kg) {
            covering = Some(this);
        } else {
            last = Some(this);
        }
        fuel_kg = next;
        hint = Box::new(move |stage| stage_start_mass(&result_for_hint, stage, fallback));
    }
    // Iterations exhausted: the smallest load whose flight completed and covered
    // its needs is reported over a last flight that did not settle.
    match (last, covering) {
        (Some(last), _) if last.closure != PlanClosure::NotClosed => Ok(last),
        (_, Some(covering)) => Ok(covering),
        (Some(last), None) => Ok(last),
        (None, None) => Err("the plan could not be flown".to_owned()),
    }
}

/// The load case of a closure.
pub(super) fn build_case(
    run: &Run<'_>,
    budget: &Budget,
    closed: &Closed,
    capacity_kg: Option<f64>,
    config: &AlasConfig,
    trim_drag: TrimDragReport,
) -> PlanLoadCase {
    // The tanks hold the ramp fuel; the take-off mass limit applies at brake
    // release, after the taxi fuel is burned.
    let loadable_kg = capacity_kg.map_or(f64::INFINITY, |capacity| capacity);
    let mtow_fuel_kg = config.requirements.mtow_kg - budget.zero_fuel_mass_kg;
    let shortfall_kg = (closed.fuel_kg - loadable_kg)
        .max(closed.fuel_kg - budget.taxi_before_takeoff_kg - mtow_fuel_kg)
        .max(0.0);
    PlanLoadCase {
        plan: run.plan.clone(),
        ramp_fuel_kg: closed.fuel_kg,
        takeoff_mass_kg: closed.flown.takeoff_mass_kg,
        flight_burn_kg: closed.all_burn_kg,
        taxi_fuel_kg: budget.taxi_total_kg,
        taxi_before_takeoff_kg: budget.taxi_before_takeoff_kg,
        net_offload_kg: -budget.received_kg,
        contingency_kg: closed.contingency_kg,
        closure: closed.closure,
        iterations: closed.iterations,
        shortfall_kg,
        events: run.events.clone(),
        declared_items: run.items.clone(),
        stage_of_segment: closed
            .flown
            .result
            .segments
            .iter()
            .map(|segment| stage_of_tag(&segment.spec.tag))
            .collect(),
        trim_drag,
    }
}

/// The most fuel the flight has used up at any instant, kg: the taxi, the burn
/// and what is offloaded count as used, fuel received as returned. The ramp
/// fuel has to cover it whatever is received later.
fn peak_use_kg(result: &MissionResult, events: &[ResolvedEvent]) -> f64 {
    let specs: Vec<_> = result
        .segments
        .iter()
        .map(|segment| segment.spec.clone())
        .collect();
    let triggers = trigger_legs(&specs, events);
    let (mut used, mut peak) = (0.0_f64, 0.0_f64);
    let apply = |leg: usize, used: &mut f64, peak: &mut f64| {
        for (event, trigger) in events.iter().zip(&triggers) {
            if *trigger == leg {
                *used -= event.change().fuel_kg;
                *peak = peak.max(*used);
            }
        }
    };
    for (leg, segment) in result.segments.iter().enumerate() {
        apply(leg, &mut used, &mut peak);
        let mass = &segment.conditions.total_mass_kg;
        if let (Some(first), Some(last)) = (mass.first(), mass.last()) {
            used += first - last;
            peak = peak.max(used);
        }
    }
    for leg in result.segments.len()..=triggers.iter().copied().max().unwrap_or(0) {
        apply(leg, &mut used, &mut peak);
    }
    peak
}

/// Refuse a plan that takes on in flight more fuel than the tanks have room for
/// at that moment: the flown mass would carry fuel that has nowhere to go.
/// `capacity_kg` is the usable tank capacity, when it is known.
pub(super) fn check_refuel_room(
    run: &Run<'_>,
    closed: &Closed,
    capacity_kg: Option<f64>,
) -> Result<(), String> {
    let Some(capacity_kg) = capacity_kg else {
        return Ok(());
    };
    let result = &closed.flown.result;
    // A flight that stopped early is judged as far as it flew.
    let complete = result.completed_summary().is_some();
    let specs: Vec<_> = result
        .segments
        .iter()
        .map(|segment| segment.spec.clone())
        .collect();
    let triggers = trigger_legs(&specs, &run.events);
    let mut on_board_kg = closed.fuel_kg;
    for leg in 0..=result.segments.len() {
        if leg == result.segments.len() && !complete {
            break;
        }
        for (event, trigger) in run.events.iter().zip(&triggers) {
            if *trigger != leg {
                continue;
            }
            if let Applied::Refuel { mass_kg } = event.applied {
                if mass_kg > capacity_kg - on_board_kg + 1.0 {
                    return Err(format!(
                        "stage '{}' takes on {mass_kg:.0} kg of fuel in flight with {on_board_kg:.0} kg of the {capacity_kg:.0} kg the tanks hold on board: there is room for {:.0} kg",
                        run.plan.stages[event.stage].id,
                        (capacity_kg - on_board_kg).max(0.0)
                    ));
                }
            }
            on_board_kg += event.change().fuel_kg;
        }
        if let Some(segment) = result.segments.get(leg) {
            let mass = &segment.conditions.total_mass_kg;
            if let (Some(first), Some(last)) = (mass.first(), mass.last()) {
                on_board_kg -= first - last;
            }
        }
    }
    Ok(())
}
