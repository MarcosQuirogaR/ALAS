// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Product orchestration for native Athena Vortex Lattice.
//!
//! Deck export, native completion, parsing, and physical comparability are
//! separate states. This keeps an unavailable solver or an out-of-domain
//! Prandtl-Glauert result from silently appearing as another ALAS polar.

use std::fs;
use std::path::{Path, PathBuf};

use alas_aero::analysis::{AeroAnalysis, PolarSweep};
use alas_aero::avl::{parse_total_forces, render_geometry, AvlDeckRequest, AvlModel, AvlPolar};
use alas_atmo::Atmosphere;
use alas_config::airports::get as get_airport;
use alas_config::AlasConfig;
use alas_exec::avl::{run_avl, AvlProcessStatus};

use crate::full_analysis::AnalysisReport;

/// Product-level state of a requested native AVL analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AvlAnalysisStatus {
    /// Geometry was exported but no solver executable was available.
    NotConfigured,
    /// The aircraft could not be represented by the declared AVL deck scope.
    DeckRejected,
    /// The executable could not be launched.
    LaunchFailed,
    /// The native process exceeded its deadline.
    TimedOut,
    /// AVL returned a failing process status.
    SolverFailed,
    /// AVL returned success without all fresh force files.
    OutputMissing,
    /// A native force file violated the parser contract.
    ParseFailed,
    /// Native results exist but may not be overlaid with the ALAS model.
    CompletedNotComparable,
    /// Native results and ALAS share the admitted references and frames.
    CompletedComparable,
}

impl AvlAnalysisStatus {
    /// Stable status text for UI and retained evidence.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotConfigured => "not_configured",
            Self::DeckRejected => "deck_rejected",
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

/// AVL quantities safe to overlay after the complete contract check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AvlComparableQuantity {
    /// Stability-axis whole-aircraft lift coefficient.
    LiftCoefficient,
    /// Pitching-moment coefficient about the common physical CG.
    PitchingMomentCoefficient,
}

/// Why parsed AVL results are or are not comparable with ALAS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AvlComparisonStatus {
    /// No parsed polar exists.
    NotEvaluated,
    /// At least one reference, frame, schedule, or validity check failed.
    Rejected(String),
    /// The listed quantities share physical definitions.
    Compatible(Vec<AvlComparableQuantity>),
}

/// ALAS VLM sweep evaluated at the same flight condition as an AVL run.
#[derive(Debug, Clone, PartialEq)]
pub struct AvlComparisonReference {
    /// Human-readable phase represented by the comparison condition.
    pub phase: String,
    /// Freestream Mach number supplied to both solvers.
    pub mach: f64,
    /// Geopotential altitude used by the ALAS atmosphere, in meters.
    pub altitude_m: f64,
    /// ALAS VLM polar whose coefficient rows correspond to the schedule below;
    /// its public `alpha_deg` axis is the reporting/display axis.
    pub vlm_polar: PolarSweep,
    /// Geometric alpha values actually sent to AVL. `vlm_polar.alpha_deg`
    /// remains the display/reporting axis after its PG relabeling.
    pub geometric_alpha_deg: Vec<f64>,
}

/// Files, parsed output, and status from one native AVL stage.
#[derive(Debug, Clone, PartialEq)]
pub struct AvlAnalysisResult {
    /// Explicit product status.
    pub status: AvlAnalysisStatus,
    /// Executable used, when a native run was attempted.
    pub runtime_executable: Option<PathBuf>,
    /// Rust-written native AVL geometry deck.
    pub geometry_path: PathBuf,
    /// Retained interactive command stream.
    pub session_path: PathBuf,
    /// One native total-force file per angle of attack.
    pub force_paths: Vec<PathBuf>,
    /// Captured standard output.
    pub stdout_path: PathBuf,
    /// Captured standard error.
    pub stderr_path: PathBuf,
    /// Parsed polar, retained even when comparison is rejected.
    pub polar: Option<AvlPolar>,
    /// Same-condition ALAS VLM result used by the comparison contract.
    pub comparison_reference: Option<AvlComparisonReference>,
    /// Quantity-level model-comparison contract.
    pub comparison: AvlComparisonStatus,
    /// Actionable failure or incompatibility detail.
    pub error: Option<String>,
}

