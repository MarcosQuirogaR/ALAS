// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Starting, polling and cancelling the competition search.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, TryRecvError};
use std::sync::Arc;
use std::thread;

use alas_uav::optimizer::{OptimizationError, OptimizationProgress};

use super::model::{CompetitionResult, UavExecutionStatus, UavSection, UavWorkflowOutcome};
use super::request::{run_worker, UavWorkerMessage};
use super::UavWorkflowState;

impl UavWorkflowState {
    /// Snapshot the inputs and start one deterministic competition search in a
    /// background worker.
    ///
    /// Returns `false` when a worker is already active or the inputs are
    /// invalid. Invalid input is exposed through [`Self::execution`].
    pub fn start(&mut self) -> bool {
        if self.is_running() {
            return false;
        }
        let request = match self.competition_request() {
            Ok(request) => request,
            Err(message) => {
                self.execution = UavExecutionStatus::Failed(message);
                return false;
            }
        };
        let total_candidates = request.evaluations;
        self.cancel_flag = Arc::new(AtomicBool::new(false));
        self.execution = UavExecutionStatus::Running(OptimizationProgress {
            evaluated_candidates: 0,
            total_candidates,
            verified_candidates: 0,
            best_score: None,
        });
        let (sender, receiver) = channel();
        self.worker_rx = Some(receiver);
        let cancel_flag = self.cancel_flag.clone();
        match thread::Builder::new()
            .name("alas-uav-competition".to_owned())
            .spawn(move || run_worker(request, sender, cancel_flag))
        {
            Ok(_) => true,
            Err(error) => {
                self.worker_rx = None;
                self.execution = UavExecutionStatus::Failed(format!(
                    "Could not start the UAV optimization worker: {error}"
                ));
                false
            }
        }
    }

    /// Request cooperative cancellation at the next candidate boundary.
    pub fn cancel(&mut self) {
        let UavExecutionStatus::Running(progress) = self.execution else {
            return;
        };
        self.cancel_flag.store(true, Ordering::Relaxed);
        self.execution = UavExecutionStatus::CancelRequested(progress);
    }

    /// Drain worker messages without blocking the egui frame.
    pub fn poll(&mut self) {
        let mut messages = Vec::new();
        let mut disconnected = false;
        if let Some(receiver) = &self.worker_rx {
            loop {
                match receiver.try_recv() {
                    Ok(message) => messages.push(message),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        for message in messages {
            match message {
                UavWorkerMessage::Progress(progress) => {
                    self.execution = match self.execution {
                        UavExecutionStatus::CancelRequested(_) => {
                            UavExecutionStatus::CancelRequested(progress)
                        }
                        _ => UavExecutionStatus::Running(progress),
                    };
                }
                UavWorkerMessage::Finished(finished) => {
                    self.worker_rx = None;
                    match finished.result {
                        Ok(optimum) => {
                            let preset_name = alas_uav::score::presets::load(&finished.preset_id)
                                .map_or_else(|_| finished.preset_id.clone(), |preset| preset.name);
                            self.outcome =
                                UavWorkflowOutcome::Competition(Box::new(CompetitionResult {
                                    preset_id: finished.preset_id,
                                    preset_name,
                                    optimum,
                                }));
                            self.active_section = UavSection::Results;
                            self.execution = UavExecutionStatus::Completed;
                        }
                        Err(OptimizationError::Cancelled {
                            evaluated_candidates,
                        }) => {
                            self.execution = UavExecutionStatus::Cancelled {
                                evaluated_candidates,
                            };
                        }
                        Err(error) => {
                            self.execution = UavExecutionStatus::Failed(error.to_string());
                        }
                    }
                }
            }
        }
        if disconnected && self.is_running() {
            self.worker_rx = None;
            self.execution = UavExecutionStatus::Failed(
                "The UAV optimization worker stopped without returning a result.".to_owned(),
            );
        }
    }

    /// Whether a worker is active or cancellation is pending.
    pub fn is_running(&self) -> bool {
        matches!(
            self.execution,
            UavExecutionStatus::Running(_) | UavExecutionStatus::CancelRequested(_)
        )
    }
}
