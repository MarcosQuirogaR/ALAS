// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

/// Parse the quality gate output. A zero exit code is insufficient: several
/// OpenFOAM releases report failed mesh checks while still returning success.
pub fn parse_mesh_quality(output: &str) -> MeshQuality {
    let lower = output.to_ascii_lowercase();
    let explicit_ok = lower.contains("mesh ok");
    let failed_checks = lower
        .split("failed")
        .filter_map(|tail| tail.split_whitespace().next())
        .filter_map(|token| token.parse::<u64>().ok())
        .any(|count| count > 0);
    let passed = explicit_ok && !failed_checks;
    let cells = output.lines().find_map(|line| {
        let lower_line = line.to_ascii_lowercase();
        lower_line.find("cells:").and_then(|index| {
            line[index + "cells:".len()..]
                .split_whitespace()
                .find_map(|token| token.parse::<u64>().ok())
        })
    });
    let max_non_orthogonality_deg = find_labeled_number(output, "max non-orthogonality")
        .or_else(|| find_labeled_number(output, "non-orthogonality max"));
    let max_skewness = find_labeled_number(output, "max skewness");
    let min_volume_m3 = find_labeled_number(output, "min volume");
    MeshQuality {
        passed,
        cells,
        max_non_orthogonality_deg,
        max_skewness,
        min_volume_m3,
        raw_output: output.to_owned(),
        near_wall: None,
    }
}

fn find_labeled_number(text: &str, label: &str) -> Option<f64> {
    text.lines().find_map(|line| {
        let lower = line.to_ascii_lowercase();
        let offset = lower.find(label)? + label.len();
        extract_numeric_values(&line[offset..]).into_iter().next()
    })
}

