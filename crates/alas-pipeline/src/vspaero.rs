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
        alpha_deg: report.polar.alpha_deg.clone(),
        beta_deg: 0.0,
        reynolds: atmosphere.density() * speed_m_s * reference.chord_m
            / atmosphere.dynamic_viscosity(),
        speed_m_s,
        density_kg_m3: atmosphere.density(),
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
    let comparison = classify_vspaero_comparison(&polar, report, config);
    result.polar = Some(polar);
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
    if polar.points.len() != report.polar.alpha_deg.len() {
        mismatches.push(format!(
            "alpha schedule has {} points instead of {}",
            polar.points.len(),
            report.polar.alpha_deg.len()
        ));
    } else {
        for (index, (point, &alpha)) in polar.points.iter().zip(&report.polar.alpha_deg).enumerate()
        {
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
}
