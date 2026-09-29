// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Independent VLM and AVL optimization branches.
//!
//! The native VLM objective remains the reference product path. The AVL
//! branch uses the same design vector, geometry builder, and hard feasibility
//! checks, but scores the admitted AVL Trefftz induced drag at the required
//! cruise lift. Keeping the branches in separate output namespaces means a
//! slow or unavailable external executable cannot corrupt the VLM result.

mod avl;
mod branch_setup;
#[cfg(test)]
// Tests build their own fixtures and assert on them, so a failed expect is
// the assertion failing rather than a library invariant breaking.
#[allow(clippy::expect_used)]
mod tests;
mod vlm;

use avl::*;
pub(crate) use branch_setup::seeded_config;
use branch_setup::*;
use vlm::*;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use alas_aero::avl::{AvlPolar, AvlPolarPoint};
use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_exec::RunEnvironment;
use alas_opt::objective::DesignObjective;
use alas_opt::{
    DeliveredAcceptance, DesignOptimizer, ExternalPolar, ObjectiveEvaluation, ObjectiveEvaluator,
    OptimizationResult,
};

use crate::acceptance::{AcceptanceRoute, FinalistVerification, MAX_VERIFIED_CANDIDATES};
use crate::avl::{run_avl_analysis, AvlAnalysisResult, AvlAnalysisStatus};
use crate::full_analysis::{AnalysisReport, FullAnalysis};
use crate::solver_mode::{OptimizationSolverMode, SolverKind};

/// Reported when a branch's optimizer stopped on the pipeline's own
/// cancellation signal rather than a search failure.
///
/// The `"Cancelled safely"` prefix matches `pipeline::check_cancelled`'s own
/// wording, which is the exact prefix `alas-gui` checks
/// (`crates/alas-gui/src/run.rs`, `e.starts_with("Cancelled safely")`) to
/// report a run as cancelled instead of failed; this string keeps a
/// cancelled optimization branch on that same GUI path without any GUI
/// change.
const CANCELLED_DURING_OPTIMIZATION: &str = "Cancelled safely during design-space optimization";

/// Candidate AVL execution budget, seconds. Separate from configurable final
/// reporting: candidate sweeps still drain to this deadline after cancellation.
const AVL_EVALUATION_TIMEOUT_S: f64 = 60.0;

/// Written to a cancelled branch's own directory so the analyses the search
/// already paid for survive the cancellation.
const CANCELLED_SEARCH_RECORD: &str = "cancelled_search.json";

/// Lifecycle state of one requested optimization branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolverOptimizationStatus {
    /// This backend was not selected for the run.
    NotRequested,
    /// The backend produced a design and a full report.
    Completed,
    /// The selected backend could not produce a usable result.
    Failed,
}

impl SolverOptimizationStatus {
    /// Stable spelling for run manifests and UI diagnostics.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotRequested => "not_requested",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

/// One independently optimized aircraft solution.
#[derive(Debug, Clone, PartialEq)]
pub struct SolverOptimizationResult {
    /// Concrete backend that produced this solution.
    pub solver: SolverKind,
    /// Explicit lifecycle state.
    pub status: SolverOptimizationStatus,
    /// Best design vector, when the branch completed.
    pub design: Option<DesignVector>,
    /// Optimizer trajectory, when the branch completed.
    pub optimization: Option<OptimizationResult>,
    /// Full report for this branch's best design.
    pub report: Option<AnalysisReport>,
    /// Retained AVL run for the AVL branch's best design.
    pub avl_result: Option<AvlAnalysisResult>,
    /// Branch-local output directory, when one is configured.
    pub output_dir: Option<PathBuf>,
    /// Actionable failure detail, present only for a failed branch.
    pub error: Option<String>,
}

impl SolverOptimizationResult {
    fn not_requested(solver: SolverKind) -> Self {
        Self {
            solver,
            status: SolverOptimizationStatus::NotRequested,
            design: None,
            optimization: None,
            report: None,
            avl_result: None,
            output_dir: None,
            error: None,
        }
    }

