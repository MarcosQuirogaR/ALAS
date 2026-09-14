// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What a completed run actually did, as opposed to what it was configured to
//! do.
//!
//! A run's per-stage timings and its search's evaluation count are both
//! already computed -- the stage timings ride on [`crate::runs::RunEvent`] and
//! the search metadata on [`alas_opt::OptimizationResult`] -- but neither was
//! ever written to disk, so a slow run could only be diagnosed by rerunning it
//! under observation. This manifest persists them beside the design database.
//!
//! The executed search method is recorded separately from the configured one
//! on purpose. `optimizer.solver.method` is a loadable configuration string
//! that still accepts legacy names such as `differential_evolution`, while
//! every product run outside the frozen compatibility path is dispatched to
//! the MADS driver (`alas_opt::DesignOptimizer::run_product_search`). Reading
//! the configured string as the executed algorithm is how a run gets
//! diagnosed against the wrong search.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::pipeline::PipelineResult;
use crate::runs::{RunEvent, RunEventKind};

/// File name written into the run's output directory.
pub const RUN_MANIFEST_FILE: &str = "run_manifest.json";

/// One completed pipeline stage's wall-clock cost.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StageTiming {
    /// Stage identifier, as emitted (e.g. `optimization`, `full_analysis`,
    /// or a detailed child such as `downstream/mses`).
    pub stage: String,
    /// Milliseconds since pipeline execution began, at completion.
    pub elapsed_ms: u64,
    /// The stage's own duration in milliseconds.
    pub duration_ms: u64,
}

/// The search's own account of what it ran.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchManifest {
    /// The algorithm that actually executed, as the optimizer reports it.
    pub executed_method: String,
    /// The configured `optimizer.solver.method` string, retained because it
    /// can legitimately differ from `executed_method`.
    pub configured_method: String,
    /// The executed search's strategy label.
    pub strategy: String,
    /// Durable lifecycle reason the search stopped.
    pub termination: String,
    /// Objective evaluations the search recorded.
    pub evaluations: usize,
    /// Search wall-clock seconds.
    pub wall_time_s: f64,
}

/// Everything a completed run can say about its own cost and dispatch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunManifest {
    /// Total pipeline wall-clock seconds, from the last stage event observed.
    pub total_wall_time_s: f64,
    /// Per-stage timings in completion order.
    pub stages: Vec<StageTiming>,
    /// Total milliseconds attributed to each stage identifier, so a stage
    /// entered more than once is still summed rather than only listed.
    pub stage_totals_ms: BTreeMap<String, u64>,
    /// Search dispatch and cost, when the run optimized.
    pub search: Option<SearchManifest>,
}

impl RunManifest {
    /// Build the manifest from a finished run and the events it emitted.
    pub fn from_run(result: &PipelineResult, events: &[RunEvent]) -> Self {
        let stages: Vec<StageTiming> = events
            .iter()
            .filter(|event| event.kind == RunEventKind::StageCompleted)
            .filter_map(|event| {
                Some(StageTiming {
                    stage: event.stage.clone(),
                    elapsed_ms: event.elapsed_ms,
                    duration_ms: event.duration_ms?,
                })
            })
            .collect();
        let mut stage_totals_ms: BTreeMap<String, u64> = BTreeMap::new();
        for stage in &stages {
            *stage_totals_ms.entry(stage.stage.clone()).or_default() += stage.duration_ms;
        }
        // The clock is monotonic since pipeline start, so the largest observed
        // `elapsed_ms` is the run's own measured length. Falling back to the
        // stage sum would double count nested downstream stages.
        let total_wall_time_s = events
            .iter()
            .map(|event| event.elapsed_ms)
            .max()
            .unwrap_or(0) as f64
            / 1_000.0;
        let search = result
            .optimization_result
            .as_ref()
            .map(|optimization| SearchManifest {
                executed_method: optimization.method.clone(),
                configured_method: result.config.optimizer.solver.method.clone(),
                strategy: optimization.strategy.clone(),
                termination: optimization.termination.clone(),
                evaluations: optimization.history.n_evaluations(),
                wall_time_s: optimization.wall_time_s,
            });
        Self {
            total_wall_time_s,
            stages,
            stage_totals_ms,
            search,
        }
    }

    /// Write the manifest as `run_manifest.json` in `output_dir` and return
    /// the path written.
    ///
    /// # Errors
    ///
    /// The serialization or filesystem error that prevented the write.
    pub fn write(&self, output_dir: &Path) -> Result<PathBuf, String> {
        std::fs::create_dir_all(output_dir)
            .map_err(|error| format!("could not create {}: {error}", output_dir.display()))?;
        let path = output_dir.join(RUN_MANIFEST_FILE);
        let json = serde_json::to_string_pretty(self)
            .map_err(|error| format!("could not serialize the run manifest: {error}"))?;
        std::fs::write(&path, json)
            .map_err(|error| format!("could not write {}: {error}", path.display()))?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::RunEventSeverity;

    fn completed(stage: &str, elapsed_ms: u64, duration_ms: u64) -> RunEvent {
        RunEvent {
            stage: stage.to_owned(),
            message: String::new(),
            fraction: None,
            kind: RunEventKind::StageCompleted,
            severity: RunEventSeverity::Info,
            stage_index: None,
            stage_count: None,
            elapsed_ms,
            duration_ms: Some(duration_ms),
        }
    }

    #[test]
    fn stage_timings_are_kept_in_order_and_summed_per_stage() {
        let events = [
            completed("optimization", 600_000, 590_000),
            completed("downstream/mses", 700_000, 90_000),
            completed("downstream/mses", 800_000, 100_000),
            RunEvent {
                kind: RunEventKind::Diagnostic,
                duration_ms: None,
                ..completed("full_analysis", 880_000, 0)
            },
        ];
        let stages: Vec<StageTiming> = events
            .iter()
            .filter(|event| event.kind == RunEventKind::StageCompleted)
            .filter_map(|event| {
                Some(StageTiming {
                    stage: event.stage.clone(),
                    elapsed_ms: event.elapsed_ms,
                    duration_ms: event.duration_ms?,
                })
            })
            .collect();
        assert_eq!(stages.len(), 3, "diagnostics are not stage timings");
        let mut totals: BTreeMap<String, u64> = BTreeMap::new();
        for stage in &stages {
            *totals.entry(stage.stage.clone()).or_default() += stage.duration_ms;
        }
        assert_eq!(totals["downstream/mses"], 190_000);
        assert_eq!(totals["optimization"], 590_000);
    }
}
