// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Quick Analysis results and the worker that produces them.
//!
//! One background thread runs the reduced pipeline model; every event it
//! streams carries the configuration revision it was requested for. Events
//! for any other revision are dropped, so a late result can never fill the
//! panel for newer geometry. A model-affecting edit marks the shown results
//! stale immediately and asks the running job to stop at its next boundary.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use alas_config::{AlasConfig, DesignVector};
use alas_pipeline::quick_analysis::{
    run_quick_analysis, QuickAnalysisRequest, QuickAnalysisSummary, QuickEvent, QuickMetric,
    QuickOutcome,
};

/// The state of one metric in the estimates panel.
#[derive(Debug, Clone, PartialEq)]
pub enum MetricState {
    /// Never requested for the shown revision.
    Idle,
    /// Requested, result pending.
    Running,
    /// Terminated.
    Done(QuickOutcome),
}

enum WorkerMessage {
    Event(QuickEvent),
    Finished(QuickAnalysisSummary),
}

/// Quick Analysis results for one configuration revision.
pub struct QuickEstimates {
    states: Vec<(QuickMetric, MetricState)>,
    /// The revision the shown results were computed for.
    pub computed_revision: Option<u64>,
    /// Whether the shown results are older than the current model.
    pub stale: bool,
    running: bool,
    receiver: Option<Receiver<WorkerMessage>>,
    cancel: Option<Arc<AtomicBool>>,
    /// When the shown run started.
    pub started: Option<Instant>,
    /// The completed run's timing summary.
    pub summary: Option<QuickAnalysisSummary>,
    /// Milliseconds at which the first result of the shown run arrived.
    pub first_result_ms: Option<u64>,
}

impl Default for QuickEstimates {
    fn default() -> Self {
        Self {
            states: QuickMetric::ALL
                .iter()
                .map(|metric| (*metric, MetricState::Idle))
                .collect(),
            computed_revision: None,
            stale: false,
            running: false,
            receiver: None,
            cancel: None,
            started: None,
            summary: None,
            first_result_ms: None,
        }
    }
}

impl QuickEstimates {
    /// Whether a job is in flight.
    pub fn running(&self) -> bool {
        self.running
    }

    /// The state of one metric.
    pub fn state(&self, metric: QuickMetric) -> &MetricState {
        self.states
            .iter()
            .find(|(m, _)| *m == metric)
            .map(|(_, state)| state)
            .unwrap_or(&MetricState::Idle)
    }

    /// Every metric with its state, in display order.
    pub fn states(&self) -> &[(QuickMetric, MetricState)] {
        &self.states
    }

    /// Whether any result is shown.
    pub fn has_results(&self) -> bool {
        self.states
            .iter()
            .any(|(_, state)| matches!(state, MetricState::Done(_)))
    }

    fn set(&mut self, metric: QuickMetric, state: MetricState) {
        if let Some(slot) = self.states.iter_mut().find(|(m, _)| *m == metric) {
            slot.1 = state;
        }
    }

    /// Start a job for `revision`, cancelling any earlier one.
    pub fn start(&mut self, config: AlasConfig, design: DesignVector, revision: u64) {
        self.request_cancel();
        for (_, state) in &mut self.states {
            *state = MetricState::Running;
        }
        self.computed_revision = Some(revision);
        self.stale = false;
        self.running = true;
        self.started = Some(Instant::now());
        self.summary = None;
        self.first_result_ms = None;
        let cancel = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = channel();
        self.receiver = Some(receiver);
        self.cancel = Some(cancel.clone());
        let request = QuickAnalysisRequest {
            config,
            design,
            revision,
        };
        thread::spawn(move || {
            let event_sender = sender.clone();
            let mut sink = move |event: QuickEvent| {
                let _ = event_sender.send(WorkerMessage::Event(event));
            };
            let summary = run_quick_analysis(&request, &mut sink, &cancel);
            let _ = sender.send(WorkerMessage::Finished(summary));
        });
    }

