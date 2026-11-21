// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Engine sizing of a clean-sheet candidate: the thrust follows the closed
//! design instead of being a catalogue input.
//!
//! A clean-sheet brief owns thrust requirements (take-off field, engine-out
//! climb, approach and landing climb, cruise), not an engine. The engine the
//! user selects is the baseline cycle; this module scales it
//! ([`alas_config::EngineConfig::scaled_by`]: Raymer Eqs. 10.1-10.3,
//! Jenkinson Sec. 9.10, FLOPS Eq. 75 for the mass) to the smallest rating
//! that meets every thrust requirement of the closed airframe, and closes
//! the coupled problem, because the engine's mass, nacelle and fuel flow move
//! the take-off mass the requirements are written on.
//!
//! # Method
//!
//! 1. Close the candidate with the engine at the present factor `f` on the
//!    selected engine (`f = 1` first, or a search model's predicted factor,
//!    `SizingControls::initial_engine_factor`): the ordinary mass, trim and
//!    mission closure, main-gear placement included.
//! 2. On that closed airframe find the factor `r` on the installed rating at
//!    which the thrust rows of the performance family are exactly met
//!    ([`required_ratio`]): a bisection over the real residual table, with the
//!    airframe, mass and mission plan frozen. A row is met on the nominal
//!    value: the scatter bands that let a shortfall pass as a warning are an
//!    uncertainty on the model, not margin to design into. The rows are the
//!    take-off field, the engine-out climbs and the net take-off flight path
//!    (its gradient and its obstacle clearance); the cruise requirement is the
//!    design level at the mid-cruise mass. The planner's cruise-level and
//!    top-of-climb rows are *not* sizing rows: the mission plan lowers the
//!    level until they hold, so they are met by construction at any engine.
//! 3. Repeat from 1 with `f <- f * r (1 + tol/2)` (secant-accelerated) until
//!    the installed rating lies within [`AIM_BAND`] of the aim (a search
//!    model's [`SEARCH_AIM_BAND`]), the
//!    requirement raised by half the match tolerance. Every start then ends
//!    at the same engine to within that band, whatever the factor it began
//!    from; a stop anywhere inside the tolerance would leave the sized engine
//!    (and the take-off mass, balance and fuel that follow it) up to a
//!    percent apart between two analyses of one design. The coupling is weak (engine, nacelle and pylon are about
//!    8 percent of the take-off mass), so the iteration contracts by about a
//!    factor of ten per closure.
//!
//! Only [`alas_config::DesignMode::CleanSheet`] runs size; reference
//! adaptation and the baseline sandbox keep the declared engine, and take the
//! unchanged path (no row, no copy). An external aerodynamic polar describes
//! the nacelle it was computed for, so a run on one keeps the engine and
//! reports the requirement (`engine_thrust_margin`) and the oversize check
//! (`engine_oversize`) as diagnostics, exactly like a held engine.

use alas_config::{AlasConfig, SizedEngine, ENGINE_SCALE_SEARCH_BRACKET};

mod requirement;
mod rows;
use requirement::{required_ratio, Requirement};
pub(crate) use rows::residuals;

use crate::cancellation::EvaluationCancellation;
use crate::mdo::mission_model::SizingBudget;
use crate::mdo::residuals::gear_placement::size_with_main_gear_placement;
use crate::mdo::sizing::SizingOutcome;
use crate::mdo::types::{CandidateFailure, ExternalPolar, SizingControls, SizingWork};

/// How far above the requirement the installed rating may sit, as a fraction
/// of the requirement, for the engine to count as matched. It is also the
/// two-sided tolerance of the hard `engine_thrust_match` row. A 1 percent
/// engine moves the take-off mass by under 0.1 percent (the engine group is
/// about 8 percent of it, with exponent 1.15), below the model's own scatter.
pub(crate) const MATCH_TOLERANCE: f64 = 0.01;

/// Relative distance from the aim (the requirement raised by half of
/// [`MATCH_TOLERANCE`]) within which the iteration stops on every analysis
/// that reports: 0.005 percent. Measured on 21 A320-200 clean-sheet designs,
/// each analysed from the selected engine and from a start near
/// its requirement: stopping anywhere inside the tolerance left the two
/// engines 0.4 percent apart, 0.1 to 0.2 percent of block fuel and three
/// static-margin verdicts; a band of 0.025 percent still left 0.08 percent of
/// cost on five designs (the cost moves about 1.3 times the engine factor
/// here); this band leaves 0.002 percent on 20 of 21. The 21st has two
/// self-consistent engines 0.035 percent apart (its requirement steps with
/// the engine), and the start picks one: the delivery guard replays the
/// delivered design from the selected engine for that reason. Engineering
/// choice; it costs about 15 percent more closure work than 0.025 percent.
const AIM_BAND: f64 = 5.0e-5;

