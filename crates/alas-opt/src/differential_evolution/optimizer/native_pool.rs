// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! One bounded compute pool for independent native candidates and nested VLM.
//! The coordinator waits outside Rayon, forwarding borrowed cancellation into
//! an owned worker token. Results and history remain in candidate order.

use std::sync::atomic::AtomicBool;
use std::sync::{mpsc, Arc};
use std::time::Duration;

use rayon::prelude::*;
use rayon::{ThreadPool, ThreadPoolBuildError, ThreadPoolBuilder};

use super::{DesignObjective, OptimizationHistory, SearchObjective};
use crate::cancellation::{CancelScope, EvaluationCancellation};

type CandidateAnswer = Option<(f64, bool, OptimizationHistory)>;

pub(super) struct NativeObjective<'a> {
    objective: &'a mut DesignObjective,
    baseline: DesignObjective,
    pool: Arc<ThreadPool>,
    parent_cancel: Option<&'a AtomicBool>,
    cancellation: EvaluationCancellation,
}

impl<'a> NativeObjective<'a> {
    pub(super) fn new(
        objective: &'a mut DesignObjective,
        workers: usize,
        parent_cancel: Option<&'a AtomicBool>,
    ) -> Result<Self, ThreadPoolBuildError> {
        let pool = ThreadPoolBuilder::new()
            .num_threads(workers.max(1))
            .thread_name(|index| format!("alas-native-{index}"))
            .build()?;
        Ok(Self::with_pool(objective, Arc::new(pool), parent_cancel))
    }

    pub(super) fn with_pool(
        objective: &'a mut DesignObjective,
        pool: Arc<ThreadPool>,
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
            baseline,
            pool,
            parent_cancel,
            cancellation,
        }
    }

    /// Screening and full-fidelity candidates use the same total CPU budget.
    pub(super) fn shared_pool(&self) -> Arc<ThreadPool> {
        Arc::clone(&self.pool)
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
    fn evaluate(&mut self, design: &[f64]) -> f64 {
        self.evaluate_batch(&[design.to_vec()], 1)[0].0
    }

    fn history(&self) -> &OptimizationHistory {
        &self.objective.history
    }

    fn evaluate_batch(&mut self, designs: &[Vec<f64>], _workers: usize) -> Vec<(f64, bool)> {
        if designs.is_empty() {
            return Vec::new();
        }
        self.forward_cancellation();
        let baseline = self.baseline.clone();
        let points = designs.to_vec();
        let (sender, receiver) = mpsc::channel();
        // Even a singleton with one worker runs inside this pool, so nested
        // Rayon work cannot escape to the global pool and the coordinator can
        // continue forwarding the GUI's borrowed cancellation flag.
        self.pool.spawn(move || {
            let answer = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                points
                    .par_iter()
                    .map(|values| evaluate_independent(&baseline, values))
                    .collect::<Vec<_>>()
            }))
            .ok();
            let _ = sender.send(answer);
        });
        let results = self
            .receive(&receiver)
            .ok()
            .flatten()
            .unwrap_or_else(|| (0..designs.len()).map(|_| None).collect());
        results
            .into_iter()
            .zip(designs)
            .map(|(result, values)| {
                if let Some((cost, valid, history)) = result {
                    self.objective.history.append(history);
                    (cost, valid)
                } else {
                    // A partial failed history stays in the discarded local
                    // objective. Retrying on the coordinator could re-panic and
                    // would bypass both the compute budget and cancellation.
                    record_worker_failure(self.objective, values)
                }
            })
            .collect()
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

fn record_worker_failure(objective: &mut DesignObjective, values: &[f64]) -> (f64, bool) {
    let cost = objective.config.optimizer.weights.failure_cost;
    let design = alas_config::DesignVector::from_array(values).unwrap_or_default();
    objective.history.record(
        design,
        false,
        cost,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        "evaluation_worker_failure",
    );
    (cost, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_worker_failure_records_an_invalid_analysis_without_retrying_it() {
        let mut objective = DesignObjective::new(alas_config::AlasConfig::default());
        let values = alas_config::DesignVector::default().to_array();
        let (_, valid) = record_worker_failure(&mut objective, &values);
        assert!(!valid);
        assert_eq!(objective.history.n_evaluations(), 1);
        assert_eq!(
            objective.history.reject_reason[0],
            "evaluation_worker_failure"
        );
    }

    #[test]
    fn persistent_workers_keep_history_order_across_batches() {
        for workers in [1, 3] {
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
    fn screening_reuses_the_same_pool_and_nested_work_obeys_its_budget() {
        for workers in [1, 3] {
            let mut full = DesignObjective::new(alas_config::AlasConfig::default());
            let mut screening = full.clone();
            let pool = NativeObjective::new(&mut full, workers, None).unwrap();
            let scan = NativeObjective::with_pool(&mut screening, pool.shared_pool(), None);
            assert!(Arc::ptr_eq(&pool.pool, &scan.pool));
            let nested = scan.pool.install(|| {
                (0..12)
                    .into_par_iter()
                    .map(|_| (rayon::current_num_threads(), rayon::current_thread_index()))
                    .collect::<Vec<_>>()
            });
            assert!(nested
                .iter()
                .all(|&(size, index)| size == workers && index.is_some()));
        }
    }

    #[test]
    fn a_borrowed_cancellation_flag_reaches_a_running_single_worker() {
        use std::sync::atomic::Ordering;
        use std::time::Instant;

        let parent = AtomicBool::new(false);
        let mut objective = DesignObjective::new(alas_config::AlasConfig::default());
        let pool = NativeObjective::new(&mut objective, 1, Some(&parent)).unwrap();
        let token = pool.cancellation.clone();
        let (started_sender, started_receiver) = mpsc::channel();
        let (sender, receiver) = mpsc::channel();
        pool.pool.spawn(move || {
            started_sender.send(()).unwrap();
            let deadline = Instant::now() + Duration::from_secs(3);
            while !token.requested() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(1));
            }
            sender
                .send((token.requested(), rayon::current_num_threads()))
                .unwrap();
        });
        std::thread::scope(|scope| {
            let parent = &parent;
            scope.spawn(move || {
                started_receiver.recv().unwrap();
                parent.store(true, Ordering::Release);
            });
            assert_eq!(pool.receive(&receiver).unwrap(), (true, 1));
        });
    }
}
