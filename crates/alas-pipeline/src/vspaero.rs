// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Product orchestration for an independent VSPAERO lifting-surface solve.
//!
//! Geometry generation, native solver completion, native-file parsing, and
//! model comparability are separate claims. The result keeps those claims
//! separate so an unavailable or incompatible VSPAERO run cannot silently
//! fall back to the in-process aerodynamic model.

use std::fs;
use std::path::{Path, PathBuf};

use alas_aero::vspaero::{
    parse_polar, render_setup, ReferenceLengthUnit, VspaeroModel, VspaeroPolar, VspaeroReference,
    VspaeroSweepRequest,
};
use alas_atmo::Atmosphere;
use alas_config::AlasConfig;
use alas_exec::vspaero::{run_vspaero, VspaeroProcessStatus};

use crate::full_analysis::AnalysisReport;
use crate::openvsp::OpenVspExportResult;

/// Product-level outcome of a requested VSPAERO analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VspaeroAnalysisStatus {
    /// No native VSPAERO executable was configured or discovered.
    NotConfigured,
    /// OpenVSP did not produce the lifting-surface mesh VSPAERO needs.
    GeometryUnavailable,
    /// The SI setup request was invalid or could not be written.
    SetupRejected,
    /// The configured timeout was not a finite number of seconds greater than
    /// zero; the solver was not launched.
    InvalidTimeout,
    /// The native executable could not be launched.
    LaunchFailed,
    /// The native process exceeded its deadline.
    TimedOut,
    /// VSPAERO returned a failing process status.
    SolverFailed,
    /// VSPAERO returned success without a fresh polar.
    OutputMissing,
    /// The fresh native polar failed the strict parser contract.
    ParseFailed,
    /// A polar was parsed, but it is not safe to overlay on the report model.
    CompletedNotComparable,
    /// A polar was parsed and its shared quantities satisfy the comparison contract.
    CompletedComparable,
}

impl VspaeroAnalysisStatus {
    /// Stable text used by retained runtime evidence.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotConfigured => "not_configured",
            Self::GeometryUnavailable => "geometry_unavailable",
            Self::SetupRejected => "setup_rejected",
            Self::InvalidTimeout => "invalid_timeout",
            Self::LaunchFailed => "launch_failed",
            Self::TimedOut => "timed_out",
            Self::SolverFailed => "solver_failed",
            Self::OutputMissing => "output_missing",
            Self::ParseFailed => "parse_failed",
            Self::CompletedNotComparable => "completed_not_comparable",
            Self::CompletedComparable => "completed_comparable",
        }
    }
}

/// Coefficients safe to overlay after the full reference/frame check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VspaeroComparableQuantity {
    /// Wind-axis lift coefficient against physical angle of attack.
    LiftCoefficient,
    /// Body-axis pitching-moment coefficient about the shared moment origin.
    PitchingMomentCoefficient,
}

/// Why a parsed polar is or is not comparable with the report polar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VspaeroComparisonStatus {
    /// No parsed polar exists to assess.
    NotEvaluated,
    /// At least one reference, frame, geometry, or operating condition differs.
    Rejected(String),
    /// The named coefficients share physical definitions with the report.
    Compatible(Vec<VspaeroComparableQuantity>),
}

/// Files, native output, and status from one product VSPAERO stage.
#[derive(Debug, Clone, PartialEq)]
pub struct VspaeroAnalysisResult {
    /// Explicit product status; never inferred from an optional polar alone.
    pub status: VspaeroAnalysisStatus,
    /// Native solver executable used, when one was attempted.
    pub runtime_executable: Option<PathBuf>,
    /// Extensionless case path.
    pub case_path: PathBuf,
    /// OpenVSP lifting-surface mesh.
    pub geometry_path: PathBuf,
    /// Rust-written native setup.
    pub setup_path: PathBuf,
    /// Native solver polar.
    pub polar_path: PathBuf,
    /// Captured native standard output.
    pub stdout_path: PathBuf,
    /// Captured native standard error.
    pub stderr_path: PathBuf,
    /// Strictly parsed polar, retained even when comparison is rejected.
    pub polar: Option<VspaeroPolar>,
    /// Quantity-level model-comparison contract.
    pub comparison: VspaeroComparisonStatus,
    /// Actionable failure or incompatibility detail.
    pub error: Option<String>,
}