    fn request_cancel(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Mark the shown results as belonging to an older model. Pending
    /// results of the running job are still accepted for their own
    /// revision; the panel labels them stale.
    pub fn invalidate(&mut self) {
        if self.computed_revision.is_some() {
            self.stale = true;
        }
        self.request_cancel();
    }

    /// Cancel any job and forget every result.
    pub fn abandon(&mut self) {
        self.request_cancel();
        *self = Self::default();
    }

    /// Drain worker messages; returns whether anything changed.
    pub fn poll(&mut self) -> bool {
        let mut pending = Vec::new();
        let mut finished = None;
        if let Some(receiver) = &self.receiver {
            loop {
                match receiver.try_recv() {
                    Ok(WorkerMessage::Event(event)) => pending.push(event),
                    Ok(WorkerMessage::Finished(summary)) => {
                        finished = Some(summary);
                        break;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        finished = Some(QuickAnalysisSummary {
                            cancelled: true,
                            ..QuickAnalysisSummary::default()
                        });
                        break;
                    }
                }
            }
        } else {
            return false;
        }
        let mut changed = false;
        for event in pending {
            if Some(event.revision) != self.computed_revision {
                // A late result for a superseded revision.
                continue;
            }
            if self.first_result_ms.is_none() {
                self.first_result_ms = Some(event.elapsed_ms);
            }
            self.set(event.metric, MetricState::Done(event.outcome));
            changed = true;
        }
        if let Some(summary) = finished {
            self.receiver = None;
            self.cancel = None;
            self.running = false;
            let message = if summary.cancelled {
                "cancelled before this estimate was reached"
            } else {
                "the reduced model ended without producing this estimate"
            };
            for (_, state) in &mut self.states {
                if matches!(state, MetricState::Running) {
                    *state = MetricState::Done(QuickOutcome::Failed(message.to_owned()));
                }
            }
            self.summary = Some(summary);
            changed = true;
        }
        changed
    }
}

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn wait_until_idle(estimates: &mut QuickEstimates) {
        let deadline = Instant::now() + Duration::from_secs(120);
        while estimates.running() && Instant::now() < deadline {
            estimates.poll();
            thread::sleep(Duration::from_millis(20));
        }
        assert!(!estimates.running(), "quick analysis did not finish");
    }

    #[test]
    fn a_completed_job_terminates_every_metric_for_its_revision() {
        let mut estimates = QuickEstimates::default();
        estimates.start(AlasConfig::default(), DesignVector::default(), 5);
        wait_until_idle(&mut estimates);
        assert_eq!(estimates.computed_revision, Some(5));
        assert!(!estimates.stale);
        for (metric, state) in estimates.states() {
            assert!(
                matches!(state, MetricState::Done(_)),
                "{metric:?} still {state:?}"
            );
        }
        assert!(estimates.first_result_ms.is_some());
    }

    #[test]
    fn a_superseded_job_never_repopulates_the_newer_revision() {
        let mut estimates = QuickEstimates::default();
        estimates.start(AlasConfig::default(), DesignVector::default(), 1);
        // The model changes before the first job reports: its results must
        // be dropped and the second job's results kept.
        estimates.invalidate();
        assert!(estimates.stale);
        estimates.start(AlasConfig::default(), DesignVector::default(), 2);
        wait_until_idle(&mut estimates);
        assert_eq!(estimates.computed_revision, Some(2));
        assert!(!estimates.stale);
        assert!(estimates.has_results());
    }

    #[test]
    fn invalidation_marks_results_stale_without_discarding_them() {
        let mut estimates = QuickEstimates::default();
        estimates.start(AlasConfig::default(), DesignVector::default(), 3);
        wait_until_idle(&mut estimates);
        estimates.invalidate();
        assert!(estimates.stale);
        assert!(estimates.has_results());
        estimates.abandon();
        assert!(!estimates.has_results());
        assert_eq!(estimates.computed_revision, None);
    }
}
