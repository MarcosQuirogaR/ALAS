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

/// The MTOW modes the application offers a user; the calibration modes
/// (`sized_by_mission`, `unconstrained`) are not aircraft benchmarks.
pub(super) const USER_MTOW_MODES: [MtowSizing; 3] = [
    MtowSizing::FixedRequirement,
    MtowSizing::MtowBand,
    MtowSizing::PayloadAdjusted,
];

/// The value after `--mtow-mode`, by its stable serialized name.
pub(super) fn parse_mtow_mode(value: Option<&String>) -> io::Result<MtowSizing> {
    let name = value.map(String::as_str).unwrap_or_default();
    USER_MTOW_MODES
        .into_iter()
        .find(|mode| mode.as_str() == name)
        .ok_or_else(|| {
            invalid(
                "--mtow-mode requires fixed_requirement, mtow_band or payload_adjusted: the \
                 modes the application offers"
                    .to_owned(),
            )
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
/// the winner's. This auxiliary comparison executes native analyses only;
/// its failure is recorded in the row, never raised.
pub(super) fn run_baseline_pipeline(
    config: AlasConfig,
    environment: RunEnvironment,
    seed: u64,
    result_dir: &Path,
    timeout: Option<Duration>,
) -> Result<PipelineResult, String> {
    let (config, options) = baseline_request(config, seed, result_dir);
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

fn baseline_request(
    mut config: AlasConfig,
    seed: u64,
    result_dir: &Path,
) -> (AlasConfig, PipelineOptions) {
    super::preparation::apply_execution_scope(&mut config, true);
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: true,
        aerodynamic_solver: alas_pipeline::AerodynamicSolverMode::Vlm,
        optimization_solver: alas_pipeline::OptimizationSolverMode::Vlm,
        output_dir: Some(result_dir.join("baseline_run")),
        save_plots: false,
        seed: Some(seed),
        quiet: true,
    };
    (config, options)
}

#[cfg(test)]
// Registered preset decoding failures are test assertions.
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn auxiliary_baseline_keeps_native_physics_and_disables_external_execution() {
        for preset in alas_config::presets::registry() {
            let original = AlasConfig::from_value(&serde_json::json!({"preset":preset.name}))
                .expect("registered preset");
            let (mut native, options) =
                baseline_request(original.clone(), 7, Path::new("evidence"));
            assert_eq!(
                options.aerodynamic_solver,
                alas_pipeline::AerodynamicSolverMode::Vlm
            );
            assert!(!options.optimize && !options.compare_baseline);
            assert!(!native.mses.enabled);
            assert!(!native.downstream.openvsp && !native.downstream.vspaero);
            assert!(!native.downstream.avl && !native.downstream.flowunsteady);
            assert!(!native.structures.run_nastran && !native.structures.run_patran_export);
            native.mses.enabled = original.mses.enabled;
            native.downstream = original.downstream.clone();
            native.structures.run_nastran = original.structures.run_nastran;
            native.structures.run_patran_export = original.structures.run_patran_export;
            assert_eq!(native, original, "{} native model", preset.name);
        }
    }
}