impl VspaeroAnalysisResult {
    /// True only when a parsed polar may be used by the whole-aircraft view.
    pub fn comparable_polar(&self) -> Option<&VspaeroPolar> {
        matches!(self.comparison, VspaeroComparisonStatus::Compatible(_))
            .then_some(())
            .and(self.polar.as_ref())
    }
}

/// Build, run, parse, and classify the independent VSPAERO model.
pub fn run_vspaero_analysis(
    report: &AnalysisReport,
    config: &AlasConfig,
    openvsp: &OpenVspExportResult,
    executable: Option<&Path>,
    timeout_seconds: f64,
) -> VspaeroAnalysisResult {
    let geometry_path = openvsp.vspaero_geometry_path.clone();
    let case_path = geometry_path.with_extension("");
    let setup_path = case_path.with_extension("vspaero");
    let polar_path = case_path.with_extension("polar");
    let stdout_path = case_path.with_extension("vspaero.stdout.txt");
    let stderr_path = case_path.with_extension("vspaero.stderr.txt");
    let mut result = VspaeroAnalysisResult {
        status: VspaeroAnalysisStatus::NotConfigured,
        runtime_executable: executable.map(Path::to_path_buf),
        case_path,
        geometry_path,
        setup_path,
        polar_path,
        stdout_path,
        stderr_path,
        polar: None,
        comparison: VspaeroComparisonStatus::NotEvaluated,
        error: None,
    };
    let Some(executable) = executable else {
        result.error = Some("native VSPAERO executable is not configured".to_owned());
        return result;
    };
    if !result.geometry_path.is_file() {
        result.status = VspaeroAnalysisStatus::GeometryUnavailable;
        result.error = Some(format!(
            "OpenVSP did not produce {}",
            result.geometry_path.display()
        ));
        return result;
    }

    let atmosphere = Atmosphere::new(config.requirements.cruise_altitude_m);
    let speed_m_s = config.requirements.cruise_mach * atmosphere.speed_of_sound();
    let reference = VspaeroReference {
        area_m2: report.airplane.s_ref,
        chord_m: report.airplane.c_ref,
        span_m: report.airplane.b_ref,
        moment_reference_m: report.airplane.xyz_ref,
    };
    let request = VspaeroSweepRequest {
        reference,
        mach: config.requirements.cruise_mach,
        alpha_deg: report.polar.geometric_alpha_deg.clone(),
        beta_deg: 0.0,
        reynolds: atmosphere.density() * speed_m_s * reference.chord_m
            / atmosphere.dynamic_viscosity(),
        speed_m_s,
        density_kg_m3: atmosphere.density(),
        // Keep the historical five-iteration request visible in the native
        // setup, but admit its coefficients only when the retained history
        // proves that the final wake change is below the gate below.
        wake_iterations: 5,
    };
    let setup = match render_setup(&request) {
        Ok(setup) => setup,
        Err(error) => {
            result.status = VspaeroAnalysisStatus::SetupRejected;
            result.error = Some(error.to_string());
            return result;
        }
    };
    if let Err(error) = fs::write(&result.setup_path, &setup) {
        result.status = VspaeroAnalysisStatus::SetupRejected;
        result.error = Some(format!(
            "cannot write {}: {error}",
            result.setup_path.display()
        ));
        return result;
    }

    let process = run_vspaero(executable, &result.case_path, 4, timeout_seconds);
    match process.status {
        VspaeroProcessStatus::InputMissing => {
            result.status = VspaeroAnalysisStatus::GeometryUnavailable;
            result.error = process.error;
            return result;
        }
        VspaeroProcessStatus::InvalidTimeout => {
            result.status = VspaeroAnalysisStatus::InvalidTimeout;
            result.error = process.error;
            return result;
        }
        VspaeroProcessStatus::LaunchFailed => {
            result.status = VspaeroAnalysisStatus::LaunchFailed;
            result.error = process.error;
            return result;
        }
        VspaeroProcessStatus::TimedOut => {
            result.status = VspaeroAnalysisStatus::TimedOut;
            result.error = process.error;
            return result;
        }
        VspaeroProcessStatus::SolverFailed => {
            result.status = VspaeroAnalysisStatus::SolverFailed;
            result.error = process.error;
            return result;
        }
        VspaeroProcessStatus::OutputMissing => {
            result.status = VspaeroAnalysisStatus::OutputMissing;
            result.error = process.error;
            return result;
        }
        VspaeroProcessStatus::Completed => {}
    }

    let polar_text = match fs::read_to_string(&result.polar_path) {
        Ok(text) => text,
        Err(error) => {
            result.status = VspaeroAnalysisStatus::ParseFailed;
            result.error = Some(format!(
                "cannot read {}: {error}",
                result.polar_path.display()
            ));
            return result;
        }
    };
    let polar = match parse_polar(
        &polar_text,
        &setup,
        ReferenceLengthUnit::Meter,
        VspaeroModel::ALAS_VLM,
    ) {
        Ok(polar) => polar,
        Err(error) => {
            result.status = VspaeroAnalysisStatus::ParseFailed;
            result.error = Some(error.to_string());
            return result;
        }
    };
    result.polar = Some(polar.clone());
    let history_path = result.case_path.with_extension("history");
    if let Err(reason) = assess_vspaero_wake_history(&history_path) {
        result.status = VspaeroAnalysisStatus::CompletedNotComparable;
        result.error = Some(reason.clone());
        result.comparison = VspaeroComparisonStatus::Rejected(reason);
        return result;
    }
    let comparison = classify_vspaero_comparison(&polar, report, config);
    match comparison {
        VspaeroComparisonStatus::Compatible(_) => {
            result.status = VspaeroAnalysisStatus::CompletedComparable;
            result.error = None;
        }
        VspaeroComparisonStatus::Rejected(ref reason) => {
            result.status = VspaeroAnalysisStatus::CompletedNotComparable;
            result.error = Some(reason.clone());
        }
        VspaeroComparisonStatus::NotEvaluated => {
            result.status = VspaeroAnalysisStatus::CompletedNotComparable;
            result.error = Some("VSPAERO comparison was not evaluated".to_owned());
        }
    }
    result.comparison = comparison;
    result
}