/// The wider band a search model stops in
/// (`SizingControls::engine_aim_band`): 0.25 percent, half the distance from
/// the aim to either edge of the tolerance, so its engines still meet the
/// hard match row in one closure from a good start. Its verdicts are
/// provisional: every finalist is replayed in [`AIM_BAND`].
pub(crate) const SEARCH_AIM_BAND: f64 = 2.5e-3;

/// Closures one sizing may take before it reports non-convergence.
const MAX_CLOSURES: usize = 6;

/// What the sizing settled on, carried by the outcome.
#[derive(Debug, Clone)]
pub(crate) struct EngineSizing {
    /// The engine as configured, before any scaling.
    pub base: SizedEngine,
    /// The engine the closed design carries.
    pub sized: SizedEngine,
    /// Whether the run sizes the engine (false: held, requirement reported).
    pub sizes: bool,
    /// Required rating over the installed one at the final closure (one is an
    /// engine exactly as large as the requirement); `None` when no thrust row
    /// could be evaluated.
    pub required_ratio: Option<f64>,
    /// The row that governs the requirement.
    pub governing: &'static str,
    /// Closures taken.
    pub closures: usize,
    /// Whether the iteration ended inside the match tolerance (or at the end
    /// of the search bracket, which the validity row then reports).
    pub converged: bool,
    /// A search model holds the engine its run sizes
    /// (`SizingControls::predicts_engine_scale`): the rows predict the
    /// sized engine's validity verdict.
    pub predicts_scale: bool,
}

/// Whether `config` sizes the engine of a candidate for this run.
pub(crate) fn applies(config: &AlasConfig, polar: Option<&ExternalPolar>) -> bool {
    config.optimizer.design_space.sizes_engine() && polar.is_none()
}

/// One closure of the iteration with the requirement found on it.
struct Closed {
    outcome: SizingOutcome,
    read: AlasConfig,
    requirement: Option<Requirement>,
}

