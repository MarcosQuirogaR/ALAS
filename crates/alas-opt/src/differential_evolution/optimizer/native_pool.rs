// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native candidate evaluation across `workers` single-threaded lanes.
//!
//! A batch is one work-stealing queue: every lane repeatedly takes the next
//! unclaimed candidate index until the batch is exhausted, and each answer is
//! stored at its own index, so the scores and the history order never depend
//! on which lane ran what or on the lane count. Each lane is a one-thread
//! Rayon pool, so the vortex-lattice assembly's own parallel loops run
//! sequentially inside a candidate instead of competing with the other
//! candidates for the same cores. The coordinator waits outside the lanes,
//! forwarding the caller's borrowed cancellation flag into an owned token the
//! analyses poll. Once the token is set no lane takes another candidate: a
//! request costs at most the candidates already in flight, one per lane, and
//! every candidate never started is recorded as `cancelled_unstarted` so the history
//! still has one row per requested design. The same queue checks the stage
//! deadline before each candidate; a timed stop returns only its started
//! prefix, with no history rows for unstarted designs.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use rayon::{ThreadPool, ThreadPoolBuildError, ThreadPoolBuilder};

use super::{DesignObjective, OptimizationHistory, SearchObjective};
use crate::cancellation::{CancelScope, EvaluationCancellation};

type CandidateAnswer = Option<(f64, bool, OptimizationHistory)>;

/// One lane's report on one candidate: its index, its answer (`None` after a
/// panic), how long it ran and whether it finished after the request.
type LaneReport = (usize, CandidateAnswer, Duration, bool);

pub(super) struct NativeObjective<'a> {
    objective: &'a mut DesignObjective,
    baseline: Arc<DesignObjective>,
    lanes: Arc<Vec<ThreadPool>>,
    parent_cancel: Option<&'a AtomicBool>,
    cancellation: EvaluationCancellation,
    telemetry: Option<Vec<(Duration, bool)>>,
    deadline: Option<Instant>,
}