/// Absolute coefficient-change gate for a native VSPAERO wake history.
///
/// A successful process and a parseable `.polar` do not establish that the
/// free wake reached a fixed point. The history contains one iteration table
/// per alpha case; all three quantities used by the comparison (`CLtot`,
/// `CDi`, and `CMytot`) must change by no more than this tolerance between the
/// final two rows of every case.
const VSPAERO_WAKE_COEFFICIENT_TOLERANCE: f64 = 1.0e-4;

fn assess_vspaero_wake_history(path: &Path) -> Result<(), String> {
    let text = fs::read_to_string(path).map_err(|error| {
        format!(
            "VSPAERO wake convergence history is unavailable at {}: {error}",
            path.display()
        )
    })?;
    assess_vspaero_wake_history_text(&text)
}

fn assess_vspaero_wake_history_text(text: &str) -> Result<(), String> {
    let mut cases: Vec<Vec<[f64; 3]>> = Vec::new();
    let mut current: Option<Vec<[f64; 3]>> = None;
    let mut columns: Option<[usize; 3]> = None;

    let finish_case = |current: &mut Option<Vec<[f64; 3]>>, cases: &mut Vec<Vec<[f64; 3]>>| {
        if let Some(rows) = current.take() {
            if !rows.is_empty() {
                cases.push(rows);
            }
        }
    };

    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("Solver Case:") {
            finish_case(&mut current, &mut cases);
            current = Some(Vec::new());
            columns = None;
            continue;
        }
        if trimmed.starts_with("Iter ") || trimmed == "Iter" {
            let headers = trimmed.split_whitespace().collect::<Vec<_>>();
            let required = ["CLtot", "CDi", "CMytot"];
            let Some(indices) = required
                .map(|name| headers.iter().position(|header| *header == name))
                .into_iter()
                .collect::<Option<Vec<_>>>()
                .and_then(|indices| indices.try_into().ok())
            else {
                return Err("VSPAERO wake history is missing CLtot/CDi/CMytot columns".to_owned());
            };
            columns = Some(indices);
            if current.is_none() {
                current = Some(Vec::new());
            }
            continue;
        }
        let Some(indices) = columns else {
            continue;
        };
        let fields = trimmed.split_whitespace().collect::<Vec<_>>();
        if fields
            .first()
            .and_then(|value| value.parse::<usize>().ok())
            .is_none()
        {
            continue;
        }
        let mut row = [0.0; 3];
        for (slot, &index) in indices.iter().enumerate() {
            let Some(token) = fields.get(index) else {
                return Err("VSPAERO wake history contains a truncated iteration row".to_owned());
            };
            row[slot] = token.parse::<f64>().map_err(|_| {
                format!("VSPAERO wake history contains a malformed coefficient '{token}'")
            })?;
            if !row[slot].is_finite() {
                return Err("VSPAERO wake history contains a non-finite coefficient".to_owned());
            }
        }
        current.get_or_insert_with(Vec::new).push(row);
    }
    finish_case(&mut current, &mut cases);
    if cases.is_empty() {
        return Err("VSPAERO wake history contains no iteration cases".to_owned());
    }
    for (case_index, rows) in cases.iter().enumerate() {
        let Some([previous, final_row]) = rows
            .len()
            .checked_sub(2)
            .map(|index| [rows[index], rows[index + 1]])
        else {
            return Err(format!(
                "VSPAERO wake history case {} has fewer than two iterations",
                case_index + 1
            ));
        };
        let change = previous
            .iter()
            .zip(final_row)
            .map(|(left, right)| (right - left).abs())
            .fold(0.0, f64::max);
        if change > VSPAERO_WAKE_COEFFICIENT_TOLERANCE {
            return Err(format!(
                "VSPAERO wake case {} is not converged: final coefficient change {change:.6e} exceeds {:.6e}",
                case_index + 1,
                VSPAERO_WAKE_COEFFICIENT_TOLERANCE
            ));
        }
    }
    Ok(())
}

