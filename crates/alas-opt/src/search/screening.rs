// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The screening stage: a space-filling sample of the design box, plus the
//! baseline, evaluated with the screening model in fixed-size batches.
//!
//! # The screening model
//!
//! [`ScreeningFidelity`] states exactly what the screening model changes
//! relative to the full in-loop evaluation, and it may change only work that
//! does not decide a candidate's rank: the geometry, mass and balance build,
//! every constraint and the mission physics are the run's own.
//! It may loosen the takeoff-mass closure tolerance, coarsen the in-loop
//! chordwise mesh, and set the mission model's starting step count and a
//! per-candidate work budget; the shipped descriptor coarsens the mesh and
//! the starting step count (see [`ScreeningFidelity::shipped`]). The
//! rank-correlation experiment in `alas-acceptance`
//! (`screening_rank_correlation`) measures whether a descriptor still ranks
//! candidates like the full model before it is shipped.
//!
//! # Determinism
//!
//! The sample is one sequence of Latin hypercubes of [`BATCH_SIZE`] points
//! drawn from the seeded stream; with the native model each drawn point's
//! root chord is projected onto its admissible planform interval
//! ([`super::planform_projection`]), a deterministic map. A batch takes points from it until
//! [`BATCH_SIZE`] of them pass the design-vector pre-gate, or the budget of
//! analysed points or the rejection cap ends. The evaluated points are
//! therefore a prefix of the sequence: a run stopped by its time limit
//! after `n` analysed points evaluated exactly the points a longer run
//! evaluates up to its `n`-th analysed one, and replays with
//! `stop_after = n`. The number of analyses per batch is fixed, never the
//! worker count.

use std::time::{Duration, Instant};

use alas_config::AlasConfig;

use crate::cancellation::{CancelPhase, CancelScope};
use crate::mdo::mission_model::SizingBudget;
use crate::mdo::SizingControls;
use crate::search_methods::product_de::Termination;
use crate::search_methods::rng::SearchRng;
use crate::search_methods::{latin_hypercube, EvaluateBatch, ScoredPoint};

use super::planform_projection::PlanformProjection;

/// Points per screening batch: a fixed number so the sample never depends on
/// the machine, large enough to occupy a 32-thread batch twice over
/// (engineering choice).
pub(crate) const BATCH_SIZE: usize = 64;

/// What the screening model changes relative to the full in-loop
/// evaluation. `None` in a field means "as configured".
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ScreeningFidelity {
    /// Takeoff-mass closure tolerance, kg. Never tightens the configured
    /// value.
    pub sizing_tolerance_kg: Option<f64>,
    /// In-loop vortex-lattice chordwise panel count
    /// (`AnalysisConfig::chordwise_resolution`). Never refines the
    /// configured mesh.
    pub chordwise_resolution: Option<i64>,
    /// Integration steps per planned segment the frozen-plan refinement
    /// starts from (`SizingControls::steps_per_segment`); the frozen count
    /// still meets the mission model's Richardson tolerance.
    pub steps_per_segment: Option<usize>,
    /// Work limit of one candidate's sizing closure
    /// (`SizingControls::budget`); an exhausted candidate is not closed.
    pub sizing_budget: Option<SizingBudget>,
}

impl ScreeningFidelity {
    /// The full in-loop model, unchanged.
    #[must_use]
    pub const fn full() -> Self {
        Self {
            sizing_tolerance_kg: None,
            chordwise_resolution: None,
            steps_per_segment: None,
            sizing_budget: None,
        }
    }