    fn failed(solver: SolverKind, output_dir: Option<PathBuf>, error: impl Into<String>) -> Self {
        Self {
            solver,
            status: SolverOptimizationStatus::Failed,
            design: None,
            optimization: None,
            report: None,
            avl_result: None,
            output_dir,
            error: Some(error.into()),
        }
    }
}

/// Run-scoped pair of independently optimized aircraft solutions.
#[derive(Debug, Clone, PartialEq)]
pub struct SolverOptimizationSet {
    /// Native VLM optimization branch.
    pub vlm: SolverOptimizationResult,
    /// External AVL optimization branch.
    pub avl: SolverOptimizationResult,
}

impl SolverOptimizationSet {
    /// Select the requested primary solution, without silently falling back
    /// from an explicitly AVL-only request.
    pub fn selected(
        &self,
        mode: OptimizationSolverMode,
    ) -> Result<&SolverOptimizationResult, String> {
        match mode {
            OptimizationSolverMode::Vlm => self.completed_or_error(&self.vlm),
            OptimizationSolverMode::Avl => self.completed_or_error(&self.avl),
            OptimizationSolverMode::Both => {
                if self.vlm.status == SolverOptimizationStatus::Completed {
                    Ok(&self.vlm)
                } else {
                    self.completed_or_error(&self.avl)
                }
            }
        }
    }

    fn completed_or_error<'a>(
        &self,
        result: &'a SolverOptimizationResult,
    ) -> Result<&'a SolverOptimizationResult, String> {
        if result.status == SolverOptimizationStatus::Completed {
            Ok(result)
        } else {
            Err(result.error.clone().unwrap_or_else(|| {
                format!(
                    "{} optimization did not produce a usable result",
                    result.solver.as_str()
                )
            }))
        }
    }
}

/// Run the requested optimizer branches, optionally in parallel.
// Each argument is an independent runtime control or typed input boundary;
// keeping them explicit makes the pipeline call site auditable and avoids a
// configuration object that could silently mix the two solver namespaces.
#[allow(clippy::too_many_arguments)]
pub fn run_solver_optimizations(
    config: &AlasConfig,
    mode: OptimizationSolverMode,
    parallel: bool,
    seed: Option<u64>,
    environment: &RunEnvironment,
    nominal_design: &DesignVector,
    bounds: Option<&[(f64, f64)]>,
    output_dir: Option<&Path>,
    acceptance_route: Option<&AcceptanceRoute>,
    cancel: Option<&AtomicBool>,
) -> SolverOptimizationSet {
    let want_vlm = matches!(
        mode,
        OptimizationSolverMode::Vlm | OptimizationSolverMode::Both
    );
    let want_avl = matches!(
        mode,
        OptimizationSolverMode::Avl | OptimizationSolverMode::Both
    );
    let bounds = bounds.map(<[(f64, f64)]>::to_vec);
    let nominal = *nominal_design;
    let vlm_config = serial_solver_config(config, parallel);
    let avl_config = serial_solver_config(config, parallel);
    let avl_environment = environment.clone();
    let vlm_output = output_dir.map(|path| path.join("solvers/vlm"));
    let avl_output = output_dir.map(|path| path.join("solvers/avl"));

    let run_vlm = || {
        if want_vlm {
            run_vlm_optimizer(VlmOptimizerRequest {
                config: vlm_config,
                seed,
                nominal,
                bounds: bounds.as_deref(),
                output_dir: vlm_output,
                acceptance_route,
                cancel,
            })
        } else {
            SolverOptimizationResult::not_requested(SolverKind::Vlm)
        }
    };
    let run_avl = || {
        if want_avl {
            run_avl_optimizer(
                avl_config,
                seed,
                avl_environment,
                nominal,
                bounds.as_deref(),
                avl_output,
                cancel,
            )
        } else {
            SolverOptimizationResult::not_requested(SolverKind::Avl)
        }
    };

    if parallel && want_vlm && want_avl {
        std::thread::scope(|scope| {
            let vlm = scope.spawn(run_vlm);
            let avl = scope.spawn(run_avl);
            SolverOptimizationSet {
                vlm: vlm.join().unwrap_or_else(|_| {
                    SolverOptimizationResult::failed(
                        SolverKind::Vlm,
                        None,
                        "VLM optimizer worker panicked",
                    )
                }),
                avl: avl.join().unwrap_or_else(|_| {
                    SolverOptimizationResult::failed(
                        SolverKind::Avl,
                        None,
                        "AVL optimizer worker panicked",
                    )
                }),
            }
        })
    } else {
        SolverOptimizationSet {
            vlm: run_vlm(),
            avl: run_avl(),
        }
    }
}
