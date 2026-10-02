// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The product search: a screening stage over the anchored design box, a
//! diverse elite, then the refinement kernel at full in-loop fidelity, each
//! stage under its own evaluation budget and wall-clock limit.

use super::batch::BatchEvaluator;
use super::evaluation_cache::EvaluationCache;
use super::*;
use crate::search::planform_projection::PlanformProjection;
use crate::search::{elite, screening};
use crate::search_methods::product_de::Termination;
use crate::search_methods::ScoredPoint;
use crate::{BaselineComparison, SizingWorkSummary, StageRejections, StageSummary, SEARCH_SCOPE};

mod screen;

/// Share of the refinement's initial population filled from the screening
/// elite; the baseline takes one member and a Latin hypercube the rest
/// (engineering choice: half known-good starts, half fresh coverage).
const ELITE_POPULATION_FRACTION: f64 = 0.5;

/// Which model the screening stage evaluates with.
pub(super) enum ScreeningModel<'a> {
    /// An external (delegated) model: no screening stage, the refinement
    /// starts from the baseline, and the pre-gate checks only the box because
    /// the model's constraint set is its own.
    Skip,
    /// The refinement's own model. Its scores enter the run's history and
    /// the refinement reuses them exactly, so the elite and the baseline cost
    /// nothing to re-score.
    Same,
    /// A distinct screening model; its scores stay out of the history and
    /// the refinement re-scores every member it keeps.
    Separate(&'a mut dyn SearchObjective),
}

/// What the screening stage hands on.
struct Screened {
    summary: StageSummary,
    rejections: StageRejections,
    elite: Vec<ScoredPoint>,
    cancelled: bool,
    /// The exact scores, when the refinement evaluates with the same model.
    cache: Option<EvaluationCache>,
}

/// The run's shared inputs.
struct Run<'a> {
    bounds: &'a [(f64, f64)],
    /// The baseline clamped into the box, and whether that moved it.
    baseline: Option<Vec<f64>>,
    baseline_clamped: bool,
    seed: u64,
    workers: usize,
    cancel: Option<&'a AtomicBool>,
    /// The configuration the design-vector pre-gate checks, for the native
    /// model only.
    pre_gate: Option<&'a AlasConfig>,
}

impl Run<'_> {
    /// Whether `values` reaches analysis: the baseline always does, anything
    /// else when it passes the design-vector pre-gate.
    fn admits(&self, values: &[f64]) -> bool {
        self.baseline.as_deref() == Some(values)
            || super::batch::pre_gate_violation(values, self.bounds, self.pre_gate).is_none()
    }
}