/// Size candidate `x`: the engine to the thrust requirements of its closed
/// design (when the run sizes it) and the main gear by the placement rule.
///
/// Returns the closure the residuals must be built on and the configuration
/// that carries the sized engine, the placed gear and the geometry-scaled
/// CLmax; every other mode takes the unchanged
/// [`size_with_main_gear_placement`] path.
///
/// # Errors
///
/// The sizing failure of the first closure.
pub(in crate::mdo) fn size_candidate(
    config: &AlasConfig,
    x: &[f64],
    polar: Option<&ExternalPolar>,
    preserve_explicit_fuselage_length: bool,
    cancellation: Option<EvaluationCancellation>,
    controls: SizingControls,
) -> Result<(SizingOutcome, Option<AlasConfig>), CandidateFailure> {
    if config.optimizer.design_space.mode != alas_config::DesignMode::CleanSheet {
        return size_with_main_gear_placement(
            config,
            x,
            polar,
            preserve_explicit_fuselage_length,
            cancellation,
            controls,
        );
    }
    let base = SizedEngine::of(config);
    let sizes = applies(config, polar);
    let predicts = controls.predicts_engine_scale && !sizes;
    // A search model may start from a predicted factor (a warm start inside
    // the bracket); a held engine and every reported analysis start at one.
    let mut factor = controls
        .initial_engine_factor
        .filter(|factor| sizes && factor.is_finite() && *factor > 0.0)
        .map_or(1.0_f64, |factor| {
            factor.clamp(ENGINE_SCALE_SEARCH_BRACKET.0, ENGINE_SCALE_SEARCH_BRACKET.1)
        });
    let mut vector = x.to_vec();
    let mut preserve = preserve_explicit_fuselage_length;
    let mut controls_k = controls;
    // `(factor, required factor)` of every closure, on the selected engine.
    let mut trace: Vec<(f64, f64)> = Vec::new();
    let band = controls
        .engine_aim_band
        .filter(|band| band.is_finite() && *band > 0.0)
        .unwrap_or(AIM_BAND);
    let mut last: Option<Closed> = None;
    let mut closures = 0;
    let mut work = SizingWork::default();
    loop {
        closures += 1;
        controls_k.budget = controls.budget.map(|budget| remaining_budget(budget, work));
        let scaled = config
            .with_engine_scaled(factor)
            .map_err(|_| CandidateFailure::new("engine_scale"))?;
        let sized = size_with_main_gear_placement(
            &scaled,
            &vector,
            polar,
            preserve,
            cancellation.clone(),
            controls_k,
        );
        let (mut outcome, placed) = match sized {
            Ok(done) => done,
            // A later closure that cannot be sized ends the iteration on the
            // last good one, reported as not converged.
            Err(failure) => {
                // Cancellation and an exhausted budget are the caller's
                // verdict on the whole candidate, never a result to keep.
                let keep = !matches!(
                    failure.reason,
                    "cancelled" | crate::mdo::mission_model::SIZING_BUDGET_EXHAUSTED
                );
                return match last.take().filter(|_| keep) {
                    Some(closed) => Ok(finish(closed, secant_slope(&trace), &base, (closures - 1, work), (sizes, predicts), false)),
                    None => Err(failure),
                };
            }
        };
        work = summed(work, outcome.sized.work);
        let read = placed.unwrap_or(scaled);
        let requirement = required_ratio(&mut outcome, &read, band);
        let closed = Closed {
            outcome,
            read,
            requirement,
        };
        let Some(found) = requirement.filter(|_| sizes) else {
            return Ok(finish(closed, secant_slope(&trace), &base, (closures, work), (sizes, predicts), true));
        };
        // The aimed factor at this closure: the secant runs on `f -> aim(f)`,
        // whose fixed point is the aim. Raising the fixed point of
        // `f -> required(f)` by the margin afterwards misses the aim by the
        // margin times the slope of the requirement (measured: 0.05 percent
        // on the A320 clean sheet, where the cruise requirement falls 0.11 per
        // unit factor), which a band narrower than that can never meet.
        trace.push((factor, factor * found.ratio * (1.0 + 0.5 * MATCH_TOLERANCE)));
        // Installed over aimed rating: `1 / (r (1 + tol/2))`.
        let aimed = 1.0 / (found.ratio * (1.0 + 0.5 * MATCH_TOLERANCE));
        let matched = (aimed - 1.0).abs() <= band;
        if matched {
            return Ok(finish(closed, secant_slope(&trace), &base, (closures, work), (sizes, predicts), true));
        }
        if closures >= MAX_CLOSURES {
            return Ok(finish(closed, secant_slope(&trace), &base, (closures, work), (sizes, predicts), false));
        }
        let (low, high) = ENGINE_SCALE_SEARCH_BRACKET;
        let next = next_factor(&trace, controls.initial_engine_slope).clamp(low, high);
        // The engine is already at the end of the search bracket and the
        // requirement lies beyond it: nothing more to scale. The rows report
        // the mismatch and the validity window.
        if (next / factor - 1.0).abs() < 1.0e-9 {
            return Ok(finish(closed, secant_slope(&trace), &base, (closures, work), (sizes, predicts), true));
        }
        factor = next;
        // The next closure starts from this one's mass, with the cabin-sized
        // body kept (the engine does not move the cabin).
        if closed.outcome.sized.sizing_closed {
            controls_k.initial_takeoff_mass_kg = Some(closed.outcome.sized.takeoff_mass_kg);
        }
        vector = closed.outcome.history.dv.to_array().to_vec();
        preserve = true;
        last = Some(closed);
        #[cfg(test)]
        tests::after_closure(closures, work, cancellation.as_ref());
    }
}

/// The factor on the selected engine the fixed point of `f -> aim(f)` is
/// estimated at: the Newton step on the secant slope once two closures give
/// one, on the `prior` slope at the first closure, else the plain update;
/// a slope outside the contraction regime takes the plain update.
fn next_factor(trace: &[(f64, f64)], prior: Option<f64>) -> f64 {
    let &(factor, required) = trace.last().unwrap_or(&(1.0, 1.0));
    let slope = if trace.len() >= 2 {
        secant_slope(trace)
    } else {
        prior
    };
    match slope.filter(|slope| slope.is_finite() && slope.abs() < 0.9) {
        Some(slope) => factor + (required - factor) / (1.0 - slope),
        None => required,
    }
}

/// Slope of `f -> aim(f)` between the last two closures of `trace`; `None`
/// with fewer than two or two at the same factor.
fn secant_slope(trace: &[(f64, f64)]) -> Option<f64> {
    match trace {
        [.., (f0, g0), (f1, g1)] if (f1 - f0).abs() > 1.0e-9 => Some((g1 - g0) / (f1 - f0)),
        _ => None,
    }
}

