// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Persist search evidence through the same nonblocking observer used by the GUI.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};

use alas_opt::OptimizationResult;
use alas_pipeline::PipelineResult;

pub(super) struct SnapshotEvidence {
    sender: Option<Sender<OptimizationResult>>,
    published: AtomicBool,
    writer: Option<JoinHandle<()>>,
}

impl SnapshotEvidence {
    pub(super) fn start(directory: Option<&Path>) -> Self {
        let (sender, writer) = if let Some(directory) = directory {
            let path = directory.join("optimization_evidence.json");
            let (sender, receiver) = mpsc::channel::<OptimizationResult>();
            let writer = thread::spawn(move || {
                if let Ok(result) = receiver.recv() {
                    if let Ok(bytes) = serde_json::to_vec_pretty(&result) {
                        let _ = std::fs::write(path, bytes);
                    }
                }
            });
            (Some(sender), Some(writer))
        } else {
            (None, None)
        };
        Self {
            sender,
            published: AtomicBool::new(false),
            writer,
        }
    }

    pub(super) fn publish(&self, mut snapshot: PipelineResult) {
        if let (Some(sender), Some(optimization)) =
            (&self.sender, snapshot.optimization_result.take())
        {
            if !self.published.swap(true, Ordering::Relaxed) {
                let _ = sender.send(optimization);
            }
        }
    }

    pub(super) fn finish(mut self) {
        drop(self.sender.take());
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
    }
}