impl DesignOptimizer {
    /// Run the screening stage, select the elite, then run the refinement on
    /// `objective`. Every candidate the result reports was scored by
    /// `objective`, the full coupled in-loop evaluation; screening scores
    /// only choose where the refinement starts.
    pub(super) fn run_product_search<E: SearchObjective>(
        &self,
        bounds: &[(f64, f64)],
        baseline: Option<&DesignVector>,
        objective: &mut E,
        screening_model: ScreeningModel<'_>,
        progress_callback: Option<&mut dyn FnMut(&str)>,
        cancel: Option<&AtomicBool>,
    ) -> OptimizationResult {
        let solver = &self.config.optimizer.solver;
        let started = Instant::now();
        let mut progress_callback = progress_callback;
        let mut report = |line: &str| {
            if let Some(callback) = progress_callback.as_mut() {
                callback(line);
            }
        };
        let clamped = baseline.map(|design| {
            let mut values = design.to_array();
            crate::search_methods::clamp_to_bounds(&mut values, bounds);
            (values != design.to_array(), values)
        });
        let run = Run {
            bounds,
            baseline_clamped: clamped.as_ref().is_some_and(|(moved, _)| *moved),
            baseline: clamped.map(|(_, values)| values),
            seed: solver.seed.map_or_else(runtime_seed, |value| value as u64),
            workers: solver.resolved_workers(),
            cancel,
            pre_gate: (!matches!(screening_model, ScreeningModel::Skip)).then_some(&self.config),
        };
        let scope = CancelScope::attach(cancel);
        let dimension = bounds.iter().filter(|(lower, upper)| upper > lower).count();
        // Elite sized for the ceiling's initial population, which a replay
        // keeps; a smaller planned population takes the first members.
        let ceiling =
            usize::try_from(solver.refinement.max_evaluations.max(1)).unwrap_or(usize::MAX);
        let elite_size = elite_members_kept(
            product_de::refinement_settings(solver, dimension, run.seed, ceiling).population,
        );
        let screened = match screening_model {
            _ if dimension == 0 => None,
            ScreeningModel::Skip => None,
            ScreeningModel::Same => Some(self.screen(&run, &mut *objective, elite_size, true)),
            ScreeningModel::Separate(model) => Some(self.screen(&run, model, elite_size, false)),
        };
        let mut screening_throughput = None;
        let mut stages = Vec::new();
        let mut rejections = Vec::new();
        let mut elite_members = Vec::new();
        let mut cache = None;
        if let Some(screened) = screened {
            report(&format!(
                "screening | evaluations {} | rejected {} | feasible {} | elite {} | termination {} | wall_s {:.3}",
                screened.summary.evaluations,
                screened.summary.pre_gate_rejects,
                screened.summary.feasible,
                screened.elite.len(),
                screened.summary.termination,
                screened.summary.wall_time_s
            ));
            stages.push(screened.summary);
            rejections.push(screened.rejections);
            if screened.cancelled {
                scope.search_finished(CANCELLED);
                let winner = product_de::unevaluated(run.baseline.as_deref().unwrap_or(&[]));
                return self.finish(
                    &run,
                    objective.history(),
                    MethodOutcome {
                        winner,
                        pareto_front: Vec::new(),
                    },
                    CANCELLED,
                    started,
                    SearchDiagnostics {
                        stages,
                        rejections,
                        ..empty_diagnostics()
                    },
                );
            }
            let summary = stages.last().map_or((0, 0.0, 0.0), |s: &StageSummary| {
                (s.analysis_evaluations, s.wall_time_s, s.lane_utilization)
            });
            screening_throughput = Some(summary);
            elite_members = screened.elite;
            cache = screened.cache;
        }

        let mut seeds: Vec<Vec<f64>> = run
            .baseline
            .iter()
            .cloned()
            .chain(elite_members.iter().map(|point| point.values.clone()))
            .collect();
        let stage_started = Instant::now();
        let mut evaluator =
            BatchEvaluator::new(objective, run.workers, bounds, run.pre_gate, cancel);
        evaluator.baseline = run.baseline.as_deref();
        if let Some(cache) = cache {
            evaluator.cache = cache;
        }
        let before = evaluator.analyses();
        let pilot_rows = evaluator.objective.history().n_evaluations();
        // Refinement throughput: its model analyses baseline and elite in one
        // batch (cache hits later); rate = lanes the screening kept busy over
        // their mean lane time. A 12-member pilot under-loaded the lanes and
        // planned 1.4-2x what the time afforded (A320-200, 32 threads). The
        // screening's own model reuses its scores and its throughput.
        let busy = evaluator.busy;
        if dimension > 0 && screening_throughput.is_some() {
            evaluator.evaluate_block(&seeds);
        }
        let piloted = evaluator.analyses() - before;
        let rate = screening_throughput.map(|(analysed, wall_s, utilization)| {
            if piloted > 0 {
                let lane_s = evaluator.busy.saturating_sub(busy).as_secs_f64() / piloted as f64;
                evaluator.lanes() as f64 * utilization.clamp(0.05, 1.0) / lane_s
            } else {
                analysed as f64 / wall_s
            }
        });
        let planned = product_de::planned_refinement_budget(solver, rate);
        let mut refinement = product_de::refinement_settings(solver, dimension, run.seed, planned);
        if dimension == 0 {
            refinement.population = 1;
        }
        let search_budget = refinement.max_evaluations;
        let kept = usize::from(run.baseline.is_some()) + elite_members_kept(refinement.population);
        // Pilot analyses of seeds the planned population drops are charged
        // to the budget, so the stage's analyses never exceed it; the count
        // is a function of the replayed screening, so a replay charges it too.
        let history = evaluator.objective.history();
        let dropped = seeds
            .get(kept..)
            .unwrap_or_default()
            .iter()
            .filter(|values| {
                evaluator.cache.row(values).is_some_and(|row| {
                    row >= pilot_rows
                        && history.reject_reason.get(row).map(String::as_str)
                            != Some(super::batch::CANCELLED_UNSTARTED)
                })
            });
        refinement.max_evaluations = search_budget.saturating_sub(dropped.count()).max(1);
        refinement.stop_after = refinement.stop_after.min(refinement.max_evaluations);
        seeds.truncate(kept);
        report(&format!(
            "refinement | population {} | budget {} | seed {} | workers {}",
            refinement.population, refinement.max_evaluations, run.seed, run.workers
        ));
        let hits_before = evaluator.cache.hits;
        let mut outcome = product_de::run(
            bounds,
            &seeds,
            refinement,
            stage_started,
            &scope,
            &mut |points: &[Vec<f64>]| evaluator.evaluate_block(points),
        );
        let kernel_evaluations = outcome.evaluations;
        let remaining = (
            (refinement
                .max_evaluations
                .saturating_sub(outcome.evaluations))
            .min(refinement.max_rejects.saturating_sub(outcome.rejected)),
            product_de::restoration_replay(&solver.refinement)
                .unwrap_or_else(|| refinement.stop_after.saturating_sub(outcome.evaluations)),
        );
        let restoration = feasibility_restoration::run(
            bounds,
            &mut outcome,
            remaining,
            (
                stage_started,
                refinement.time_limit.unwrap_or(Duration::MAX),
            ),
            &scope,
            &mut evaluator,
        );
        let wall = stage_started.elapsed().as_secs_f64();
        let analysis_evaluations = evaluator.analyses() - before;
        let termination = if outcome.termination == Termination::Cancelled || dimension > 0 {
            outcome.termination.label()
        } else {
            "fixed_bounds"
        };
        let (candidate_time_s, lane_utilization) =
            evaluator.lane_statistics(analysis_evaluations, wall);
        stages.push(StageSummary {
            stage: "refinement".to_owned(),
            max_evaluations: usize::try_from(solver.refinement.max_evaluations.max(1))
                .unwrap_or(usize::MAX),
            planned_evaluations: planned,
            reserved_evaluations: planned.saturating_sub(search_budget),
            time_limit_s: solver.refinement.time_limit_s,
            time_limited: refinement.time_limit.is_some(),
            evaluations: outcome.evaluations,
            restoration_evaluations: outcome.evaluations - kernel_evaluations,
            pre_gate_rejects: evaluator.pre_gate_rejects,
            analysis_evaluations,
            cancelled_unstarted: evaluator.cancelled_unstarted,
            generations: outcome.generations_completed,
            feasible: evaluator.feasible,
            elite_size: outcome.population_initial,
            wall_time_s: wall,
            candidate_time_s,
            lane_utilization,
            termination: termination.to_owned(),
            sizing_work: SizingWorkSummary::of_history(evaluator.objective.history(), before),
        });
        report(&format!(
            "refinement | evaluations {} | rejected {} | generations {} | termination {termination} | wall_s {wall:.3}",
            outcome.evaluations, evaluator.pre_gate_rejects, outcome.generations_completed
        ));
        rejections.push(StageRejections::new(
            "refinement",
            refinement.max_rejects,
            evaluator.reasons,
            evaluator
                .objective
                .history()
                .reject_reason
                .get(before..)
                .unwrap_or_default(),
        ));
        scope.search_finished(termination);

        // Rows are looked up by the requested vector, which the cache keys
        // on; the history's own vector may carry a re-sized tail.
        let winner_row = evaluator.cache.row(&outcome.winner.values);
        let baseline_comparison = run
            .baseline
            .as_ref()
            .and(outcome.first_scored.as_ref())
            .map(|first| {
                let history = evaluator.objective.history();
                let fuel_of = |row: Option<usize>| {
                    row.and_then(|row| history.block_fuel_kg.get(row).copied())
                        .filter(|fuel| fuel.is_finite())
                };
                BaselineComparison::new(
                    first.valid(),
                    first.objectives[0],
                    outcome.winner.objectives[0],
                    (
                        fuel_of(evaluator.cache.row(&first.values)),
                        fuel_of(winner_row),
                    ),
                )
            });
        let diagnostics = SearchDiagnostics {
            restoration,
            converged: outcome.termination == Termination::Converged,
            analysis_evaluations,
            cache_hits: evaluator.cache.hits - hits_before,
            poll_iterations: outcome.generations_completed,
            verification_evaluations: elite_members.len(),
            search_wall_time_s: wall,
            poll_block_size: outcome.population_initial,
            first_feasible_cost: outcome.first_feasible_cost,
            relative_improvement: outcome
                .first_feasible_cost
                .filter(|_| outcome.winner.valid())
                .map(|first| (first - outcome.winner.cost) / first.abs().max(1e-12)),
            feasible_fraction: outcome.feasible_fraction,
            epsilon_level: outcome.epsilon_final,
            stages,
            rejections,
            baseline: baseline_comparison,
            winner_history_row: winner_row,
            ..empty_diagnostics()
        };
        let winner = MethodOutcome {
            winner: outcome.winner,
            pareto_front: Vec::new(),
        };
        let history = evaluator.objective.history();
        self.finish(&run, history, winner, termination, started, diagnostics)
    }