/// The sizing work of every closure, which is all this candidate's work.
fn summed(left: SizingWork, right: SizingWork) -> SizingWork {
    SizingWork {
        plan_freezes: left.plan_freezes.saturating_add(right.plan_freezes),
        trip_flights: left.trip_flights.saturating_add(right.trip_flights),
        deck_evals: left.deck_evals.saturating_add(right.deck_evals),
    }
}

/// What remains of `budget` after `spent`, so every closure of one candidate
/// shares its one work cap.
fn remaining_budget(budget: SizingBudget, spent: SizingWork) -> SizingBudget {
    SizingBudget {
        max_trip_flights: budget
            .max_trip_flights
            .saturating_sub(u32::try_from(spent.trip_flights).unwrap_or(u32::MAX)),
        max_deck_evals: budget.max_deck_evals.saturating_sub(spent.deck_evals),
        ..budget
    }
}

fn finish(
    closed: Closed,
    slope: Option<f64>,
    base: &SizedEngine,
    (closures, work): (usize, SizingWork),
    (sizes, predicts_scale): (bool, bool),
    converged: bool,
) -> (SizingOutcome, Option<AlasConfig>) {
    let Closed {
        mut outcome,
        read,
        requirement,
        ..
    } = closed;
    outcome.sized.work = work;
    outcome.sized.engine_slope = slope.filter(|_| sizes);
    let governing = requirement.map_or("none", |r| r.governing);
    outcome.engine_sizing = Some(EngineSizing {
        base: base.clone(),
        sized: SizedEngine::of(&read),
        sizes,
        required_ratio: requirement.map(|r| r.ratio),
        governing,
        closures,
        converged,
        predicts_scale,
    });
    (outcome, Some(read))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_update_is_the_plain_required_factor() {
        assert_eq!(next_factor(&[(1.0, 0.9)], None), 0.9);
    }

    #[test]
    fn the_secant_step_lands_on_the_fixed_point_of_a_linear_coupling() {
        // required(f) = 0.5 + 0.1 f has the fixed point 0.5 / 0.9.
        let required = |f: f64| 0.5 + 0.1 * f;
        let first = required(1.0);
        let trace = [(1.0, first), (first, required(first))];
        assert!((next_factor(&trace, None) - 0.5 / 0.9).abs() < 1.0e-12);
    }

    #[test]
    fn a_slope_outside_the_contraction_regime_keeps_the_plain_update() {
        // A slope of 1.5 would extrapolate away from the fixed point.
        let trace = [(1.0, 2.0), (2.0, 3.5)];
        assert_eq!(next_factor(&trace, None), 3.5);
        // Identical factors give no slope at all.
        assert_eq!(next_factor(&[(1.0, 1.1), (1.0, 1.1)], None), 1.1);
    }

    #[test]
    fn the_budget_left_after_a_closure_is_the_work_not_yet_spent() {
        let budget = SizingBudget {
            max_trip_flights: 100,
            max_deck_evals: 1_000,
            max_outer_passes: 30,
        };
        let spent = SizingWork {
            plan_freezes: 1,
            trip_flights: 60,
            deck_evals: 1_500,
        };
        let left = remaining_budget(budget, spent);
        assert_eq!(left.max_trip_flights, 40);
        assert_eq!(left.max_deck_evals, 0);
        assert_eq!(left.max_outer_passes, 30);
        let twice = summed(spent, spent);
        assert_eq!((twice.trip_flights, twice.plan_freezes), (120, 2));
    }

    thread_local! {
        /// Test hook: request the candidate's cancellation once this many
        /// closures have completed (0 = never). Thread-local, so parallel
        /// tests do not see each other's setting.
        static CANCEL_AFTER_CLOSURE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
        /// Test hook: the candidate's cumulative (trip flights, deck
        /// evaluations) after each closure that is followed by another.
        static WORK_AFTER_CLOSURE: std::cell::RefCell<Vec<(u64, u64)>> =
            const { std::cell::RefCell::new(Vec::new()) };
    }

    pub(super) fn after_closure(
        done: usize,
        work: SizingWork,
        token: Option<&EvaluationCancellation>,
    ) {
        WORK_AFTER_CLOSURE.with(|w| {
            w.borrow_mut().push((work.trip_flights, work.deck_evals));
        });
        if CANCEL_AFTER_CLOSURE.with(std::cell::Cell::get) == done {
            if let Some(token) = token {
                token.request();
            }
        }
    }

    fn a320_clean_sheet() -> (AlasConfig, Vec<f64>) {
        let mut config =
            AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" })).unwrap();
        config.optimizer.design_space.mode = alas_config::DesignMode::CleanSheet;
        let preset = alas_config::presets::get("A320-200").unwrap();
        let design = crate::mdo::canonicalize_design(&config, preset.design_vector).unwrap();
        (config, design.to_array())
    }

    /// A request that lands after the first closure of an engine-sizing
    /// iteration (the second closure is flown with the token raised) is the
    /// caller's verdict on the whole candidate: the last good closure must
    /// not be returned as a result. Deterministic: the token is raised from
    /// inside the iteration, not by a clock. The dual-solver test holds the
    /// engine and therefore never reaches this branch (E6a MAJOR-2).
    #[test]
    fn a_cancellation_in_a_later_closure_is_never_answered_with_the_last_good_one() {
        let (config, x) = a320_clean_sheet();
        // Premise: this candidate needs at least two closures to size. The
        // registered A320 meets its requirement on the selected engine, so the
        // iteration starts 20 percent oversize (`initial_engine_factor`).
        let start = SizingControls {
            initial_engine_factor: Some(1.2),
            ..SizingControls::default()
        };
        CANCEL_AFTER_CLOSURE.with(|c| c.set(0));
        let (control, _) = size_candidate(
            &config,
            &x,
            None,
            false,
            Some(EvaluationCancellation::new()),
            start,
        )
        .unwrap_or_else(|f| panic!("{}", f.reason));
        let sizing = control.engine_sizing.expect("a clean-sheet run sizes");
        assert!(
            sizing.closures >= 2,
            "premise: {} closures",
            sizing.closures
        );

        CANCEL_AFTER_CLOSURE.with(|c| c.set(1));
        let result = size_candidate(
            &config,
            &x,
            None,
            false,
            Some(EvaluationCancellation::new()),
            start,
        );
        CANCEL_AFTER_CLOSURE.with(|c| c.set(0));
        match result {
            Err(failure) => assert_eq!(failure.reason, "cancelled"),
            Ok(_) => panic!("a cancelled candidate must not return its first closure"),
        }
    }

    /// Exhausted-budget variant of the same branch (E6c): the budget is the
    /// work closure 1 spent, so closure 2 starts with nothing left. The
    /// budget rejection is the candidate's verdict; the first closure must
    /// not be returned. Not yet run: the exact-exhaustion boundary (whether
    /// closure 1 itself trips at equality) is to be confirmed by a test run.
    #[test]
    fn an_exhausted_budget_in_a_later_closure_is_never_answered_with_the_first_closure() {
        let (config, x) = a320_clean_sheet();
        CANCEL_AFTER_CLOSURE.with(|c| c.set(0));
        WORK_AFTER_CLOSURE.with(|w| w.borrow_mut().clear());
        // Starts 20 percent oversize, as above, so a second closure follows.
        let start = SizingControls {
            initial_engine_factor: Some(1.2),
            ..SizingControls::default()
        };
        let (control, _) = size_candidate(
            &config,
            &x,
            None,
            false,
            Some(EvaluationCancellation::new()),
            start,
        )
        .unwrap_or_else(|f| panic!("{}", f.reason));
        let sizing = control.engine_sizing.expect("a clean-sheet run sizes");
        assert!(
            sizing.closures >= 2,
            "premise: {} closures",
            sizing.closures
        );
        let (trips, decks) = WORK_AFTER_CLOSURE.with(|w| w.borrow()[0]);
        let budget = SizingBudget {
            max_trip_flights: u32::try_from(trips).expect("trip flights fit u32"),
            max_deck_evals: decks,
            max_outer_passes: u32::MAX,
        };
        let controls = SizingControls {
            budget: Some(budget),
            ..start
        };
        let result = size_candidate(
            &config,
            &x,
            None,
            false,
            Some(EvaluationCancellation::new()),
            controls,
        );
        WORK_AFTER_CLOSURE.with(|w| w.borrow_mut().clear());
        match result {
            Err(failure) => assert_eq!(
                failure.reason,
                crate::mdo::mission_model::SIZING_BUDGET_EXHAUSTED
            ),
            Ok(_) => panic!("an exhausted budget must not return the first closure"),
        }
    }
}