    /// The shipped screening model: the draft four-panel chordwise mesh
    /// (the `draft` fidelity preset's in-loop value) and frozen mission
    /// plans refined from two integration steps per segment instead of four.
    ///
    /// The shipping rule is a ranking-key Spearman rho of at least 0.95 and
    /// a top-k overlap of at least 90 % against the full model on every
    /// preset measured. With `screening_rank_correlation` (60 seeded
    /// screening candidates per preset, seed 20260930) it gave rho 1.000,
    /// 0.999, 0.996 and 1.000 on A320-200, B787-9, A380-800 and ATR72-600,
    /// top-10 overlap 10 of 10 on all four and elite (29) overlap 29, 29, 28
    /// and 29, with the same feasible set, for 30 to 55 % less wall time per
    /// candidate. A 50 kg closure tolerance failed the rule (B787-9 rho
    /// 0.87, top-10 overlap 8 of 10) and is not used.
    #[must_use]
    pub const fn shipped() -> Self {
        Self {
            chordwise_resolution: Some(4),
            steps_per_segment: Some(2),
            ..Self::full()
        }
    }

    /// `config` as the screening model evaluates it.
    #[must_use]
    pub fn configure(&self, config: &AlasConfig) -> AlasConfig {
        let mut screening = config.clone();
        if let Some(tolerance) = self.sizing_tolerance_kg {
            let objective = &mut screening.optimizer.objective;
            objective.sizing_tolerance_kg = objective.sizing_tolerance_kg.max(tolerance);
        }
        if let Some(panels) = self.chordwise_resolution.filter(|&panels| panels > 0) {
            let analysis = &mut screening.analysis;
            analysis.chordwise_resolution = analysis.chordwise_resolution.min(panels);
        }
        screening
    }

    /// The sizing controls the screening model evaluates every candidate
    /// with.
    #[must_use]
    pub fn controls(&self) -> SizingControls {
        SizingControls {
            budget: self.sizing_budget,
            initial_takeoff_mass_kg: None,
            steps_per_segment: self.steps_per_segment,
        }
    }
}

/// Resolved limits of one screening run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Settings {
    /// Points that pass the pre-gate (analysed) the stage may evaluate.
    pub(crate) max_evaluations: usize,
    pub(crate) stop_after: usize,
    /// Pre-gate rejections after which the stage stops.
    pub(crate) max_rejects: usize,
    pub(crate) time_limit: Option<Duration>,
    /// Lanes evaluating a batch in parallel; sizes only the last,
    /// time-fitted batch ([`fitted_room`]).
    pub(crate) workers: usize,
    pub(crate) seed: u64,
    /// The root-chord projection of the sampler, for the native model.
    pub(crate) projection: Option<PlanformProjection>,
}

/// What one screening run evaluated and why it stopped.
pub(crate) struct Outcome {
    /// Every evaluated point, in sample order; the baseline first when given.
    pub(crate) scored: Vec<ScoredPoint>,
    /// Points among them that passed the pre-gate.
    pub(crate) analysed: usize,
    pub(crate) batches: usize,
    pub(crate) termination: Termination,
}

/// The screening sequence: Latin hypercubes of [`BATCH_SIZE`] points drawn
/// from the seeded stream one after another, the baseline replacing the
/// first point of the first.
/// Every drawn point but the baseline is projected when a projection is
/// given.
struct Sequence<'a> {
    bounds: &'a [(f64, f64)],
    baseline: Option<&'a [f64]>,
    projection: Option<PlanformProjection>,
    rng: SearchRng,
    pending: std::vec::IntoIter<Vec<f64>>,
    drawn: usize,
}

impl<'a> Sequence<'a> {
    fn new(
        bounds: &'a [(f64, f64)],
        baseline: Option<&'a [f64]>,
        seed: u64,
        projection: Option<PlanformProjection>,
    ) -> Self {
        Self {
            bounds,
            baseline,
            projection,
            rng: SearchRng::stream(seed, 1),
            pending: Vec::new().into_iter(),
            drawn: 0,
        }
    }