    /// The result, with the diagnostics every exit reports.
    fn finish(
        &self,
        run: &Run<'_>,
        history: &OptimizationHistory,
        outcome: MethodOutcome,
        termination: &str,
        started: Instant,
        mut diagnostics: SearchDiagnostics,
    ) -> OptimizationResult {
        let screening = diagnostics.stages.iter().find(|s| s.stage == "screening");
        diagnostics.screening_evaluations = screening.map_or(0, |s| s.evaluations);
        diagnostics.screening_feasible = screening.map_or(0, |s| s.feasible);
        diagnostics.scan_wall_time_s = screening.map_or(0.0, |s| s.wall_time_s);
        diagnostics.workers = run.workers;
        diagnostics.seed = Some(run.seed);
        diagnostics.baseline_clamped = run.baseline_clamped;
        diagnostics.scope = SEARCH_SCOPE.to_owned();
        let mut result = result_from_method(
            outcome,
            product_de::METHOD,
            product_de::STRATEGY,
            termination,
            history,
            started.elapsed().as_secs_f64(),
        );
        result.search_diagnostics = Some(diagnostics);
        result
    }
}

/// Screening elite members an initial population of `population` keeps
/// beside the baseline ([`ELITE_POPULATION_FRACTION`]).
fn elite_members_kept(population: usize) -> usize {
    (population.saturating_sub(1) as f64 * ELITE_POPULATION_FRACTION) as usize
}