impl AvlAnalysisResult {
    /// Return a polar only when it is admitted to whole-aircraft overlays.
    pub fn comparable_polar(&self) -> Option<&AvlPolar> {
        matches!(self.comparison, AvlComparisonStatus::Compatible(_))
            .then_some(())
            .and(self.polar.as_ref())
    }
}

/// Export, run, parse, and classify one native AVL sweep.
pub fn run_avl_analysis(
    report: &AnalysisReport,
    config: &AlasConfig,
    output_dir: &Path,
    executable: Option<&Path>,
    timeout_seconds: f64,
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
    )
}

fn run_avl_analysis_with_reference(
    report: &AnalysisReport,
    config: &AlasConfig,
    output_dir: &Path,
    executable: Option<&Path>,
    timeout_seconds: f64,
    reference: AvlComparisonReference,
) -> AvlAnalysisResult {
    let geometry_path = output_dir.join("avl/optimized_aircraft.avl");
    let base = geometry_path.with_extension("");
    let mut result = AvlAnalysisResult {
        status: AvlAnalysisStatus::NotConfigured,
        runtime_executable: executable.map(Path::to_path_buf),
        geometry_path: geometry_path.clone(),
        session_path: base.with_extension("avl.session.txt"),
        force_paths: Vec::new(),
        stdout_path: base.with_extension("avl.stdout.txt"),
        stderr_path: base.with_extension("avl.stderr.txt"),
        polar: None,
        comparison_reference: Some(reference.clone()),
        comparison: AvlComparisonStatus::NotEvaluated,
        error: None,
    };
    let parent = geometry_path.parent().unwrap_or(output_dir);
    if let Err(error) = fs::create_dir_all(parent) {
        result.status = AvlAnalysisStatus::DeckRejected;
        result.error = Some(format!("cannot create {}: {error}", parent.display()));
        return result;
    }
    let chordwise_vortices = positive_usize(config.analysis.fine_chordwise_resolution).max(8);
    let spanwise_vortices = positive_usize(config.analysis.fine_spanwise_resolution)
        .saturating_mul(10)
        .max(20);
    let deck = match render_geometry(AvlDeckRequest {
        airplane: &report.airplane,
        mach: reference.mach,
        chordwise_vortices,
        spanwise_vortices,
    }) {
        Ok(deck) => deck,
        Err(error) => {
            result.status = AvlAnalysisStatus::DeckRejected;
            result.error = Some(error.to_string());
            return result;
        }
    };
    if let Err(error) = fs::write(&geometry_path, deck) {
        result.status = AvlAnalysisStatus::DeckRejected;
        result.error = Some(format!("cannot write {}: {error}", geometry_path.display()));
        return result;
    }
    let Some(executable) = executable else {
        result.error =
            Some("native AVL executable is not configured; geometry deck exported".to_owned());
        return result;
    };
    let process = run_avl(
        executable,
        &geometry_path,
        &reference.geometric_alpha_deg,
        timeout_seconds,
    );
    result.session_path = process.session_path;
    result.force_paths = process.force_paths;
    result.stdout_path = process.stdout_path;
    result.stderr_path = process.stderr_path;
    match process.status {
        AvlProcessStatus::InputMissing => {
            result.status = AvlAnalysisStatus::DeckRejected;
            result.error = process.error;
            return result;
        }
        AvlProcessStatus::LaunchFailed => {
            result.status = AvlAnalysisStatus::LaunchFailed;
            result.error = process.error;
            return result;
        }
        AvlProcessStatus::TimedOut => {
            result.status = AvlAnalysisStatus::TimedOut;
            result.error = process.error;
            return result;
        }
        AvlProcessStatus::SolverFailed => {
            result.status = AvlAnalysisStatus::SolverFailed;
            result.error = process.error;
            return result;
        }
        AvlProcessStatus::OutputMissing => {
            result.status = AvlAnalysisStatus::OutputMissing;
            result.error = process.error;
            return result;
        }
        AvlProcessStatus::Completed => {}
    }
    let texts = match result
        .force_paths
        .iter()
        .map(fs::read_to_string)
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(texts) => texts,
        Err(error) => {
            result.status = AvlAnalysisStatus::ParseFailed;
            result.error = Some(format!("cannot read native AVL force output: {error}"));
            return result;
        }
    };
    let views = texts.iter().map(String::as_str).collect::<Vec<_>>();
    let polar = match parse_total_forces(&views, AvlModel::ALAS_LIFTING_SURFACES) {
        Ok(polar) => polar,
        Err(error) => {
            result.status = AvlAnalysisStatus::ParseFailed;
            result.error = Some(error.to_string());
            return result;
        }
    };
    let comparison = classify_avl_comparison(&polar, report, &reference);
    result.polar = Some(polar);
    match &comparison {
        AvlComparisonStatus::Compatible(_) => {
            result.status = AvlAnalysisStatus::CompletedComparable;
            result.error = None;
        }
        AvlComparisonStatus::Rejected(reason) => {
            result.status = AvlAnalysisStatus::CompletedNotComparable;
            result.error = Some(reason.clone());
        }
        AvlComparisonStatus::NotEvaluated => {
            result.status = AvlAnalysisStatus::CompletedNotComparable;
            result.error = Some("AVL comparison was not evaluated".to_owned());
        }
    }
    result.comparison = comparison;
    result
}