    fn next_point(&mut self) -> Vec<f64> {
        if let Some(point) = self.pending.next() {
            return point;
        }
        let mut points = latin_hypercube(self.bounds, BATCH_SIZE, &mut self.rng);
        if let Some(projection) = &self.projection {
            for point in &mut points {
                projection.apply(point);
            }
        }
        if self.drawn == 0 {
            if let Some(baseline) = self
                .baseline
                .filter(|point| point.len() == self.bounds.len())
            {
                points[0] = baseline.to_vec();
                crate::search_methods::clamp_to_bounds(&mut points[0], self.bounds);
            }
        }
        self.drawn += 1;
        self.pending = points.into_iter();
        self.pending.next().unwrap_or_default()
    }
}

/// The first `count` points the screening stage evaluates for `seed`.
pub(crate) fn sample(
    bounds: &[(f64, f64)],
    baseline: Option<&[f64]>,
    count: usize,
    seed: u64,
    projection: Option<&PlanformProjection>,
) -> Vec<Vec<f64>> {
    let mut sequence = Sequence::new(bounds, baseline, seed, projection.copied());
    (0..count).map(|_| sequence.next_point()).collect()
}

/// Evaluate the sample batch by batch until the budget, the rejection cap,
/// the time limit or a cancellation stops it. `started` is the stage clock.
///
/// A batch is refilled past the design-vector pre-gate: points are drawn
/// from the sequence until [`BATCH_SIZE`] of them pass `admit` (or the
/// budget ends), and the rejected ones travel in the same batch without
/// counting against the budget. A batch therefore always carries a full
/// complement of analyses for the workers, and its composition depends only
/// on the seed, the budget, the rejection cap and the pre-gate, never on
/// the worker count. Both limits are exact: drawing stops at the
/// `stop_after`-th admitted point or the `max_rejects`-th rejected one, the
/// latter ending the stage as [`Termination::PregateExhausted`].
pub(crate) fn run(
    bounds: &[(f64, f64)],
    baseline: Option<&[f64]>,
    settings: Settings,
    started: Instant,
    scope: &CancelScope<'_>,
    admit: &dyn Fn(&[f64]) -> bool,
    evaluate: &mut EvaluateBatch<'_>,
) -> Outcome {
    let stop_after = settings.stop_after.min(settings.max_evaluations);
    let mut sequence = Sequence::new(bounds, baseline, settings.seed, settings.projection);
    let mut outcome = Outcome {
        scored: Vec::new(),
        analysed: 0,
        batches: 0,
        termination: Termination::EvaluationBudget,
    };
    let mut rejected = 0;
    let mut last_batch = (Duration::ZERO, 0);
    while outcome.analysed < stop_after {
        if rejected >= settings.max_rejects {
            outcome.termination = Termination::PregateExhausted;
            break;
        }
        let mut room = BATCH_SIZE.min(stop_after - outcome.analysed);
        if let Some(limit) = settings.time_limit {
            let remaining = limit.saturating_sub(started.elapsed());
            room = room.min(fitted_room(remaining, last_batch, settings.workers));
            if room == 0 {
                outcome.termination = Termination::TimeBudget;
                break;
            }
        }
        scope.enter(CancelPhase::ScreeningScanBlock, outcome.batches as u64);
        if scope.requested() {
            scope.work_skipped(format!(
                "screening stopped before batch {}",
                outcome.batches
            ));
            outcome.termination = Termination::Cancelled;
            break;
        }
        let batch_started = Instant::now();
        let mut points = Vec::new();
        let mut admitted = 0;
        while admitted < room && rejected < settings.max_rejects {
            let point = sequence.next_point();
            if admit(&point) {
                admitted += 1;
            } else {
                rejected += 1;
            }
            points.push(point);
        }
        outcome.analysed += admitted;
        let scores = evaluate(&points);
        outcome
            .scored
            .extend(scores.into_iter().map(ScoredPoint::sanitized));
        outcome.batches += 1;
        if scope.requested() {
            outcome.termination = Termination::Cancelled;
            break;
        }
        last_batch = (batch_started.elapsed(), admitted);
    }
    outcome
}

