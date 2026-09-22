// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Quick Analysis results and the worker that produces them.
//!
//! One background thread runs the reduced pipeline model; every event it
//! streams carries the configuration revision it was requested for. Events
//! for any other revision are dropped, so a late result can never fill the
//! panel for newer geometry. A model-affecting edit marks the shown results
//! stale immediately, asks the running job to stop at its next boundary and
//! closes its channel, so whatever that job still publishes for the old
//! revision is rejected even when no replacement job is started.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
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
        let (sender, cancel) = self.begin(revision);
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

    /// Reset the panel for a new job on `revision` and open its channel.
    /// The returned sender is the only way results reach this panel; the
    /// flag asks the job to stop at its next boundary.
    fn begin(&mut self, revision: u64) -> (Sender<WorkerMessage>, Arc<AtomicBool>) {
        self.detach();
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
        (sender, cancel)
    }

    /// Cut the running job off: ask it to stop and close its channel, so
    /// nothing it still publishes for its revision can reach the panel.
    fn detach(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.receiver = None;
        self.running = false;
    }

    /// Terminate every metric still pending with a failure message.
    fn terminate_pending(&mut self, message: &str) {
        for (_, state) in &mut self.states {
            if matches!(state, MetricState::Running) {
                *state = MetricState::Done(QuickOutcome::Failed(message.to_owned()));
            }
        }
    }

    /// Mark the shown results as belonging to an older model and reject
    /// everything the running job may still publish for that revision.
    /// Results already shown stay, labelled stale; metrics still pending
    /// terminate now, so the panel is neither left Running nor filled
    /// later by a job that raced the edit.
    pub fn invalidate(&mut self) {
        if self.computed_revision.is_some() {
            self.stale = true;
        }
        if self.running {
            self.detach();
            self.terminate_pending("cancelled by a model edit before this estimate was reached");
        }
    }

    /// Cancel any job and forget every result.
    pub fn abandon(&mut self) {
        self.detach();
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
            self.terminate_pending(message);
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
    use alas_pipeline::quick_analysis::QuickValue;
    use std::time::Duration;

    fn wait_until_idle(estimates: &mut QuickEstimates) {
        let deadline = Instant::now() + Duration::from_secs(120);
        while estimates.running() && Instant::now() < deadline {
            estimates.poll();
            thread::sleep(Duration::from_millis(20));
        }
        assert!(!estimates.running(), "quick analysis did not finish");
    }

    fn value_event(revision: u64, metric: QuickMetric, achieved: f64) -> WorkerMessage {
        WorkerMessage::Event(QuickEvent {
            revision,
            metric,
            outcome: QuickOutcome::Value(QuickValue {
                achieved,
                requested: None,
                unit: "kg".to_owned(),
                note: String::new(),
            }),
            elapsed_ms: 12,
        })
    }

    fn failed_event(revision: u64, metric: QuickMetric, message: &str) -> WorkerMessage {
        WorkerMessage::Event(QuickEvent {
            revision,
            metric,
            outcome: QuickOutcome::Failed(message.to_owned()),
            elapsed_ms: 20,
        })
    }

    fn achieved(estimates: &QuickEstimates, metric: QuickMetric) -> Option<f64> {
        match estimates.state(metric) {
            MetricState::Done(QuickOutcome::Value(value)) => Some(value.achieved),
            _ => None,
        }
    }

    fn failure(estimates: &QuickEstimates, metric: QuickMetric) -> Option<&str> {
        match estimates.state(metric) {
            MetricState::Done(QuickOutcome::Failed(message)) => Some(message.as_str()),
            _ => None,
        }
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
    fn current_revision_events_are_admitted_until_the_job_reports_finished() {
        let mut estimates = QuickEstimates::default();
        let (sender, cancel) = estimates.begin(4);
        assert!(estimates.running());
        assert!(!cancel.load(Ordering::Relaxed));
        sender
            .send(value_event(4, QuickMetric::TakeoffMass, 70_000.0))
            .unwrap();
        sender
            .send(failed_event(4, QuickMetric::Range, "no route"))
            .unwrap();
        assert!(estimates.poll());
        assert_eq!(
            achieved(&estimates, QuickMetric::TakeoffMass),
            Some(70_000.0)
        );
        assert_eq!(failure(&estimates, QuickMetric::Range), Some("no route"));
        assert_eq!(estimates.first_result_ms, Some(12));
        assert!(estimates.running(), "still running until Finished arrives");
        assert_eq!(
            estimates.state(QuickMetric::FuelCapacity),
            &MetricState::Running
        );
        // Nothing to drain: no change reported.
        assert!(!estimates.poll());
        sender
            .send(WorkerMessage::Finished(QuickAnalysisSummary {
                published: 2,
                initial_stage_ms: 12,
                final_ms: 30,
                ..QuickAnalysisSummary::default()
            }))
            .unwrap();
        assert!(estimates.poll());
        assert!(!estimates.running());
        assert!(!estimates.stale);
        assert_eq!(estimates.computed_revision, Some(4));
        assert_eq!(estimates.summary.as_ref().map(|s| s.final_ms), Some(30));
        // Metrics the job never reached terminate honestly; shown ones stay.
        assert_eq!(
            failure(&estimates, QuickMetric::FuelCapacity),
            Some("the reduced model ended without producing this estimate")
        );
        assert_eq!(
            achieved(&estimates, QuickMetric::TakeoffMass),
            Some(70_000.0)
        );
    }

    #[test]
    fn an_edit_without_a_replacement_run_rejects_every_later_event() {
        let mut estimates = QuickEstimates::default();
        let (sender, cancel) = estimates.begin(1);
        sender
            .send(value_event(1, QuickMetric::TakeoffMass, 70_000.0))
            .unwrap();
        assert!(estimates.poll());
        // The model changes; no second Quick Analysis is requested.
        estimates.invalidate();
        assert!(estimates.stale);
        assert!(!estimates.running(), "the old job no longer owns the panel");
        assert!(
            cancel.load(Ordering::Relaxed),
            "the old job is asked to stop"
        );
        let cancelled = "cancelled by a model edit before this estimate was reached";
        assert_eq!(
            failure(&estimates, QuickMetric::FuelCapacity),
            Some(cancelled)
        );
        assert_eq!(
            achieved(&estimates, QuickMetric::TakeoffMass),
            Some(70_000.0)
        );
        assert_eq!(estimates.computed_revision, Some(1));
        // The job raced the edit and still publishes for revision 1: its
        // channel is closed, so nothing reaches the panel.
        assert!(sender
            .send(value_event(1, QuickMetric::FuelCapacity, 20_000.0))
            .is_err());
        assert!(sender
            .send(WorkerMessage::Finished(QuickAnalysisSummary {
                published: 15,
                final_ms: 900,
                ..QuickAnalysisSummary::default()
            }))
            .is_err());
        assert!(!estimates.poll());
        assert_eq!(
            failure(&estimates, QuickMetric::FuelCapacity),
            Some(cancelled)
        );
        assert!(estimates.summary.is_none());
        assert!(!estimates.running());
        // A later edit with nothing in flight only keeps the stale label.
        estimates.invalidate();
        assert!(estimates.stale);
        assert!(estimates.has_results());
    }

    #[test]
    fn a_replacement_run_only_admits_its_own_revision() {
        let mut estimates = QuickEstimates::default();
        let (old_sender, old_cancel) = estimates.begin(1);
        old_sender
            .send(value_event(1, QuickMetric::TakeoffMass, 70_000.0))
            .unwrap();
        assert!(estimates.poll());
        estimates.invalidate();
        let (sender, cancel) = estimates.begin(2);
        assert!(old_cancel.load(Ordering::Relaxed));
        assert!(!cancel.load(Ordering::Relaxed));
        assert!(!estimates.stale);
        assert!(estimates.running());
        assert_eq!(estimates.computed_revision, Some(2));
        assert_eq!(
            estimates.state(QuickMetric::TakeoffMass),
            &MetricState::Running
        );
        // The old job's channel is closed.
        assert!(old_sender
            .send(value_event(1, QuickMetric::TakeoffMass, 71_000.0))
            .is_err());
        // Even a message carrying the old revision on the live channel is dropped.
        sender
            .send(value_event(1, QuickMetric::TakeoffMass, 71_000.0))
            .unwrap();
        assert!(!estimates.poll());
        assert_eq!(
            estimates.state(QuickMetric::TakeoffMass),
            &MetricState::Running
        );
        assert_eq!(estimates.first_result_ms, None);
        sender
            .send(value_event(2, QuickMetric::TakeoffMass, 72_000.0))
            .unwrap();
        assert!(estimates.poll());
        assert_eq!(
            achieved(&estimates, QuickMetric::TakeoffMass),
            Some(72_000.0)
        );
        assert!(!estimates.stale);
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