/// Admit only AVL quantities with shared geometry scope, frames, references,
/// schedule, and a Prandtl-Glauert Mach normal to the wing below 0.7.
pub fn classify_avl_comparison(
    polar: &AvlPolar,
    report: &AnalysisReport,
    reference: &AvlComparisonReference,
) -> AvlComparisonStatus {
    let mut mismatches = Vec::new();
    for (name, actual, target) in [
        ("Sref", polar.reference.area_m2, report.airplane.s_ref),
        ("Cref", polar.reference.chord_m, report.airplane.c_ref),
        ("Bref", polar.reference.span_m, report.airplane.b_ref),
    ] {
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
    if polar.model != AvlModel::ALAS_LIFTING_SURFACES {
        mismatches.push("AVL geometry scope or coefficient frames differ".to_owned());
    }
    if polar.points.len() != reference.geometric_alpha_deg.len() {
        mismatches.push(format!(
            "alpha schedule has {} points instead of {}",
            polar.points.len(),
            reference.geometric_alpha_deg.len()
        ));
    } else {
        for (index, (point, &alpha)) in polar
            .points
            .iter()
            .zip(&reference.geometric_alpha_deg)
            .enumerate()
        {
            if !close(point.alpha_deg, alpha) {
                mismatches.push(format!(
                    "alpha[{index}] {:.12} != {alpha:.12}",
                    point.alpha_deg
                ));
                break;
            }
            if !close(point.mach, reference.mach) {
                mismatches.push(format!(
                    "Mach[{index}] {:.12} != {:.12}",
                    point.mach, reference.mach
                ));
                break;
            }
            if !close(point.beta_deg, 0.0) {
                mismatches.push(format!("beta[{index}] {:.12} != 0", point.beta_deg));
                break;
            }
        }
    }
    let wing_normal_mach = wing_normal_mach(reference.mach, report.design.sweep_deg);
    if !pg_domain_supported(wing_normal_mach) {
        mismatches.push(format!(
            "wing-normal Mach {wing_normal_mach:.3} is outside AVL's documented Prandtl-Glauert expected-valid range (<0.7)"
        ));
    }
    if mismatches.is_empty() {
        AvlComparisonStatus::Compatible(vec![
            AvlComparableQuantity::LiftCoefficient,
            AvlComparableQuantity::PitchingMomentCoefficient,
        ])
    } else {
        AvlComparisonStatus::Rejected(mismatches.join("; "))
    }
}

fn takeoff_comparison_reference(
    report: &AnalysisReport,
    config: &AlasConfig,
) -> Result<AvlComparisonReference, String> {
    let airport = get_airport(&config.departure_airport)
        .map_err(|error| format!("takeoff comparison airport is unavailable: {error}"))?;
    let altitude_m = airport.elevation_m + 0.5 * config.mission.profile.takeoff_altitude_gain_m;
    let atmosphere = Atmosphere::new(altitude_m);
    let speed_of_sound = atmosphere.speed_of_sound();
    let airspeed = config.mission.profile.takeoff_air_speed_m_s;
    if !airspeed.is_finite()
        || airspeed <= 0.0
        || !speed_of_sound.is_finite()
        || speed_of_sound <= 0.0
    {
        return Err(
            "takeoff comparison requires finite positive airspeed and atmosphere".to_owned(),
        );
    }
    let mach = airspeed / speed_of_sound;
    let mut analysis = config.analysis.clone();
    analysis.spanwise_resolution = analysis.fine_spanwise_resolution;
    analysis.chordwise_resolution = analysis.fine_chordwise_resolution;
    let vlm_polar = AeroAnalysis::new(
        &report.airplane,
        report.design.sweep_deg,
        Some(config.geometry.clone()),
        Some(config.drag_model.clone()),
        Some(analysis),
    )
    .run_sweep(mach, altitude_m)
    .map_err(|error| format!("takeoff-condition ALAS VLM sweep failed: {error}"))?;
    let geometric_alpha_deg = vlm_polar.geometric_alpha_deg.clone();
    Ok(AvlComparisonReference {
        phase: "takeoff climb midpoint".to_owned(),
        mach,
        altitude_m,
        vlm_polar,
        geometric_alpha_deg,
    })
}

fn rejected_reference_result(
    output_dir: &Path,
    executable: Option<&Path>,
    error: String,
) -> AvlAnalysisResult {
    let geometry_path = output_dir.join("avl/optimized_aircraft.avl");
    let base = geometry_path.with_extension("");
    AvlAnalysisResult {
        status: AvlAnalysisStatus::DeckRejected,
        runtime_executable: executable.map(Path::to_path_buf),
        geometry_path,
        session_path: base.with_extension("avl.session.txt"),
        force_paths: Vec::new(),
        stdout_path: base.with_extension("avl.stdout.txt"),
        stderr_path: base.with_extension("avl.stderr.txt"),
        polar: None,
        comparison_reference: None,
        comparison: AvlComparisonStatus::NotEvaluated,
        error: Some(error),
    }
}

fn positive_usize(value: i64) -> usize {
    usize::try_from(value).unwrap_or(0)
}

fn wing_normal_mach(mach: f64, sweep_deg: f64) -> f64 {
    mach * sweep_deg.to_radians().cos().abs()
}

fn pg_domain_supported(wing_normal_mach: f64) -> bool {
    wing_normal_mach.is_finite() && wing_normal_mach < 0.7
}

fn close(left: f64, right: f64) -> bool {
    (left - right).abs() <= 1.0e-9 * left.abs().max(right.abs()).max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_text_keeps_runtime_and_comparison_failures_distinct() {
        assert_eq!(AvlAnalysisStatus::NotConfigured.as_str(), "not_configured");
        assert_eq!(AvlAnalysisStatus::TimedOut.as_str(), "timed_out");
        assert_eq!(AvlAnalysisStatus::ParseFailed.as_str(), "parse_failed");
        assert_eq!(
            AvlAnalysisStatus::CompletedNotComparable.as_str(),
            "completed_not_comparable"
        );
        assert_eq!(
            AvlAnalysisStatus::CompletedComparable.as_str(),
            "completed_comparable"
        );
    }

    #[test]
    fn prandtl_glauert_gate_uses_mach_normal_to_the_swept_wing() {
        assert!(pg_domain_supported(wing_normal_mach(0.84, 45.0)));
        assert!(!pg_domain_supported(wing_normal_mach(0.84, 30.0)));
        assert!(!pg_domain_supported(f64::NAN));
    }

    #[test]
    fn configured_takeoff_comparison_stays_inside_the_avl_pg_domain() {
        let config = AlasConfig::default();
        let airport = get_airport(&config.departure_airport)
            .unwrap_or_else(|error| panic!("default departure airport: {error}"));
        let altitude_m = airport.elevation_m + 0.5 * config.mission.profile.takeoff_altitude_gain_m;
        let mach = config.mission.profile.takeoff_air_speed_m_s
            / Atmosphere::new(altitude_m).speed_of_sound();
        assert!(pg_domain_supported(wing_normal_mach(
            mach,
            alas_config::design_variables::DesignVector::default().sweep_deg
        )));
    }
}