/// Decide whether the available evidence supports a numerically converged
/// coefficient result.
pub fn classify_convergence(
    config: &CfdStudyConfig,
    process_status: OpenFoamProcessStatus,
    mesh_quality: &MeshQuality,
    residuals: &[ResidualSample],
    forces: &[ForceSample],
    mass_balance: &[MassBalanceSample],
) -> (CfdOutcome, String) {
    // checkMesh can return exit code zero while reporting failed quality
    // checks. Preserve that concrete gate reason even though the runner marks
    // the overall study as failed.
    let mesh_reported_failure = !mesh_quality.passed
        && (!mesh_quality.raw_output.trim().is_empty())
        && (mesh_quality
            .raw_output
            .to_ascii_lowercase()
            .contains("failed"));
    if mesh_reported_failure {
        return (
            CfdOutcome::Failed,
            "checkMesh reported failed mesh checks; the solver was not launched.".to_owned(),
        );
    }
    match process_status {
        OpenFoamProcessStatus::Cancelled => {
            return (
                CfdOutcome::Cancelled,
                "The solver process was cancelled by the user.".to_owned(),
            )
        }
        OpenFoamProcessStatus::TimedOut => {
            return (
                CfdOutcome::Failed,
                "The solver exceeded its configured timeout.".to_owned(),
            )
        }
        OpenFoamProcessStatus::LaunchFailed => {
            return (
                CfdOutcome::Failed,
                "The solver could not be launched.".to_owned(),
            )
        }
        OpenFoamProcessStatus::Failed => {
            return (
                CfdOutcome::Failed,
                "The solver exited with a non-zero status.".to_owned(),
            )
        }
        OpenFoamProcessStatus::Completed => {}
    }
    if !mesh_quality.passed {
        return (
            CfdOutcome::Failed,
            "checkMesh did not report Mesh OK with zero failed checks.".to_owned(),
        );
    }
    if residuals.is_empty() {
        return (
            CfdOutcome::Unconverged,
            "No equation residuals were parsed from the solver log.".to_owned(),
        );
    }
    if residuals
        .iter()
        .any(|sample| !sample.initial.is_finite() || !sample.final_residual.is_finite())
    {
        return (
            CfdOutcome::Unconverged,
            "Equation residual history contains a non-finite value; convergence cannot be certified."
                .to_owned(),
        );
    }
    let required_equations = ["p", "ux", "uy", "k", "omega"];
    // OpenFOAM writes two pressure solves for each SIMPLE iteration.  Use a
    // common outer `Time =` marker and the largest initial residual for each
    // equation in that iteration; enumerating log lines would incorrectly
    // treat the second pressure correction as a later iteration.
    // The largest parsed outer time is authoritative.  Falling back to an
    // earlier complete iteration would allow a truncated or malformed final
    // solver iteration to inherit a green result from stale history.
    let latest_iteration = residuals
        .iter()
        .map(|sample| sample.iteration)
        .max()
        .unwrap_or_default();
    let missing_equations = required_equations
        .iter()
        .filter(|required| {
            !residuals.iter().any(|sample| {
                sample.iteration == latest_iteration
                    && sample.field.eq_ignore_ascii_case(required)
                    && sample.initial.is_finite()
            })
        })
        .copied()
        .collect::<Vec<_>>();
    if !missing_equations.is_empty() {
        return (
            CfdOutcome::Unconverged,
            format!(
                "The final log iteration is missing required equation(s): {}.",
                missing_equations.join(", ")
            ),
        );
    }
    let mut last_initial_residuals = BTreeMap::new();
    for residual in residuals
        .iter()
        .filter(|sample| sample.iteration == latest_iteration)
    {
        let field = residual.field.to_ascii_lowercase();
        last_initial_residuals
            .entry(field)
            .and_modify(|value: &mut f64| *value = value.max(residual.initial))
            .or_insert(residual.initial);
    }
    if required_equations.iter().any(|required| {
        !residuals.iter().any(|sample| {
            sample.iteration == latest_iteration
                && sample.field.eq_ignore_ascii_case(required)
                && sample.final_residual.is_finite()
        })
    }) {
        return (
            CfdOutcome::Unconverged,
            "The final outer SIMPLE iteration lacks a finite linear-solver residual for every primary equation."
                .to_owned(),
        );
    }
    if required_equations.iter().any(|required| {
        last_initial_residuals
            .get(*required)
            .is_none_or(|value| !value.is_finite() || *value > config.solver.residual_tolerance)
    }) {
        return (
            CfdOutcome::Unconverged,
            format!(
                "At least one final outer SIMPLE initial residual exceeds {:.3e}.",
                config.solver.residual_tolerance
            ),
        );
    }
    if forces.iter().any(|sample| {
        !sample.time.is_finite()
            || !sample.cd.is_finite()
            || !sample.cl.is_finite()
            || !sample.cm.is_finite()
            || [
                sample.cd_pressure,
                sample.cd_viscous,
                sample.cl_pressure,
                sample.cl_viscous,
            ]
            .into_iter()
            .flatten()
            .any(|value| !value.is_finite())
    }) {
        return (
            CfdOutcome::Unconverged,
            "Force history contains a non-finite coefficient; convergence cannot be certified."
                .to_owned(),
        );
    }
    let window = config.solver.force_window.min(forces.len());
    if window < 3 {
        return (
            CfdOutcome::Unconverged,
            "Fewer than three force samples are available for stabilization.".to_owned(),
        );
    }
    let tail = &forces[forces.len() - window..];
    if relative_spread(tail.iter().map(|row| row.cd)) > config.solver.force_tolerance
        || relative_spread(tail.iter().map(|row| row.cl)) > config.solver.force_tolerance
        || relative_spread(tail.iter().map(|row| row.cm)) > config.solver.force_tolerance
    {
        return (
            CfdOutcome::Unconverged,
            format!(
                "Lift, drag or pitching moment has not stabilized within {:.3} relative spread.",
                config.solver.force_tolerance
            ),
        );
    }
    if mass_balance.is_empty() {
        return (
            CfdOutcome::Unconverged,
            "No continuity/mass-balance diagnostic was parsed from the solver log.".to_owned(),
        );
    }
    // `cumulative` is an integral history and is expected to remain larger
    // than a per-iteration tolerance in a long run.  Gate only the latest
    // local/global continuity errors; retain cumulative values as audit data.
    let continuity_bad = mass_balance
        .iter()
        .rev()
        .find(|row| row.sum_local.is_some() || row.global.is_some())
        .map(|row| {
            [row.sum_local, row.global]
                .into_iter()
                .flatten()
                .any(|value| {
                    !value.is_finite() || value.abs() > config.solver.mass_balance_tolerance
                })
        })
        .unwrap_or(true);
    if continuity_bad {
        return (
            CfdOutcome::Unconverged,
            format!(
                "Continuity error exceeds {:.3e}.",
                config.solver.mass_balance_tolerance
            ),
        );
    }
    (
        CfdOutcome::NumericallyConverged,
        "Residuals, stabilized forces and continuity diagnostics satisfy the configured criteria."
            .to_owned(),
    )
}

fn relative_spread(values: impl Iterator<Item = f64>) -> f64 {
    let values = values.collect::<Vec<_>>();
    if values.is_empty() {
        return f64::INFINITY;
    }
    if values.iter().any(|value| !value.is_finite()) {
        return f64::INFINITY;
    }
    let min = values
        .iter()
        .fold(f64::INFINITY, |value, next| value.min(*next));
    let max = values
        .iter()
        .fold(f64::NEG_INFINITY, |value, next| value.max(*next));
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    (max - min) / mean.abs().max(1.0e-3)
}