/// Analysed points the next batch may carry so it ends inside `remaining`,
/// projected from the last batch `(wall time, analysed points)`: that batch
/// ran in `ceil(analysed / workers)` waves of one analysis per lane, and the
/// next runs as many whole waves as fit. A full [`BATCH_SIZE`] when no
/// analysed batch has run yet (the first batch is never cut), zero when not
/// one wave fits.
///
/// Only the batch partition depends on the clock: the points evaluated are
/// still a prefix of the sequence, so a replay with the recorded analysed
/// count evaluates the same points.
fn fitted_room(remaining: Duration, (wall, analysed): (Duration, usize), workers: usize) -> usize {
    if remaining.is_zero() {
        return 0;
    }
    if analysed == 0 {
        return BATCH_SIZE;
    }
    let lanes = workers.clamp(1, analysed);
    let waves = analysed.div_ceil(lanes);
    let wave_s = wall.as_secs_f64() / waves as f64;
    if wave_s <= 0.0 {
        return BATCH_SIZE;
    }
    let fitting = (remaining.as_secs_f64() / wave_s).floor();
    if fitting >= BATCH_SIZE as f64 {
        BATCH_SIZE
    } else {
        (fitting as usize * lanes).min(BATCH_SIZE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search_methods::Tier;

    fn bowl(points: &[Vec<f64>]) -> Vec<ScoredPoint> {
        points
            .iter()
            .map(|values| ScoredPoint {
                values: values.clone(),
                cost: values.iter().map(|x| x * x).sum(),
                tier: Tier::Feasible,
                constraint_violation: 0.0,
                objectives: [0.0; 3],
            })
            .collect()
    }

    fn settings(max_evaluations: usize, stop_after: usize) -> Settings {
        Settings {
            max_evaluations,
            stop_after,
            max_rejects: usize::MAX,
            projection: None,
            time_limit: None,
            workers: 1,
            seed: 11,
        }
    }

    #[test]
    fn the_budget_is_respected_and_a_shorter_run_is_a_prefix_of_a_longer_one() {
        let bounds = [(-1.0, 1.0), (0.0, 2.0), (3.0, 3.0)];
        let scope = CancelScope::attach(None);
        let long = run(
            &bounds,
            Some(&[0.1, 0.2, 3.0]),
            settings(150, 150),
            Instant::now(),
            &scope,
            &|_| true,
            &mut bowl,
        );
        assert_eq!(long.scored.len(), 150);
        assert_eq!(long.termination, Termination::EvaluationBudget);
        assert_eq!(long.scored[0].values, vec![0.1, 0.2, 3.0]);
        // A replay stopped at 70 evaluations with the same budget evaluates
        // exactly the first 70 points, which is what makes a time-limited
        // run replayable.
        let replay = run(
            &bounds,
            Some(&[0.1, 0.2, 3.0]),
            settings(150, 70),
            Instant::now(),
            &scope,
            &|_| true,
            &mut bowl,
        );
        assert_eq!(replay.scored.len(), 70);
        assert_eq!(replay.scored[..], long.scored[..70]);
        assert!(long.scored.iter().all(|point| point.values[2] == 3.0));
    }

    #[test]
    fn the_time_limit_stops_only_between_batches() {
        let bounds = [(0.0, 1.0); 2];
        let mut calls = Vec::new();
        let outcome = run(
            &bounds,
            None,
            Settings {
                time_limit: Some(Duration::from_millis(30)),
                ..settings(10_000, 10_000)
            },
            Instant::now(),
            &CancelScope::attach(None),
            &|_| true,
            &mut |points: &[Vec<f64>]| {
                calls.push(points.len());
                std::thread::sleep(Duration::from_millis(20));
                bowl(points)
            },
        );
        assert_eq!(outcome.termination, Termination::TimeBudget);
        // The guard never cuts a batch in flight: every requested point is
        // scored, and the first batch is always whole.
        assert_eq!(calls.first(), Some(&BATCH_SIZE), "{calls:?}");
        assert!(calls.iter().all(|&len| len <= BATCH_SIZE), "{calls:?}");
        assert_eq!(outcome.scored.len(), calls.iter().sum::<usize>());
        assert!(calls.len() <= 3, "{calls:?}");
    }

    /// Screens on `lanes` lanes that each spend `wave` per analysis, until
    /// `limit`: `(wall, outcome, batch sizes)`.
    fn timed_screening(
        lanes: usize,
        wave: Duration,
        limit: Duration,
        stop_after: usize,
    ) -> (Duration, Outcome, Vec<usize>) {
        let mut calls = Vec::new();
        let started = Instant::now();
        let outcome = run(
            &[(0.0, 1.0); 2],
            None,
            Settings {
                time_limit: Some(limit),
                workers: lanes,
                ..settings(10_000, stop_after)
            },
            started,
            &CancelScope::attach(None),
            &|_| true,
            &mut |points: &[Vec<f64>]| {
                calls.push(points.len());
                std::thread::sleep(wave * points.len().div_ceil(lanes) as u32);
                bowl(points)
            },
        );
        (started.elapsed(), outcome, calls)
    }

    #[test]
    fn screening_ends_within_one_batch_time_of_its_limit_on_a_fitted_last_batch() {
        // Four lanes, 20 ms per wave: a full batch is 16 waves, 320 ms. A
        // whole second batch would overrun the 0.5 s limit, so the stage
        // fits a smaller last one instead of stopping at 0.32 s.
        let wave = Duration::from_millis(20);
        let limit = Duration::from_millis(500);
        let (wall, outcome, calls) = timed_screening(4, wave, limit, 10_000);
        assert_eq!(outcome.termination, Termination::TimeBudget);
        assert_eq!(calls[0], BATCH_SIZE);
        let last = *calls.last().expect("a batch ran");
        assert!(calls.len() >= 2 && last < BATCH_SIZE, "{calls:?}");
        assert_eq!(last % 4, 0, "whole waves: {calls:?}");
        let batch_time = wave * (BATCH_SIZE / 4) as u32;
        assert!(
            wall + batch_time > limit,
            "{wall:?} left a batch time unused"
        );
        assert!(limit.saturating_sub(wall) < 3 * wave, "{wall:?}");
        // The clock changes the partition only: replaying the analysed count
        // without a time limit evaluates the same points.
        let replay = run(
            &[(0.0, 1.0); 2],
            None,
            settings(10_000, outcome.analysed),
            Instant::now(),
            &CancelScope::attach(None),
            &|_| true,
            &mut bowl,
        );
        assert_eq!(replay.scored, outcome.scored);
    }

    #[test]
    fn the_fitted_batch_takes_whole_waves_of_the_last_batch() {
        let batch = (Duration::from_millis(160), 64);
        assert_eq!(fitted_room(Duration::from_millis(400), batch, 32), 64);
        assert_eq!(fitted_room(Duration::from_millis(100), batch, 32), 32);
        assert_eq!(fitted_room(Duration::from_millis(79), batch, 32), 0);
        assert_eq!(fitted_room(Duration::from_millis(30), batch, 4), 12);
        assert_eq!(
            fitted_room(Duration::from_millis(5), (Duration::ZERO, 0), 8),
            64
        );
        assert_eq!(fitted_room(Duration::ZERO, (Duration::ZERO, 0), 8), 0);
    }

    #[test]
    fn the_budget_counts_analysed_points_exactly_whatever_the_reject_rate() {
        let bounds = [(0.0, 1.0); 3];
        for reject_rate in [0.0, 0.5, 0.9] {
            let admit = move |point: &[f64]| point[0] >= reject_rate;
            let mut batches: Vec<Vec<Vec<f64>>> = Vec::new();
            let outcome = run(
                &bounds,
                None,
                settings(1_000, 1_000),
                Instant::now(),
                &CancelScope::attach(None),
                &admit,
                &mut |points: &[Vec<f64>]| {
                    batches.push(points.to_vec());
                    bowl(points)
                },
            );
            assert_eq!(outcome.termination, Termination::EvaluationBudget);
            let points: Vec<Vec<f64>> = batches.concat();
            let analysed = points.iter().filter(|point| admit(point)).count();
            assert_eq!(
                (outcome.analysed, analysed),
                (1_000, 1_000),
                "{reject_rate}"
            );
            // Every batch but the last carries a full complement of analyses.
            let (_, full) = batches.split_last().expect("a batch ran");
            for batch in full {
                let admitted = batch.iter().filter(|point| admit(point)).count();
                assert_eq!(admitted, BATCH_SIZE, "reject rate {reject_rate}");
            }
            // The points are the sequence's prefix, ending on the 1000th
            // analysed one; the rejects ride along at about the given rate.
            assert_eq!(points, sample(&bounds, None, points.len(), 11, None));
            assert!(points.last().is_some_and(|point| admit(point)));
            let rejected = (points.len() - analysed) as f64 / points.len() as f64;
            assert!((rejected - reject_rate).abs() < 0.02, "{rejected}");
        }
    }

    #[test]
    fn the_rejection_cap_stops_the_stage_exactly_as_pregate_exhausted() {
        let bounds = [(0.0, 1.0); 3];
        let admit = |point: &[f64]| point[0] >= 0.9;
        let mut drawn: Vec<Vec<f64>> = Vec::new();
        let outcome = run(
            &bounds,
            None,
            Settings {
                max_rejects: 300,
                ..settings(1_000, 1_000)
            },
            Instant::now(),
            &CancelScope::attach(None),
            &admit,
            &mut |points: &[Vec<f64>]| {
                drawn.extend_from_slice(points);
                bowl(points)
            },
        );
        assert_eq!(outcome.termination, Termination::PregateExhausted);
        assert_eq!(drawn.iter().filter(|point| !admit(point)).count(), 300);
        assert!(drawn.last().is_some_and(|point| !admit(point)));
        assert!(outcome.analysed < 1_000);
        assert_eq!(outcome.scored.len(), drawn.len());
    }

    #[test]
    fn a_tolerance_descriptor_only_loosens_the_closure_tolerance() {
        let config = AlasConfig::default();
        let loosened = ScreeningFidelity {
            sizing_tolerance_kg: Some(50.0),
            ..ScreeningFidelity::full()
        };
        let screening = loosened.configure(&config);
        assert_eq!(screening.optimizer.objective.sizing_tolerance_kg, 50.0);
        let mut restored = screening.clone();
        restored.optimizer.objective.sizing_tolerance_kg =
            config.optimizer.objective.sizing_tolerance_kg;
        assert_eq!(restored, config);
        assert_eq!(ScreeningFidelity::full().configure(&config), config);
        assert_eq!(
            ScreeningFidelity::full().controls(),
            SizingControls::default()
        );
    }

    #[test]
    fn a_mesh_descriptor_only_coarsens_and_never_refines() {
        let config = AlasConfig::default();
        let configured = config.analysis.chordwise_resolution;
        let coarse = ScreeningFidelity {
            chordwise_resolution: Some(configured / 2),
            ..ScreeningFidelity::full()
        }
        .configure(&config);
        assert_eq!(coarse.analysis.chordwise_resolution, configured / 2);
        let mut restored = coarse.clone();
        restored.analysis.chordwise_resolution = configured;
        assert_eq!(restored, config);
        let finer = ScreeningFidelity {
            chordwise_resolution: Some(configured * 2),
            ..ScreeningFidelity::full()
        };
        assert_eq!(finer.configure(&config), config);
    }
}
