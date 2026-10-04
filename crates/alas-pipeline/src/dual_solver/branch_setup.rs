// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Branch directories, seeded configuration and the cancelled-search record.

use super::*;

/// Independent configurations and the scheduling choice for the two branches.
pub(super) struct SolverBranches {
    pub(super) vlm_config: AlasConfig,
    pub(super) avl_config: AlasConfig,
    pub(super) parallel: bool,
}

/// Branch scheduling preserves the candidate worker count configured for
/// each optimizer, including automatic resolution against the machine.
pub(super) fn solver_branches(config: &AlasConfig, parallel: bool) -> SolverBranches {
    SolverBranches {
        vlm_config: config.clone(),
        avl_config: config.clone(),
        parallel,
    }
}

/// Persist what a cancelled search produced, in its own branch directory.
///
/// # Why a cancelled run writes anything at all
///
/// A cancelled branch delivers no design - it is not feasible, not converged,
/// and the pipeline fails the run - but it did execute real coupled analyses,
/// and their count, timing and the phase the stop landed in are the evidence
/// a later run is planned from. Throwing them away is what made every
/// cancelled run look identical from the outside.
///
/// # What this file is not
///
/// Every verdict field in it is `false` by construction, and it carries no
/// design vector: nothing downstream reads it, and nothing in it can be
/// mistaken for a result. It is a record of effort spent, labelled as such.
pub(super) fn write_cancelled_search_record(
    output_dir: Option<&Path>,
    optimization: &OptimizationResult,
    cancel: Option<&AtomicBool>,
) {
    let Some(directory) = output_dir else {
        return;
    };
    let diagnostics = optimization.search_diagnostics.as_ref();
    let telemetry = alas_opt::CancelScope::attach(cancel)
        .snapshot()
        .and_then(|snapshot| serde_json::to_value(snapshot).ok());
    let record = serde_json::json!({
        "record_kind": "cancelled_search",
        "explanation": "The search was stopped by its supervisor before it reached any \
                        stopping criterion of its own. This file records the analyses the run \
                        had already paid for. It is not a result: no design is delivered, and \
                        every verdict below is false.",
        "method": optimization.method,
        "strategy": optimization.strategy,
        "termination": optimization.termination,
        "stop_reason": alas_opt::StopReason::from_termination(&optimization.termination),
        "converged": false,
        "delivered_feasible": false,
        "best_valid": false,
        "wall_time_s": optimization.wall_time_s,
        "analyses": {
            "search_evaluations": diagnostics.map(|d| d.analysis_evaluations),
            "screening_evaluations": diagnostics.map(|d| d.screening_evaluations),
            "screening_feasible": diagnostics.map(|d| d.screening_feasible),
            "verification_evaluations": diagnostics.map(|d| d.verification_evaluations),
            "cache_hits": diagnostics.map(|d| d.cache_hits),
            "poll_iterations": diagnostics.map(|d| d.poll_iterations),
            "history_evaluations": optimization.history.n_evaluations(),
        },
        "timing_s": {
            "scan_wall_time": diagnostics.map(|d| d.scan_wall_time_s),
            "search_wall_time": diagnostics.map(|d| d.search_wall_time_s),
        },
        "cancellation_telemetry": telemetry,
    });
    let path = directory.join(CANCELLED_SEARCH_RECORD);
    match serde_json::to_vec_pretty(&record) {
        Ok(bytes) => {
            if let Err(error) = std::fs::write(&path, bytes) {
                tracing::warn!(%error, path = %path.display(), "cannot persist the cancelled-search record");
            }
        }
        Err(error) => {
            tracing::warn!(%error, "cannot serialize the cancelled-search record");
        }
    }
}

/// Create the VLM branch's evidence directory. The native search writes
/// nothing there itself, so a failure is logged and the search still runs.
pub(super) fn create_branch_directory(output_dir: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(path) = &output_dir {
        if let Err(error) = std::fs::create_dir_all(path) {
            tracing::warn!(%error, path = %path.display(), "solver branch directory could not be created");
        }
    }
    output_dir
}

/// The optimizer configuration with the run's seed applied.
pub(crate) fn seeded_config(config: &AlasConfig, seed: Option<u64>) -> Result<AlasConfig, String> {
    let mut effective = config.clone();
    if let Some(seed) = seed {
        effective
            .optimizer
            .solver
            .set_seed(seed)
            .map_err(|error| error.to_string())?;
    }
    Ok(effective)
}
