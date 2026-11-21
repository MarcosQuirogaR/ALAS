// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The exploration of a staged search: the kernel on the cheap screening
//! model, in rounds, with its constraint margins calibrated on the
//! refinement model between rounds.
//!
//! Each of the [`ROUNDS`] rounds (a clean sheet's [`CLEAN_SHEET_ROUNDS`])
//! runs the guided kernel
//! (`lshade_de::run_guided`, `guide`) for an equal share of the stage's
//! remaining candidates, from the baseline and the best designs found so
//! far. Before the first round and between rounds the best
//! [`calibration::CALIBRATION_POINTS`] designs not yet compared are analysed
//! with the refinement model, the margin offsets between the two models are
//! measured on the most recent [`CALIBRATION_WINDOW`] such pairs
//! (`calibration`), and every design of the stage is judged again on its
//! corrected margins. The offset of a lattice-dependent row is not the same
//! everywhere in the box, so it is measured where the search is, as the
//! search moves: the first calibration sees the scan, the later ones the
//! incumbent's neighbourhood. Measured on the LJ60 with the offsets of the
//! scan alone: 33 of 43 promoted candidates failed the static-margin floor
//! on the refinement model.
//!
//! The fidelity therefore rises as the search narrows: wide and cheap
//! first, then cheap but corrected by the finer model where it matters,
//! then the finer model itself (the refinement stage), then the reporting
//! analysis of the finalists.

use std::cell::RefCell;

use super::super::*;
use crate::search_methods::{Guidance, Tier};

/// Kernel rounds of one exploration, each followed (but the last) by a
/// calibration (engineering choice: enough re-calibrations for the offsets
/// to follow the incumbent, each round still tens of generations long).
const ROUNDS: usize = 3;

/// Kernel rounds of a clean-sheet exploration: two, so two calibrations.
/// Every clean-sheet calibration point is a refinement analysis that sizes
/// the fuselage and the engine, about twice the cost of a reference-mode
/// one, and [`CALIBRATION_WINDOW`] measures the offsets on the last two
/// calibrations anyway, so a third adds a wave of the dearest analyses for
/// an estimate that still rests on two (engineering choice, so the clean
/// sheet keeps the five-minute plan; the population is kept).
const CLEAN_SHEET_ROUNDS: usize = 2;

/// The kernel rounds of an exploration in `mode`.
pub(super) fn rounds(mode: alas_config::DesignMode) -> usize {
    if mode == alas_config::DesignMode::CleanSheet {
        CLEAN_SHEET_ROUNDS
    } else {
        ROUNDS
    }
}

/// Most recent calibration pairs the offsets are measured on: two
/// calibrations, so the estimate follows the search without resting on one
/// wave.
const CALIBRATION_WINDOW: usize = 2 * calibration::CALIBRATION_POINTS;

/// Initial and floor population of the exploration kernel (engineering
/// choice). Thirty members on at most thirteen free variables is the
/// lower clamp region of `product_de::initial_population`; a round of one
/// to three hundred candidates then affords six to sixteen generations
/// (Piotrowski 2017, cited in `product_de`: small budgets favour small
/// populations). Whole multiples of `product_de::NATIVE_LANE_QUANTUM`.
const POPULATION: usize = 2 * product_de::NATIVE_LANE_QUANTUM;
const MINIMUM_POPULATION: usize = product_de::NATIVE_LANE_QUANTUM;

/// Whether the exploration kernel is guided by the surrogate. On unless the
/// environment variable `ALAS_OPT_GUIDE` is `off`, which runs the plain
/// kernel for a like-for-like comparison of the two (study use).
fn guide_enabled() -> bool {
    std::env::var("ALAS_OPT_GUIDE").map_or(true, |value| value != "off")
}