fn empty_diagnostics() -> SearchDiagnostics {
    SearchDiagnostics {
        restoration: None,
        converged: false,
        analysis_evaluations: 0,
        cache_hits: 0,
        poll_iterations: 0,
        screening_evaluations: 0,
        screening_feasible: 0,
        verification_evaluations: 0,
        scan_wall_time_s: 0.0,
        search_wall_time_s: 0.0,
        workers: 0,
        poll_block_size: 0,
        first_feasible_cost: None,
        relative_improvement: None,
        feasible_fraction: 0.0,
        epsilon_level: 0.0,
        stages: Vec::new(),
        rejections: Vec::new(),
        seed: None,
        scope: String::new(),
        baseline: None,
        winner_history_row: None,
        baseline_clamped: false,
    }
}

// A test asserts on values it built here, so a failed expect is the
// assertion failing rather than a library invariant being broken.
#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    /// Records a derived coordinate in place of the requested one, as the
    /// reference adaptation's tail sizing does with `tail_scale`.
    struct Resizing(OptimizationHistory);

    impl SearchObjective for Resizing {
        fn evaluate(&mut self, design: &[f64]) -> f64 {
            let mut recorded = DesignVector::from_array(design).unwrap_or_default();
            let cost = recorded.span_m;
            recorded.tail_scale *= 1.07;
            self.0.record_mission_sized(
                recorded,
                true,
                cost,
                17.0,
                recorded.span_m,
                2.0,
                120.0,
                0.0,
                "",
                cost,
                70_000.0,
                100.0 * cost,
                0.0,
                0.0,
            );
            cost
        }
        fn history(&self) -> &OptimizationHistory {
            &self.0
        }
    }

    #[test]
    fn baseline_and_winner_fuel_come_from_their_own_rows_when_the_recorded_vector_differs() {
        let mut config = AlasConfig::default();
        config.optimizer.solver.screening.max_evaluations = 16;
        config.optimizer.solver.refinement.max_evaluations = 60;
        config.optimizer.solver.seed = Some(3);
        config.optimizer.solver.stop_on_evaluations_only = true;
        let nominal = DesignVector::default();
        let mut bounds: Vec<(f64, f64)> = nominal
            .to_array()
            .iter()
            .map(|&value| (value, value))
            .collect();
        bounds[0] = (0.95 * nominal.span_m, nominal.span_m);
        let mut objective = Resizing(OptimizationHistory::new());
        let result = DesignOptimizer::new(config).run_product_search(
            &bounds,
            Some(&nominal),
            &mut objective,
            ScreeningModel::Same,
            None,
            None,
        );
        let history = &result.history;
        assert!(!history.design_vectors.contains(&result.best_design));
        let diagnostics = result.search_diagnostics.as_ref().expect("diagnostics");
        let comparison = diagnostics.baseline.as_ref().expect("baseline evaluated");
        assert_eq!(
            comparison.baseline_block_fuel_kg,
            Some(100.0 * nominal.span_m)
        );
        assert_eq!(
            comparison.winner_block_fuel_kg,
            Some(100.0 * result.best_design.span_m)
        );
        assert!(comparison
            .relative_change
            .is_some_and(|change| change < 0.0));
        // The ladder offers the winner once, not again as its own row.
        let row = diagnostics.winner_history_row.expect("winner row");
        assert_eq!(
            history.design_vectors[row].span_m,
            result.best_design.span_m
        );
        let ladder = result.ranked_hard_feasible_candidates(4);
        assert!(!ladder[1..].contains(&history.design_vectors[row]));
    }

    /// Scores a batch across `workers` threads that each spend `.1` ms per
    /// candidate, then records it in input order, as the native objective
    /// does: more workers finish a batch sooner, never differently.
    struct Timed(OptimizationHistory, u32);

    impl SearchObjective for Timed {
        fn evaluate(&mut self, design: &[f64]) -> f64 {
            self.evaluate_batch(&[design.to_vec()], 1)[0].0
        }
        fn history(&self) -> &OptimizationHistory {
            &self.0
        }
        fn evaluate_batch(&mut self, designs: &[Vec<f64>], workers: usize) -> Vec<(f64, bool)> {
            let chunk = designs.len().div_ceil(workers.max(1)).max(1);
            let lane_ms = self.1;
            std::thread::scope(|scope| {
                for part in designs.chunks(chunk) {
                    scope.spawn(move || {
                        std::thread::sleep(
                            Duration::from_millis(u64::from(lane_ms)) * part.len() as u32,
                        );
                    });
                }
            });
            designs
                .iter()
                .map(|values| {
                    let design = DesignVector::from_array(values).unwrap_or_default();
                    let cost = (design.span_m - 0.97 * DesignVector::default().span_m).powi(2);
                    self.0
                        .record(design, true, cost, 17.0, design.span_m, 2.0, 120.0, 0.0, "");
                    (cost, true)
                })
                .collect()
        }
    }

    fn timed_run(config: &AlasConfig, workers: i64, span_upper: f64) -> OptimizationResult {
        let mut config = config.clone();
        config.optimizer.solver.workers = workers;
        let nominal = DesignVector::default();
        let mut bounds: Vec<(f64, f64)> = nominal
            .to_array()
            .iter()
            .map(|&value| (value, value))
            .collect();
        bounds[0] = (0.9 * nominal.span_m, span_upper * nominal.span_m);
        let mut objective = Timed(OptimizationHistory::new(), 2);
        DesignOptimizer::new(config).run_product_search(
            &bounds,
            Some(&nominal),
            &mut objective,
            ScreeningModel::Same,
            None,
            None,
        )
    }

    #[test]
    fn replaying_the_recorded_counts_of_a_time_limited_run_is_bit_identical_at_any_worker_count() {
        let mut config = AlasConfig::default();
        config.optimizer.solver.seed = Some(11);
        config.optimizer.solver.screening.max_evaluations = 2_000;
        config.optimizer.solver.screening.time_limit_s = 0.1;
        config.optimizer.solver.refinement.max_evaluations = 2_000;
        config.optimizer.solver.refinement.time_limit_s = 0.4;
        // The time limit, not stagnation, must end the refinement.
        config.optimizer.solver.convergence_stagnation_generations = 1_000_000;
        // Spans above the code F limit fail the pre-gate in both stages.
        let limited = timed_run(&config, 1, 1.4);
        let diagnostics = limited.search_diagnostics.as_ref().expect("diagnostics");
        let stages = &diagnostics.stages;
        assert!(stages.iter().all(|stage| stage.pre_gate_rejects > 0));
        for (stage, rejections) in stages.iter().zip(&diagnostics.rejections) {
            assert_eq!(rejections.pre_gate.total(), stage.pre_gate_rejects);
            assert!(rejections.pre_gate.span_code > 0);
        }
        for stage in stages {
            assert_eq!(stage.termination, "time_budget", "{}", stage.stage);
            assert!(stage.time_limited);
            assert!(stage.evaluations < stage.max_evaluations);
        }
        let count = |name: &str| {
            stages
                .iter()
                .find(|stage| stage.stage == name)
                .and_then(|stage| i64::try_from(stage.evaluations).ok())
        };
        // The refinement planned its budget from the screening throughput,
        // about 500 analyses per second of one 2 ms lane: far below the
        // ceiling, so the floor applies.
        let refinement = &stages[1];
        let planned = refinement.planned_evaluations;
        assert_eq!(planned, product_de::MIN_PLANNED_EVALUATIONS);
        assert!(planned < refinement.max_evaluations);
        assert_eq!(
            refinement.elite_size,
            product_de::initial_population(planned - refinement.reserved_evaluations, 1)
        );
        let mut replay = config.clone();
        replay.optimizer.solver.screening.replay_evaluations = count("screening");
        replay.optimizer.solver.refinement.replay_evaluations = count("refinement");
        replay
            .optimizer
            .solver
            .refinement
            .replay_planned_evaluations = Some(planned as i64);
        for workers in [1, 8] {
            let replayed = timed_run(&replay, workers, 1.4);
            assert_eq!(bits(&replayed), bits(&limited), "{workers} workers");
            let replayed_stages = &replayed
                .search_diagnostics
                .as_ref()
                .expect("diagnostics")
                .stages;
            assert!(replayed_stages.iter().all(|stage| !stage.time_limited));
            // Both recorded numbers come back: analysed and rejected.
            let counts = |stages: &[StageSummary]| -> Vec<(usize, usize)> {
                stages
                    .iter()
                    .map(|stage| (stage.evaluations, stage.pre_gate_rejects))
                    .collect()
            };
            assert_eq!(counts(replayed_stages), counts(stages), "{workers} workers");
        }
    }

    /// A run with a 2 ms screening model and an 8 ms refinement model.
    fn separate_run(config: &AlasConfig, workers: i64) -> OptimizationResult {
        let mut config = config.clone();
        config.optimizer.solver.workers = workers;
        let nominal = DesignVector::default();
        let mut bounds: Vec<(f64, f64)> = nominal.to_array().iter().map(|&v| (v, v)).collect();
        bounds[0] = (0.9 * nominal.span_m, nominal.span_m);
        let mut screening = Timed(OptimizationHistory::new(), 2);
        let mut refinement = Timed(OptimizationHistory::new(), 8);
        DesignOptimizer::new(config).run_product_search(
            &bounds,
            Some(&nominal),
            &mut refinement,
            ScreeningModel::Separate(&mut screening),
            None,
            None,
        )
    }

    #[test]
    fn a_cheaper_screening_model_plans_on_the_refinement_cost_and_replays_exactly() {
        // One lane: the plan follows the refinement's 125 analyses per
        // second, not the screening's 500.
        let mut config = AlasConfig::default();
        config.optimizer.solver.seed = Some(3);
        config.optimizer.solver.screening.time_limit_s = 0.2;
        config.optimizer.solver.refinement.time_limit_s = 4.0;
        let result = separate_run(&config, 1);
        let nominal = DesignVector::default();
        let stages = &result
            .search_diagnostics
            .as_ref()
            .expect("diagnostics")
            .stages;
        let planned = stages[1].planned_evaluations;
        let solver = &config.optimizer.solver;
        let expected = product_de::planned_refinement_budget(solver, Some(125.0));
        let screening_rate = product_de::planned_refinement_budget(solver, Some(500.0));
        // Sleep overshoot and the screening's batch overheads move the
        // measured rate by a fraction either way.
        let ratio = planned as f64 / expected as f64;
        assert!((0.6..=1.3).contains(&ratio), "{planned} vs {expected}");
        assert!(
            planned < screening_rate / 2,
            "{planned} vs {screening_rate}"
        );
        // The piloted seeds lead the refinement history, the baseline first.
        assert_eq!(result.history.design_vectors[0], nominal);
        // The replay count and the planned budget reproduce the run, pilot
        // included, at any worker count.
        let count = |stage: &StageSummary| i64::try_from(stage.evaluations).ok();
        let mut replay = config.clone();
        replay.optimizer.solver.screening.replay_evaluations = count(&stages[0]);
        replay.optimizer.solver.refinement.replay_evaluations = count(&stages[1]);
        replay
            .optimizer
            .solver
            .refinement
            .replay_planned_evaluations = i64::try_from(planned).ok();
        for workers in [1, 8] {
            let replayed = separate_run(&replay, workers);
            assert_eq!(bits(&replayed), bits(&result), "{workers}");
        }
    }

    /// Every analysed and requested design, its exact cost and the winner.
    fn bits(result: &OptimizationResult) -> (Vec<DesignVector>, Vec<u64>, DesignVector) {
        let costs = result
            .history
            .cost
            .iter()
            .map(|cost| cost.to_bits())
            .collect();
        (
            result.history.design_vectors.clone(),
            costs,
            result.best_design,
        )
    }

    #[test]
    fn pre_gate_rejects_are_refilled_and_the_run_is_bit_identical_at_1_4_and_16_workers() {
        let mut config = AlasConfig::default();
        config.optimizer.solver.seed = Some(5);
        config.optimizer.solver.stop_on_evaluations_only = true;
        config.optimizer.solver.screening.max_evaluations = 200;
        config.optimizer.solver.refinement.max_evaluations = 120;
        // Spans above the code F limit (79.99 m) fail the pre-gate: about a
        // third of the box.
        let reference = timed_run(&config, 1, 1.4);
        let stages = &reference
            .search_diagnostics
            .as_ref()
            .expect("diagnostics")
            .stages;
        let screening = stages
            .iter()
            .find(|stage| stage.stage == "screening")
            .expect("screening");
        assert!(screening.pre_gate_rejects > 0);
        // Every batch but the last analysed a full batch whatever the rejects.
        assert!(
            screening.analysis_evaluations
                >= crate::search::screening::BATCH_SIZE * (screening.generations - 1)
        );
        assert!(stages.iter().all(|stage| stage.lane_utilization > 0.0));
        for workers in [4, 16] {
            assert_eq!(
                bits(&timed_run(&config, workers, 1.4)),
                bits(&reference),
                "{workers}"
            );
        }
    }

    /// The nominal and a box of `free` coordinates `+-1 %` around it.
    fn box_around(free: usize) -> (DesignVector, Vec<(f64, f64)>) {
        let nominal = DesignVector::default();
        let bounds = nominal
            .to_array()
            .iter()
            .enumerate()
            .map(|(index, &v)| {
                if index < free {
                    (0.99 * v.min(1.01 * v), 1.01 * v.max(0.99 * v))
                } else {
                    (v, v)
                }
            })
            .collect();
        (nominal, bounds)
    }

    /// The audit case: 13 free coordinates, the 20 000 ceiling and a planned
    /// budget of 240. The ceiling's elite (38 members) is piloted, the
    /// planned population keeps 11, and every analysis, pilot included,
    /// stays inside the refinement's search budget.
    #[test]
    fn the_refinement_analyses_stay_inside_the_planned_budget_pilot_included() {
        let mut config = AlasConfig::default();
        config.optimizer.solver.seed = Some(4);
        config.optimizer.solver.stop_on_evaluations_only = true;
        config.optimizer.solver.screening.max_evaluations = 400;
        config.optimizer.solver.refinement.max_evaluations = 20_000;
        config
            .optimizer
            .solver
            .refinement
            .replay_planned_evaluations = Some(240);
        config.optimizer.solver.convergence_stagnation_generations = 1_000_000;
        let (nominal, bounds) = box_around(13);
        let mut screening = Timed(OptimizationHistory::new(), 0);
        let mut refinement = Timed(OptimizationHistory::new(), 0);
        let result = DesignOptimizer::new(config).run_product_search(
            &bounds,
            Some(&nominal),
            &mut refinement,
            ScreeningModel::Separate(&mut screening),
            None,
            None,
        );
        let stages = &result
            .search_diagnostics
            .as_ref()
            .expect("diagnostics")
            .stages;
        let (screened, refined) = (&stages[0], &stages[1]);
        assert_eq!(refined.planned_evaluations, 240);
        let search_budget = refined.planned_evaluations - refined.reserved_evaluations;
        assert_eq!(search_budget, 230);
        // The pilot dropped seeds: the scenario under test.
        assert!(screened.elite_size > elite_members_kept(refined.elite_size));
        assert!(
            refined.analysis_evaluations <= search_budget,
            "{} analyses against a budget of {search_budget}",
            refined.analysis_evaluations
        );
        assert!(refined.evaluations <= search_budget);
        assert_eq!(refined.cancelled_unstarted, 0);
    }

    /// Always closed but infeasible by `1 + |span - 0.97 span_0|`. A batch of
    /// more than two candidates sleeps `.1` ms, so the kernel's time guard
    /// stops it with a fixed fraction of a batch left, which restoration's
    /// polls (two points, then one, free) then use.
    struct Infeasible(OptimizationHistory, u32);

    impl SearchObjective for Infeasible {
        fn evaluate(&mut self, design: &[f64]) -> f64 {
            self.evaluate_batch(&[design.to_vec()], 1)[0].0
        }
        fn history(&self) -> &OptimizationHistory {
            &self.0
        }
        fn evaluate_batch(&mut self, designs: &[Vec<f64>], _workers: usize) -> Vec<(f64, bool)> {
            if designs.len() > 2 {
                std::thread::sleep(Duration::from_millis(u64::from(self.1)));
            }
            designs
                .iter()
                .map(|values| {
                    let design = DesignVector::from_array(values).unwrap_or_default();
                    let miss = (design.span_m - 0.97 * DesignVector::default().span_m).abs();
                    self.0.record_mission_sized(
                        design,
                        false,
                        miss,
                        17.0,
                        design.span_m,
                        2.0,
                        120.0,
                        0.0,
                        "range",
                        miss,
                        70_000.0,
                        1_000.0,
                        1.0 + miss,
                        0.0,
                    );
                    (miss, false)
                })
                .collect()
        }
    }

    fn infeasible_run(config: &AlasConfig, workers: i64) -> OptimizationResult {
        let mut config = config.clone();
        config.optimizer.solver.workers = workers;
        let nominal = DesignVector::default();
        let mut bounds: Vec<(f64, f64)> = nominal.to_array().iter().map(|&v| (v, v)).collect();
        bounds[0] = (0.9 * nominal.span_m, nominal.span_m);
        let mut objective = Infeasible(OptimizationHistory::new(), 100);
        DesignOptimizer::new(config).run_product_search(
            &bounds,
            Some(&nominal),
            &mut objective,
            ScreeningModel::Same,
            None,
            None,
        )
    }

    #[test]
    fn a_run_that_enters_restoration_replays_each_phase_bit_identically_at_1_and_8_workers() {
        let mut config = AlasConfig::default();
        config.optimizer.solver.seed = Some(13);
        config.optimizer.solver.screening.time_limit_s = 0.1;
        config.optimizer.solver.refinement.time_limit_s = 0.3;
        config.optimizer.solver.convergence_stagnation_generations = 1_000_000;
        let limited = infeasible_run(&config, 1);
        let diagnostics = limited.search_diagnostics.as_ref().expect("diagnostics");
        let refined = &diagnostics.stages[1];
        assert_eq!(refined.termination, "time_budget");
        assert!(
            refined.restoration_evaluations > 0,
            "restoration did not run"
        );
        assert_eq!(
            Some(refined.restoration_evaluations),
            diagnostics
                .restoration
                .as_ref()
                .map(|restoration| restoration.analysis_evaluations)
        );
        let count = |value: usize| i64::try_from(value).ok();
        let mut replay = config.clone();
        let solver = &mut replay.optimizer.solver;
        solver.screening.replay_evaluations = count(diagnostics.stages[0].evaluations);
        solver.refinement.replay_evaluations = count(refined.evaluations);
        solver.refinement.replay_planned_evaluations = count(refined.planned_evaluations);
        // The combined count alone lets the kernel run on into the
        // restoration's share: a different run.
        assert_ne!(bits(&infeasible_run(&replay, 1)), bits(&limited));
        replay
            .optimizer
            .solver
            .refinement
            .replay_restoration_evaluations = count(refined.restoration_evaluations);
        for workers in [1, 8] {
            let replayed = infeasible_run(&replay, workers);
            assert_eq!(bits(&replayed), bits(&limited), "{workers} workers");
            let stages = &replayed
                .search_diagnostics
                .as_ref()
                .expect("diagnostics")
                .stages;
            assert_eq!(stages[1].evaluations, refined.evaluations);
            assert_eq!(
                stages[1].restoration_evaluations,
                refined.restoration_evaluations
            );
        }
    }

    #[test]
    fn a_baseline_outside_the_box_is_flagged_as_a_clamped_start() {
        let mut config = AlasConfig::default();
        config.optimizer.solver.seed = Some(2);
        config.optimizer.solver.stop_on_evaluations_only = true;
        config.optimizer.solver.screening.max_evaluations = 16;
        config.optimizer.solver.refinement.max_evaluations = 60;
        let nominal = DesignVector::default();
        let clamped = |upper: f64| {
            let mut bounds: Vec<(f64, f64)> = nominal.to_array().iter().map(|&v| (v, v)).collect();
            bounds[0] = (0.9 * nominal.span_m, upper * nominal.span_m);
            let mut objective = Timed(OptimizationHistory::new(), 0);
            DesignOptimizer::new(config.clone())
                .run_product_search(
                    &bounds,
                    Some(&nominal),
                    &mut objective,
                    ScreeningModel::Same,
                    None,
                    None,
                )
                .search_diagnostics
                .expect("diagnostics")
                .baseline_clamped
        };
        assert!(!clamped(1.0));
        assert!(clamped(0.95));
    }
}