impl<'a> NativeObjective<'a> {
    pub(super) fn new(
        objective: &'a mut DesignObjective,
        workers: usize,
        parent_cancel: Option<&'a AtomicBool>,
    ) -> Result<Self, ThreadPoolBuildError> {
        let lanes = (0..workers.max(1))
            .map(|lane| {
                ThreadPoolBuilder::new()
                    .num_threads(1)
                    .thread_name(move |_| format!("alas-native-{lane}"))
                    .build()
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self::with_lanes(objective, Arc::new(lanes), parent_cancel))
    }

    /// Another objective evaluated on the same lanes, so two stages never
    /// hold two sets of threads.
    pub(super) fn sharing_lanes(&self, objective: &'a mut DesignObjective) -> NativeObjective<'a> {
        Self::with_lanes(objective, Arc::clone(&self.lanes), self.parent_cancel)
    }

    fn with_lanes(
        objective: &'a mut DesignObjective,
        lanes: Arc<Vec<ThreadPool>>,
        parent_cancel: Option<&'a AtomicBool>,
    ) -> Self {
        // Never copy accumulated trajectory data into candidate baselines.
        let history = std::mem::take(&mut objective.history);
        let mut baseline = objective.clone();
        objective.history = history;
        let cancellation = EvaluationCancellation::new();
        baseline.cancellation = Some(cancellation.clone());
        Self {
            objective,
            baseline: Arc::new(baseline),
            lanes,
            parent_cancel,
            cancellation,
            telemetry: None,
            deadline: None,
        }
    }

    fn forward_cancellation(&self) {
        if CancelScope::attach(self.parent_cancel).requested() && !self.cancellation.requested() {
            self.cancellation.request();
        }
    }

    fn receive<T>(&self, receiver: &mpsc::Receiver<T>) -> Result<T, mpsc::RecvError> {
        loop {
            self.forward_cancellation();
            match receiver.recv_timeout(Duration::from_millis(25)) {
                Ok(answer) => return Ok(answer),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err(mpsc::RecvError),
            }
        }
    }
}

impl SearchObjective for NativeObjective<'_> {
    fn set_deadline(&mut self, deadline: Option<Instant>) {
        self.deadline = deadline;
    }

    fn honors_deadline(&self) -> bool {
        true
    }

    fn evaluate(&mut self, design: &[f64]) -> f64 {
        let failure_cost = self.baseline.config.optimizer.weights.failure_cost;
        self.evaluate_batch(&[design.to_vec()], 1)
            .first()
            .map_or(failure_cost, |score| score.0)
    }

    fn history(&self) -> &OptimizationHistory {
        &self.objective.history
    }

    fn runs_concurrently(&self) -> bool {
        true
    }

    fn take_concurrent_telemetry(&mut self) -> Option<Vec<(Duration, bool)>> {
        self.telemetry.take()
    }

    fn evaluate_batch(&mut self, designs: &[Vec<f64>], _workers: usize) -> Vec<(f64, bool)> {
        self.telemetry = Some(Vec::new());
        if designs.is_empty() {
            return Vec::new();
        }
        self.forward_cancellation();
        let points = Arc::new(designs.to_vec());
        let next = Arc::new(AtomicUsize::new(0));
        let (sender, receiver) = mpsc::channel::<LaneReport>();
        for lane in self.lanes.iter().take(designs.len()) {
            let (points, next, sender) = (Arc::clone(&points), Arc::clone(&next), sender.clone());
            let (baseline, token) = (Arc::clone(&self.baseline), self.cancellation.clone());
            let deadline = self.deadline;
            lane.spawn(move || {
                run_lane(&points, &next, &token, deadline, &sender, |values| {
                    evaluate_independent(&baseline, values)
                });
            });
        }
        drop(sender);
        let mut answers: Vec<Option<CandidateAnswer>> = (0..designs.len()).map(|_| None).collect();
        let mut telemetry = Vec::new();
        for _ in 0..designs.len() {
            match self.receive(&receiver) {
                Ok((index, answer, elapsed, after_request)) => {
                    answers[index] = Some(answer);
                    telemetry.push((elapsed, after_request));
                }
                Err(_) => break,
            }
        }
        self.telemetry = Some(telemetry);
        let started = next.load(Ordering::Relaxed).min(designs.len());
        let count = if self.deadline.is_some() && !self.cancellation.requested() {
            started
        } else {
            designs.len()
        };
        answers
            .into_iter()
            .zip(designs)
            .take(count)
            .map(|(answer, values)| match answer {
                Some(Some((cost, valid, history))) => {
                    self.objective.history.append(history);
                    (cost, valid)
                }
                // A partial failed history stays in the discarded local
                // objective. Retrying on the coordinator could re-panic and
                // would bypass both the lanes and cancellation.
                Some(None) => {
                    record_unanalysed(self.objective, values, "evaluation_worker_failure")
                }
                None => {
                    record_unanalysed(self.objective, values, super::batch::CANCELLED_UNSTARTED)
                }
            })
            .collect()
    }
}

fn run_lane(
    points: &[Vec<f64>],
    next: &AtomicUsize,
    token: &EvaluationCancellation,
    deadline: Option<Instant>,
    sender: &mpsc::Sender<LaneReport>,
    mut evaluate: impl FnMut(&[f64]) -> CandidateAnswer,
) {
    while !token.requested() && deadline.is_none_or(|deadline| Instant::now() < deadline) {
        let index = next.fetch_add(1, Ordering::Relaxed);
        let Some(values) = points.get(index) else {
            break;
        };
        let started = Instant::now();
        let answer = evaluate(values);
        let report = (index, answer, started.elapsed(), token.requested());
        if sender.send(report).is_err() {
            break;
        }
    }
}

fn evaluate_independent(baseline: &DesignObjective, values: &[f64]) -> CandidateAnswer {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut local = baseline.clone();
        let cost = local.evaluate(values);
        let valid = local.history.valid.last().copied().unwrap_or(false) && cost.is_finite();
        (cost, valid, local.history)
    }))
    .ok()
}