/// The stage the exploration runs inside.
pub(super) struct Stage<'a, 'r> {
    pub(super) run: &'a Run<'r>,
    pub(super) solver: &'a alas_config::SolverSettings,
    pub(super) scope: &'a CancelScope<'a>,
    pub(super) started: Instant,
    pub(super) limit: Option<Duration>,
    pub(super) max_rejects: usize,
    /// Analysed candidates the exploration may still spend.
    pub(super) remaining: usize,
    /// Kernel rounds ([`rounds`]), each opened by a calibration.
    pub(super) rounds: usize,
}

/// What the exploration did.
pub(super) struct Explored {
    pub(super) analysed: usize,
    pub(super) generations: usize,
    pub(super) termination: Termination,
    pub(super) best_by_generation: Vec<Option<f64>>,
    pub(super) calibration: Option<String>,
}

impl Default for Explored {
    fn default() -> Self {
        Self {
            analysed: 0,
            generations: 0,
            termination: Termination::EvaluationBudget,
            best_by_generation: Vec::new(),
            calibration: None,
        }
    }
}

/// The margins of an analysed candidate of this stage.
fn margins_of<S: SearchObjective + ?Sized>(
    evaluator: &BatchEvaluator<'_, S>,
    values: &[f64],
) -> Option<calibration::Margins> {
    evaluator
        .cache
        .row(values)
        .and_then(|row| evaluator.objective.margins(row))
        .map(<[_]>::to_vec)
}

