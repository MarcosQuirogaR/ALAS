// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What a completed run actually did, as opposed to what it was configured to
//! do.
//!
//! A run's per-stage timings and its search's evaluation count are both
//! already computed: the stage timings ride on [`crate::runs::RunEvent`] and
//! the search metadata on [`alas_opt::OptimizationResult`], but neither was
//! ever written to disk, so a slow run could only be diagnosed by rerunning it
//! under observation. This manifest persists them beside the design database.
//!
//! The executed search method is recorded separately from the configured one
//! on purpose. A saved configuration may still carry a legacy method token
//! (`sqp`, `nsga2`, `turbo_1`, `cma_es`), which is migrated to
//! `differential_evolution` at load time
//! (`alas_config::settings_load_notes`); `configured_method` here is the
//! string the loaded configuration actually carries, and `executed_method` is
//! what the optimizer reports back (`alas_opt::DesignOptimizer::run_product_search`,
//! the L-SHADE epsilon-constrained kernel). Reading the configured string as
//! the executed algorithm is how a run gets diagnosed against the wrong
//! search.

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
    /// Whether the search reached its own convergence criterion *and* the
    /// design it delivered survived the reporting-fidelity re-evaluation.
    /// Absent for a search that reports no lifecycle diagnostics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub converged: Option<bool>,
    /// What the reporting-fidelity re-evaluation made of the delivered
    /// design. Absent when no caller performed one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivered_acceptance: Option<DeliveredAcceptanceManifest>,
    /// How the search spent its budget. Absent for a search method that
    /// reports no diagnostics, which is not the same as a search that
    /// reported zeros.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<SearchDiagnosticsManifest>,
}

/// How a staged search spent its evaluation budget.
///
/// Held separately from the optimizer's own `SearchDiagnostics` for the same
/// reason as [`DeliveredAcceptanceManifest`]: the manifest is a document a
/// reader parses without tracking the optimizer's internals. The two
/// optional costs stay optional here rather than becoming zero, because "the
/// search never reached a feasible point" and "it reached one at zero cost"
/// are different runs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchDiagnosticsManifest {
    /// Whether the search reached its own convergence criterion. A budget,
    /// iteration, mesh-floor or watchdog stop is `false`.
    pub converged: bool,
    /// Coupled full-fidelity analyses executed by the search stage.
    pub analysis_evaluations: usize,
    /// Repeated mesh nodes served from the search's cache, never analysed.
    pub cache_hits: usize,
    /// Poll iterations completed.
    pub poll_iterations: usize,
    /// Reduced-model analyses executed by the broad scan. Ranked on a
    /// coarser mesh and a looser sizing closure, so not comparable with the
    /// full-fidelity count above.
    pub screening_evaluations: usize,
    /// Screened candidates that were feasible under the reduced model.
    pub screening_feasible: usize,
    /// Full-fidelity analyses spent verifying the scan finalists.
    pub verification_evaluations: usize,
    /// Wall-clock seconds in the broad scan.
    pub scan_wall_time_s: f64,
    /// Wall-clock seconds in the search stage.
    pub search_wall_time_s: f64,
    /// Worker threads used inside one evaluation block.
    pub workers: usize,
    /// Points evaluated per opportunistic poll block, fixed independently of
    /// `workers` so a result does not change with the hardware.
    pub poll_block_size: usize,
    /// Objective of the first feasible point the search reached. Absent when
    /// it never reached one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_feasible_cost: Option<f64>,
    /// Relative improvement of the winner over that first feasible point,
    /// dimensionless. Absent for the same reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_improvement: Option<f64>,
    /// Fraction of the final generation's population that was strictly
    /// feasible. Absent (`0.0`) on a manifest written before this field
    /// existed.
    #[serde(default)]
    pub feasible_fraction: f64,
    /// The epsilon-constrained method's boundary at the last generation
    /// evaluated; `0` once past the epsilon control fraction of the budget.
    #[serde(default)]
    pub epsilon_level: f64,
}

impl From<&alas_opt::SearchDiagnostics> for SearchDiagnosticsManifest {
    fn from(diagnostics: &alas_opt::SearchDiagnostics) -> Self {
        Self {
            converged: diagnostics.converged,
            analysis_evaluations: diagnostics.analysis_evaluations,
            cache_hits: diagnostics.cache_hits,
            poll_iterations: diagnostics.poll_iterations,
            screening_evaluations: diagnostics.screening_evaluations,
            screening_feasible: diagnostics.screening_feasible,
            verification_evaluations: diagnostics.verification_evaluations,
            scan_wall_time_s: diagnostics.scan_wall_time_s,
            search_wall_time_s: diagnostics.search_wall_time_s,
            workers: diagnostics.workers,
            poll_block_size: diagnostics.poll_block_size,
            first_feasible_cost: diagnostics.first_feasible_cost,
            relative_improvement: diagnostics.relative_improvement,
            feasible_fraction: diagnostics.feasible_fraction,
            epsilon_level: diagnostics.epsilon_level,
        }
    }
}

