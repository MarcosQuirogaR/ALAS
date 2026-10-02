// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Command-line controls beyond the search effort (`--mtow-mode`,
//! `--time-limit`, `--max-evaluations`) and the baseline pipeline run that
//! supplies the like-for-like native-mission trip fuel.

use std::io;
use std::path::Path;
use std::time::Duration;

use alas_config::{AlasConfig, MtowSizing};
use alas_exec::RunEnvironment;
use alas_pipeline::{PipelineOptions, PipelineResult};

use super::{panic_message, run_with_timeout, RunOutcome};

fn invalid(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

/// The value after `--mtow-mode`, by its stable serialized name.
pub(super) fn parse_mtow_mode(value: Option<&String>) -> io::Result<MtowSizing> {
    let name = value.map(String::as_str).unwrap_or_default();
    MtowSizing::ALL
        .into_iter()
        .find(|mode| mode.as_str() == name)
        .ok_or_else(|| {
            invalid(format!(
                "--mtow-mode requires one of {}",
                MtowSizing::NAMES.join(", ")
            ))
        })
}

/// The non-negative integer after `flag`.
pub(super) fn parse_count(flag: &str, value: Option<&String>) -> io::Result<u64> {
    value
        .and_then(|text| text.parse().ok())
        .ok_or_else(|| invalid(format!("{flag} requires an integer")))
}

/// The `S,R[,P[,Q]]` after `--replay-evaluations`: the screening and
/// refinement replay counts a time-limited run recorded, the refinement's
/// planned budget and its restoration share (`restoration_evaluations`, at
/// most `R`).
pub(super) fn parse_replay(
    value: Option<&String>,
) -> io::Result<(u64, u64, Option<u64>, Option<u64>)> {
    let counts: Option<Vec<u64>> = value.and_then(|text| {
        text.split(',')
            .map(|count| count.trim().parse().ok())
            .collect()
    });
    match counts.as_deref() {
        Some(&[s, r]) if s > 0 && r > 0 => Ok((s, r, None, None)),
        Some(&[s, r, p]) if s > 0 && r > 0 && p > 0 => Ok((s, r, Some(p), None)),
        Some(&[s, r, p, q]) if s > 0 && r > 0 && p > 0 && q <= r => Ok((s, r, Some(p), Some(q))),
        _ => Err(invalid(
            "--replay-evaluations requires positive counts S,R or S,R,P, or S,R,P,Q with Q <= R"
                .to_owned(),
        )),
    }
}

/// The pipeline on the preset nominal with `optimize = false`: the native
/// mission and reported analysis of the baseline, from the same code path as
/// the winner's. Its failure is recorded in the row, never raised.
pub(super) fn run_baseline_pipeline(
    config: AlasConfig,
    environment: RunEnvironment,
    seed: u64,
    result_dir: &Path,
    timeout: Option<Duration>,
) -> Result<PipelineResult, String> {
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: true,
        aerodynamic_solver: alas_pipeline::AerodynamicSolverMode::Both,
        optimization_solver: alas_pipeline::OptimizationSolverMode::Vlm,
        output_dir: Some(result_dir.join("baseline_run")),
        save_plots: false,
        seed: Some(seed),
        quiet: true,
    };
    match run_with_timeout(config, options, environment, timeout) {
        RunOutcome::Finished(result, _, _) => match *result {
            Ok(Ok(run)) => Ok(run),
            Ok(Err(error)) => Err(error),
            Err(panic) => Err(format!("pipeline panicked: {}", panic_message(&panic))),
        },
        RunOutcome::Cancelled { detail, .. } => {
            Err(format!("cancelled by the external guard: {detail}"))
        }
    }
}