/// Run the exploration. `scored` holds the scan's scores on entry and every
/// design of the stage, judged on the final calibration, on return.
pub(super) fn run<S: SearchObjective + ?Sized>(
    stage: Stage<'_, '_>,
    evaluator: &mut BatchEvaluator<'_, S>,
    scored: &mut Vec<ScoredPoint>,
    mut calibrate: Option<&mut calibration::Calibrate<'_>>,
) -> Explored {
    let run = stage.run;
    let mut explored = Explored::default();
    // Every design as the model scored it; `scored` is this list re-judged.
    let mut raw: Vec<ScoredPoint> = scored.clone();
    let mut pairs: Vec<(calibration::Margins, calibration::Margins)> = Vec::new();
    let mut paired: Vec<Vec<f64>> = Vec::new();
    let mut correction = calibration::Correction::default();
    let rejudge = |evaluator: &BatchEvaluator<'_, S>,
                   correction: &calibration::Correction,
                   raw: &[ScoredPoint]| {
        raw.iter()
            .map(|point| {
                let mut point = point.clone();
                let margins = margins_of(evaluator, &point.values);
                correction.rescore(&mut point, margins.as_deref());
                point
            })
            .collect::<Vec<_>>()
    };
    let mut remaining = stage.remaining;
    for round in 0..stage.rounds {
        // Calibrate on the baseline and the best designs not yet compared.
        if let Some(calibrate) = calibrate.as_mut() {
            let mut points: Vec<Vec<f64>> = run
                .baseline
                .iter()
                .filter(|values| !paired.contains(values))
                .cloned()
                .collect();
            let candidates = elite::best_distinct(
                scored,
                run.bounds,
                calibration::CALIBRATION_POINTS,
                super::PROMOTION_NICHE_RADIUS,
                &paired,
            );
            for point in candidates {
                let analysed = matches!(point.tier, Tier::Feasible | Tier::ClosedInfeasible);
                if points.len() < calibration::CALIBRATION_POINTS
                    && analysed
                    && !points.contains(&point.values)
                {
                    points.push(point.values);
                }
            }
            if !points.is_empty() {
                let fine = calibrate(&points);
                for (values, fine) in points.iter().zip(fine) {
                    if let (Some(coarse), Some(fine)) = (margins_of(evaluator, values), fine) {
                        pairs.push((coarse, fine));
                    }
                }
                paired.extend(points);
                let window = pairs.len().saturating_sub(CALIBRATION_WINDOW);
                correction = calibration::Correction::from_pairs(&pairs[window..]);
                *scored = rejudge(evaluator, &correction, &raw);
                explored.calibration = Some(correction.describe());
            }
        }
        if stage.scope.requested() {
            explored.termination = Termination::Cancelled;
            break;
        }
        let share = remaining / (stage.rounds - round);
        if share == 0 {
            continue;
        }
        // The kernel, from the baseline and the best designs so far.
        let mut ranked: Vec<&ScoredPoint> = scored.iter().collect();
        ranked.sort_by_key(|point| point.feasibility_key());
        let mut seeds: Vec<Vec<f64>> = run.baseline.iter().cloned().collect();
        for point in ranked {
            if seeds.len() >= POPULATION {
                break;
            }
            if !seeds.contains(&point.values) {
                seeds.push(point.values.clone());
            }
        }
        // The kernel counts its seeds, which the stage's exact cache
        // answers: they ride on top of the budget of new analyses.
        let seeded = seeds.len();
        let settings = product_de::Settings {
            max_evaluations: share + seeded,
            schedule_evaluations: share + seeded,
            stop_after: share + seeded,
            population: POPULATION,
            minimum_population: MINIMUM_POPULATION,
            population_quantum: product_de::NATIVE_LANE_QUANTUM,
            // A stream of its own per round: the refinement kernel draws
            // from the run seed itself.
            seed: run.seed ^ 0x9e37_79b9_7f4a_7c15_u64.wrapping_mul(round as u64 + 1),
            adaptation: stage.solver.parameter_adaptation,
            spread_tolerance: stage.solver.tolerance.max(0.0),
            // The exploration runs its whole candidate plan.
            stagnation_generations: usize::MAX,
            time_limit: stage.limit,
            intra_batch_deadline: evaluator.objective.honors_deadline(),
            infeasible_reserve: 0,
            max_rejects: stage.max_rejects.saturating_sub(evaluator.pre_gate_rejects),
        };
        // The surrogate of everything this stage has analysed, on the
        // current calibration, orders the kernel's alternative trials.
        let surrogate = RefCell::new(guide::SurrogateGuide::new(run.bounds));
        let observe = |evaluator: &BatchEvaluator<'_, S>, scores: &[ScoredPoint]| {
            let history = evaluator.objective.history();
            let mut surrogate = surrogate.borrow_mut();
            for point in scores {
                let analysed = matches!(point.tier, Tier::Feasible | Tier::ClosedInfeasible);
                let Some(row) = evaluator.cache.row(&point.values).filter(|_| analysed) else {
                    continue;
                };
                if let (Some(&objective), Some(margins)) = (
                    history.objective_value.get(row),
                    evaluator.objective.margins(row),
                ) {
                    surrogate.observe(&point.values, objective, &correction.corrected(margins));
                }
            }
        };
        observe(evaluator, &raw);
        let mut choose = |alternatives: &[Vec<f64>]| surrogate.borrow_mut().choose(alternatives);
        let mut fresh: Vec<ScoredPoint> = Vec::new();
        let search = product_de::run_guided(
            run.bounds,
            &seeds,
            settings,
            stage.started,
            stage.scope,
            Some(&|values| run.admits(values)),
            guide_enabled().then_some(Guidance {
                alternatives: guide::ALTERNATIVES,
                choose: &mut choose,
            }),
            &mut |points: &[Vec<f64>]| {
                let scores = evaluator.evaluate_block(points);
                observe(evaluator, &scores);
                fresh.extend(scores.iter().cloned());
                scores
                    .into_iter()
                    .map(|mut point| {
                        let margins = margins_of(evaluator, &point.values);
                        correction.rescore(&mut point, margins.as_deref());
                        point
                    })
                    .collect()
            },
        );
        let analysed = search.evaluations.saturating_sub(seeded);
        explored.analysed += analysed;
        remaining = remaining.saturating_sub(analysed);
        explored.generations += search.generations_completed;
        explored
            .best_by_generation
            .extend(search.best_by_generation);
        explored.termination = search.termination;
        // The kernel's seeds came back from the cache: keep the new designs.
        for point in fresh {
            if !raw.iter().any(|known| known.values == point.values) {
                raw.push(point);
            }
        }
        *scored = rejudge(evaluator, &correction, &raw);
        if search.termination != Termination::EvaluationBudget {
            break;
        }
    }
    explored
}