/// The acceptance record as a run manifest carries it.
///
/// Kept structurally separate from the optimizer's own type so the manifest
/// is a stable document: a reader checking whether a delivered aircraft was
/// accepted, and by what, does not have to track the optimizer's internals.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeliveredAcceptanceManifest {
    /// Whether the delivered design passed with no error-severity finding.
    pub verified: bool,
    /// Findings that rejected the search's own finalist, by identifier.
    pub finalist_rejected_by: Vec<String>,
    /// Findings that reject the delivered design, by identifier. Empty when
    /// the delivered design was accepted.
    pub delivered_rejected_by: Vec<String>,
    /// The rejecting findings' own messages: an identifier says which check
    /// refused the design, only the message says why.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rejection_messages: Vec<String>,
    /// Candidates re-evaluated at reporting fidelity, the finalist included.
    pub candidates_evaluated: usize,
    /// Whether the delivered design is the search's own finalist.
    pub delivered_is_search_finalist: bool,
    /// Wall-clock seconds the re-evaluation cost, inside the search stage.
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
                converged: optimization
                    .search_diagnostics
                    .as_ref()
                    .map(|diagnostics| diagnostics.converged),
                delivered_acceptance: optimization.delivered_acceptance.as_ref().map(
                    |acceptance| DeliveredAcceptanceManifest {
                        verified: acceptance.verified,
                        finalist_rejected_by: acceptance.finalist_rejected_by.clone(),
                        delivered_rejected_by: acceptance.delivered_rejected_by.clone(),
                        rejection_messages: acceptance.rejection_messages.clone(),
                        candidates_evaluated: acceptance.candidates_evaluated,
                        delivered_is_search_finalist: acceptance.delivered_is_search_finalist,
                        wall_time_s: acceptance.wall_time_s,
                    },
                ),
                diagnostics: optimization
                    .search_diagnostics
                    .as_ref()
                    .map(SearchDiagnosticsManifest::from),
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

// Tests build their own fixtures and assert on them, so a failed expect is
// the assertion failing rather than a library invariant breaking.
#[allow(clippy::expect_used)]
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

    fn reported() -> alas_opt::SearchDiagnostics {
        alas_opt::SearchDiagnostics {
            converged: true,
            analysis_evaluations: 225,
            cache_hits: 17,
            poll_iterations: 31,
            screening_evaluations: 480,
            screening_feasible: 96,
            verification_evaluations: 8,
            scan_wall_time_s: 12.5,
            search_wall_time_s: 131.83,
            workers: 1,
            poll_block_size: 4,
            first_feasible_cost: Some(1.25),
            relative_improvement: Some(0.083),
            feasible_fraction: 0.92,
            epsilon_level: 0.0,
        }
    }

    #[test]
    fn every_reported_diagnostic_reaches_the_manifest_unchanged() {
        // The manifest is the only durable record of how a search spent its
        // budget, so a field dropped in this conversion is a measurement
        // that silently stops existing.
        let source = reported();
        let manifest = SearchDiagnosticsManifest::from(&source);
        assert_eq!(manifest.converged, source.converged);
        assert_eq!(manifest.analysis_evaluations, source.analysis_evaluations);
        assert_eq!(manifest.cache_hits, source.cache_hits);
        assert_eq!(manifest.poll_iterations, source.poll_iterations);
        assert_eq!(manifest.screening_evaluations, source.screening_evaluations);
        assert_eq!(manifest.screening_feasible, source.screening_feasible);
        assert_eq!(
            manifest.verification_evaluations,
            source.verification_evaluations
        );
        assert_eq!(manifest.scan_wall_time_s, source.scan_wall_time_s);
        assert_eq!(manifest.search_wall_time_s, source.search_wall_time_s);
        assert_eq!(manifest.workers, source.workers);
        assert_eq!(manifest.poll_block_size, source.poll_block_size);
        assert_eq!(manifest.first_feasible_cost, source.first_feasible_cost);
        assert_eq!(manifest.relative_improvement, source.relative_improvement);
        assert_eq!(manifest.feasible_fraction, source.feasible_fraction);
        assert_eq!(manifest.epsilon_level, source.epsilon_level);
    }

    #[test]
    fn a_search_that_never_became_feasible_omits_the_two_costs_rather_than_zeroing_them() {
        // Absent and zero are different runs, and a reader of the document
        // has to be able to tell them apart.
        let mut source = reported();
        source.first_feasible_cost = None;
        source.relative_improvement = None;
        let json = serde_json::to_value(SearchDiagnosticsManifest::from(&source))
            .expect("the manifest serializes");
        let object = json.as_object().expect("a JSON object");
        assert!(!object.contains_key("first_feasible_cost"), "{json}");
        assert!(!object.contains_key("relative_improvement"), "{json}");
        assert_eq!(object["analysis_evaluations"], 225);

        let with_costs = serde_json::to_value(SearchDiagnosticsManifest::from(&reported()))
            .expect("the manifest serializes");
        assert_eq!(with_costs["first_feasible_cost"], 1.25);
    }

    #[test]
    fn a_manifest_written_before_this_field_existed_still_parses() {
        // `diagnostics` is optional on the way in as well as out, so an
        // older run's manifest stays readable rather than becoming a parse
        // error that looks like a corrupt run.
        let json = serde_json::json!({
            "executed_method": "differential_evolution",
            "configured_method": "differential_evolution",
            "strategy": "staged",
            "termination": "converged",
            "evaluations": 225,
            "wall_time_s": 131.83
        });
        let search: SearchManifest = serde_json::from_value(json).expect("an older manifest");
        assert!(search.diagnostics.is_none());
        assert!(search.converged.is_none());
    }
}
