// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compatibility and cancellable entry points for native AVL reporting.
use super::*;

/// Export, run, parse, and classify one native AVL sweep.
pub fn run_avl_analysis(
    report: &AnalysisReport,
    config: &AlasConfig,
    output_dir: &Path,
    executable: Option<&Path>,
    timeout_seconds: f64,
) -> AvlAnalysisResult {
    run_avl_analysis_cancellable(
        report,
        config,
        output_dir,
        executable,
        timeout_seconds,
        None,
    )
}

/// Native AVL reporting with cooperative process cancellation.
pub fn run_avl_analysis_cancellable(
    report: &AnalysisReport,
    config: &AlasConfig,
    output_dir: &Path,
    executable: Option<&Path>,
    timeout_seconds: f64,
    cancel: Option<&AtomicBool>,
) -> AvlAnalysisResult {
    let reference = AvlComparisonReference {
        phase: "cruise".to_owned(),
        mach: config.requirements.cruise_mach,
        altitude_m: config.requirements.cruise_altitude_m,
        vlm_polar: report.polar.clone(),
        geometric_alpha_deg: report.polar.geometric_alpha_deg.clone(),
    };
    run_avl_analysis_with_reference(
        report,
        config,
        output_dir,
        executable,
        timeout_seconds,
        reference,
        cancel,
    )
}

/// Compare AVL with ALAS VLM at the configured takeoff-climb condition.
///
/// The optimizer and the cruise report remain unchanged. This is a separate
/// diagnostic run at the midpoint of the configured takeoff segment, where
/// the linear Prandtl-Glauert model is comfortably inside its documented
/// expected-valid domain for ordinary transport-aircraft schedules.
pub fn run_avl_takeoff_comparison(
    report: &AnalysisReport,
    config: &AlasConfig,
    output_dir: &Path,
    executable: Option<&Path>,
    timeout_seconds: f64,
) -> AvlAnalysisResult {
    run_avl_takeoff_comparison_cancellable(
        report,
        config,
        output_dir,
        executable,
        timeout_seconds,
        None,
    )
}

/// Native AVL reporting with cooperative process cancellation.
pub fn run_avl_takeoff_comparison_cancellable(
    report: &AnalysisReport,
    config: &AlasConfig,
    output_dir: &Path,
    executable: Option<&Path>,
    timeout_seconds: f64,
    cancel: Option<&AtomicBool>,
) -> AvlAnalysisResult {
    let reference = match takeoff_comparison_reference(report, config) {
        Ok(reference) => reference,
        Err(error) => return rejected_reference_result(output_dir, executable, error),
    };
    run_avl_analysis_with_reference(
        report,
        config,
        output_dir,
        executable,
        timeout_seconds,
        reference,
        cancel,
    )
}