/// One history row for a requested design that produced no analysis.
fn record_unanalysed(objective: &mut DesignObjective, values: &[f64], reason: &str) -> (f64, bool) {
    let cost = objective.config.optimizer.weights.failure_cost;
    let design = alas_config::DesignVector::from_array(values).unwrap_or_default();
    objective
        .history
        .record(design, false, cost, 0.0, 0.0, 0.0, 0.0, 0.0, reason);
    (cost, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_expired_deadline_leaves_every_candidate_unanalysed() {
        let mut objective = DesignObjective::new(alas_config::AlasConfig::default());
        let mut pool = NativeObjective::new(&mut objective, 4, None).unwrap();
        pool.set_deadline(Some(Instant::now()));
        assert!(pool.evaluate_batch(&vec![Vec::new(); 64], 4).is_empty());
        assert!(pool.evaluate(&[]).is_finite());
        assert_eq!(pool.history().n_evaluations(), 0);
        assert_eq!(pool.take_concurrent_telemetry(), Some(Vec::new()));
        pool.set_deadline(None);
        assert_eq!(pool.evaluate_batch(&[Vec::new()], 4).len(), 1);
    }

    #[test]
    fn a_deadline_stops_the_queue_after_at_most_one_in_flight_candidate_per_lane() {
        let points = vec![Vec::new(); 64];
        let next = AtomicUsize::new(0);
        let token = EvaluationCancellation::new();
        let (sender, receiver) = mpsc::channel();
        let deadline = Instant::now() + Duration::from_millis(10);
        std::thread::scope(|scope| {
            for _ in 0..4 {
                let sender = sender.clone();
                let (points, next, token) = (&points, &next, &token);
                scope.spawn(move || {
                    run_lane(points, next, token, Some(deadline), &sender, |_| {
                        std::thread::sleep(Duration::from_millis(30));
                        None
                    });
                });
            }
        });
        drop(sender);
        let mut indices: Vec<usize> = receiver.into_iter().map(|report| report.0).collect();
        indices.sort_unstable();
        assert!(indices.len() <= 4, "{indices:?}");
        assert_eq!(
            indices,
            (0..next.load(Ordering::Relaxed)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_worker_failure_records_an_invalid_analysis_without_retrying_it() {
        let mut objective = DesignObjective::new(alas_config::AlasConfig::default());
        let values = alas_config::DesignVector::default().to_array();
        let (_, valid) = record_unanalysed(&mut objective, &values, "evaluation_worker_failure");
        assert!(!valid);
        assert_eq!(objective.history.n_evaluations(), 1);
        assert_eq!(
            objective.history.reject_reason[0],
            "evaluation_worker_failure"
        );
    }

    #[test]
    fn lanes_keep_history_order_across_batches_at_any_lane_count() {
        for workers in [1, 3, 16] {
            let mut objective = DesignObjective::new(alas_config::AlasConfig::default());
            let mut pool = NativeObjective::new(&mut objective, workers, None).unwrap();
            for size in [1, 5, 2, 7] {
                let scores = pool.evaluate_batch(&vec![Vec::new(); size], workers);
                assert_eq!(scores.len(), size);
                assert!(scores
                    .iter()
                    .all(|(cost, valid)| cost.is_finite() && !valid));
            }
            assert_eq!(pool.history().n_evaluations(), 15);
            assert!(pool
                .history()
                .reject_reason
                .iter()
                .all(|reason| reason == "design_space"));
        }
    }

    #[test]
    fn after_a_request_no_lane_starts_a_candidate_and_none_counts_as_an_analysis() {
        let parent = AtomicBool::new(true);
        let mut objective = DesignObjective::new(alas_config::AlasConfig::default());
        let mut pool = NativeObjective::new(&mut objective, 4, Some(&parent)).unwrap();
        let scores = pool.evaluate_batch(&vec![Vec::new(); 6], 4);
        assert_eq!(scores.len(), 6);
        assert!(scores.iter().all(|(_, valid)| !valid));
        assert_eq!(pool.take_concurrent_telemetry(), Some(Vec::new()));
        assert_eq!(pool.history().n_evaluations(), 6);
        assert!(pool
            .history()
            .reject_reason
            .iter()
            .all(|reason| reason == super::super::batch::CANCELLED_UNSTARTED));
        // Through the stage evaluator: one history row per requested design,
        // none of them an analysis.
        let bounds = vec![(0.0, 1.0); alas_config::DesignVector::bounds().len()];
        let designs = vec![vec![0.5; bounds.len()], vec![0.25; bounds.len()]];
        let mut evaluator =
            super::super::batch::BatchEvaluator::new(&mut pool, 4, &bounds, None, Some(&parent));
        let before = evaluator.analyses();
        evaluator.evaluate_block(&designs);
        assert_eq!(evaluator.objective.history().n_evaluations(), 8);
        assert_eq!(evaluator.analyses(), before);
        assert_eq!(evaluator.cancelled_unstarted, 2);
    }

    #[test]
    fn nested_parallel_work_inside_a_lane_runs_on_that_lane_alone() {
        let mut objective = DesignObjective::new(alas_config::AlasConfig::default());
        let pool = NativeObjective::new(&mut objective, 3, None).unwrap();
        for lane in pool.lanes.iter() {
            let threads = lane.install(|| {
                use rayon::prelude::*;
                (0..64)
                    .into_par_iter()
                    .map(|_| rayon::current_num_threads())
                    .max()
            });
            assert_eq!(threads, Some(1));
        }
    }

    #[test]
    fn a_borrowed_cancellation_flag_reaches_a_running_lane() {
        use std::time::Instant;

        let parent = AtomicBool::new(false);
        let mut objective = DesignObjective::new(alas_config::AlasConfig::default());
        let pool = NativeObjective::new(&mut objective, 1, Some(&parent)).unwrap();
        let token = pool.cancellation.clone();
        let (started_sender, started_receiver) = mpsc::channel();
        let (sender, receiver) = mpsc::channel();
        pool.lanes[0].spawn(move || {
            let _ = started_sender.send(());
            let deadline = Instant::now() + Duration::from_secs(3);
            while !token.requested() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(1));
            }
            let _ = sender.send(token.requested());
        });
        std::thread::scope(|scope| {
            let parent = &parent;
            scope.spawn(move || {
                let _ = started_receiver.recv();
                parent.store(true, Ordering::Release);
            });
            assert_eq!(pool.receive(&receiver).ok(), Some(true));
        });
    }
}