/// Classify which parsed VSPAERO quantities share the report's references,
/// geometry scope, frames, and operating-point schedule.
///
/// This pure check is also used when replaying a retained native solve for
/// visual acceptance, so replay cannot manufacture comparability merely by
/// constructing a successful status value.
pub fn classify_vspaero_comparison(
    polar: &VspaeroPolar,
    report: &AnalysisReport,
    config: &AlasConfig,
) -> VspaeroComparisonStatus {
    let mut mismatches = Vec::new();
    let expected = [
        ("Sref", polar.reference.area_m2, report.airplane.s_ref),
        ("Cref", polar.reference.chord_m, report.airplane.c_ref),
        ("Bref", polar.reference.span_m, report.airplane.b_ref),
    ];
    for (name, actual, target) in expected {
        if !close(actual, target) {
            mismatches.push(format!("{name} {actual:.12} != {target:.12}"));
        }
    }
    for (axis, (&actual, &target)) in polar
        .reference
        .moment_reference_m
        .iter()
        .zip(&report.airplane.xyz_ref)
        .enumerate()
    {
        if !close(actual, target) {
            mismatches.push(format!(
                "moment origin axis {axis} {actual:.12} != {target:.12}"
            ));
        }
    }
    if polar.model != VspaeroModel::ALAS_VLM {
        mismatches.push("solver method, geometry scope, or coefficient frames differ".to_owned());
    }
    let geometric_alpha_deg = report.polar.geometric_alpha_deg.clone();
    if polar.points.len() != geometric_alpha_deg.len() {
        mismatches.push(format!(
            "alpha schedule has {} points instead of {}",
            polar.points.len(),
            geometric_alpha_deg.len()
        ));
    } else {
        for (index, (point, &alpha)) in polar.points.iter().zip(&geometric_alpha_deg).enumerate() {
            if !close(point.alpha_deg, alpha) {
                mismatches.push(format!(
                    "alpha[{index}] {:.12} != {alpha:.12}",
                    point.alpha_deg
                ));
                break;
            }
            if !close(point.mach, config.requirements.cruise_mach) {
                mismatches.push(format!(
                    "Mach[{index}] {:.12} != {:.12}",
                    point.mach, config.requirements.cruise_mach
                ));
                break;
            }
            if !close(point.beta_deg, 0.0) {
                mismatches.push(format!("beta[{index}] {:.12} != 0", point.beta_deg));
                break;
            }
        }
    }
    if mismatches.is_empty() {
        VspaeroComparisonStatus::Compatible(vec![
            VspaeroComparableQuantity::LiftCoefficient,
            VspaeroComparableQuantity::PitchingMomentCoefficient,
        ])
    } else {
        VspaeroComparisonStatus::Rejected(mismatches.join("; "))
    }
}

fn close(left: f64, right: f64) -> bool {
    (left - right).abs() <= 1.0e-9 * left.abs().max(right.abs()).max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_text_does_not_collapse_runtime_and_comparison_failures() {
        assert_eq!(
            VspaeroAnalysisStatus::NotConfigured.as_str(),
            "not_configured"
        );
        assert_eq!(VspaeroAnalysisStatus::ParseFailed.as_str(), "parse_failed");
        assert_eq!(
            VspaeroAnalysisStatus::CompletedNotComparable.as_str(),
            "completed_not_comparable"
        );
        assert_eq!(
            VspaeroAnalysisStatus::CompletedComparable.as_str(),
            "completed_comparable"
        );
    }

    fn history(rows: &str) -> String {
        format!("Solver Case: 1\n  Iter Mach AoA Beta CLo CLi CLtot CDo CDi CDtot CMytot\n{rows}")
    }

    #[test]
    fn wake_history_rejects_a_final_change_above_the_coefficient_gate() {
        let text = history(
            "  4 0.8 3 0 0 0 0.50 0 0.020 0 0.010\n\
             5 0.8 3 0 0 0 0.504 0 0.0204 0 0.011\n",
        );
        let error = match assess_vspaero_wake_history_text(&text) {
            Err(error) => error,
            Ok(_) => panic!("the fifth iteration is still changing"),
        };
        assert!(error.contains("case 1 is not converged"), "{error}");
    }

    #[test]
    fn wake_history_accepts_all_cases_when_final_coefficients_are_stable() {
        let text = format!(
            "{}\nSolver Case: 2\n  Iter Mach AoA Beta CLo CLi CLtot CDo CDi CDtot CMytot\n  1 0.8 3 0 0 0 0.50 0 0.020 0 0.010\n  2 0.8 3 0 0 0 0.50001 0 0.02001 0 0.01001\n",
            history(
                "  1 0.8 3 0 0 0 0.50 0 0.020 0 0.010\n\
                 2 0.8 3 0 0 0 0.50001 0 0.02001 0 0.01001\n",
            )
        );
        assert!(assess_vspaero_wake_history_text(&text).is_ok());
    }

    #[test]
    fn wake_history_requires_a_fresh_iteration_table() {
        let error = match assess_vspaero_wake_history_text("Solver Case: 1\n") {
            Err(error) => error,
            Ok(_) => panic!("a headerless history is not convergence evidence"),
        };
        assert!(error.contains("no iteration cases"), "{error}");
    }
}
