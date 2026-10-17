// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Build the shared comparison-condition references (take-off and cruise) or
//! their typed rejection, and verify a reference against its stated phase.

use std::path::Path;

use alas_aero::analysis::AeroAnalysis;
use alas_atmo::Atmosphere;
use alas_config::airports::get as get_airport;
use alas_config::AlasConfig;

use super::{AvlAnalysisResult, AvlAnalysisStatus, AvlComparisonReference, AvlComparisonStatus};
use crate::full_analysis::AnalysisReport;

/// Phase label of the take-off-climb comparison reference.
pub(super) const PHASE_TAKEOFF: &str = "takeoff climb midpoint";
/// Phase label of a comparison at the full configured cruise Mach.
pub(super) const PHASE_CRUISE: &str = "cruise";
/// Phase label of a cruise comparison whose Mach was lowered to the highest
/// value inside AVL's linear (Prandtl-Glauert) validity domain.
pub(super) const PHASE_CRUISE_LINEAR: &str = "cruise (linear-valid Mach)";

/// Wing-normal Mach at and above which AVL's Prandtl-Glauert correction is
/// outside its documented expected-valid range (the existing admission gate;
/// AVL's documentation states PG for M below about 0.7).
pub(super) const PG_NORMAL_MACH_LIMIT: f64 = 0.7;

/// Wing-normal Mach of the capped comparison. It sits just inside
/// [`PG_NORMAL_MACH_LIMIT`] so that rounding in the quarter-chord sweep can
/// never flip the admission gate; it is the "highest Mach where linear theory
/// is valid" to within 1.5 % of the limit.
pub(super) const LINEAR_VALID_NORMAL_MACH: f64 = 0.69;

/// Highest freestream Mach inside AVL's linear-valid domain for a wing with
/// quarter-chord sweep `sweep_deg` (degrees), capped at the cruise Mach. The
/// wing-normal Mach is `M cos(sweep)`, so the cap is
/// `LINEAR_VALID_NORMAL_MACH / cos(sweep)`.
pub(super) fn linear_valid_mach(cruise_mach: f64, sweep_deg: f64) -> f64 {
    let cos_sweep = sweep_deg.to_radians().cos().abs();
    if cos_sweep <= f64::EPSILON {
        return LINEAR_VALID_NORMAL_MACH;
    }
    cruise_mach.min(LINEAR_VALID_NORMAL_MACH / cos_sweep)
}

fn quarter_chord_sweep_deg(report: &AnalysisReport) -> f64 {
    AeroAnalysis::quarter_chord_sweep_deg(&report.airplane, report.design.sweep_deg)
}

/// Take-off-climb midpoint Mach and altitude (m) from the configuration.
fn takeoff_condition(config: &AlasConfig) -> Result<(f64, f64), String> {
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
    Ok((airspeed / speed_of_sound, altitude_m))
}

/// Cruise comparison condition: the configured cruise altitude and the
/// highest Mach not above cruise whose wing-normal Mach stays inside the
/// linear-valid domain, with its phase label.
fn cruise_condition(report: &AnalysisReport, config: &AlasConfig) -> (f64, f64, &'static str) {
    let cruise_mach = config.requirements.cruise_mach;
    let mach = linear_valid_mach(cruise_mach, quarter_chord_sweep_deg(report));
    let phase = if mach < cruise_mach {
        PHASE_CRUISE_LINEAR
    } else {
        PHASE_CRUISE
    };
    (mach, config.requirements.cruise_altitude_m, phase)
}

/// Why `reference` is not the condition its phase label states, if it is not.
///
/// The classifier checks AVL against the reference it was handed, so without
/// this check a take-off Mach could pass as "cruise". The expected condition
/// is rebuilt here from the configuration.
pub(super) fn reference_condition_mismatch(
    reference: &AvlComparisonReference,
    report: &AnalysisReport,
    config: &AlasConfig,
) -> Option<String> {
    let expected = match reference.phase.as_str() {
        PHASE_TAKEOFF => takeoff_condition(config).ok(),
        PHASE_CRUISE => Some((
            config.requirements.cruise_mach,
            config.requirements.cruise_altitude_m,
        )),
        PHASE_CRUISE_LINEAR => {
            let (mach, altitude_m, _) = cruise_condition(report, config);
            Some((mach, altitude_m))
        }
        other => return Some(format!("unknown comparison phase '{other}'")),
    };
    let Some((mach, altitude_m)) = expected else {
        return Some(format!(
            "the {} condition cannot be rebuilt from the configuration",
            reference.phase
        ));
    };
    condition_difference(
        &reference.phase,
        (reference.mach, reference.altitude_m),
        (mach, altitude_m),
    )
}

/// A message when `actual` (Mach, altitude in m) is not `expected` for
/// `phase`, otherwise `None`.
fn condition_difference(phase: &str, actual: (f64, f64), expected: (f64, f64)) -> Option<String> {
    if super::close(actual.0, expected.0) && super::close(actual.1, expected.1) {
        return None;
    }
    Some(format!(
        "reference Mach {:.4} at {:.0} m is not the {phase} condition (Mach {:.4} at {:.0} m)",
        actual.0, actual.1, expected.0, expected.1
    ))
}

/// ALAS VLM sweep at `mach` and `altitude_m`, with the fine mesh.
fn vlm_reference_sweep(
    report: &AnalysisReport,
    config: &AlasConfig,
    mach: f64,
    altitude_m: f64,
    label: &str,
) -> Result<alas_aero::analysis::PolarSweep, String> {
    let mut analysis = config.analysis.clone();
    analysis.spanwise_resolution = analysis.fine_spanwise_resolution;
    analysis.chordwise_resolution = analysis.fine_chordwise_resolution;
    let vlm_polar = AeroAnalysis::new(
        &report.airplane,
        quarter_chord_sweep_deg(report),
        Some(config.geometry.clone()),
        Some(config.drag_model.clone()),
        Some(analysis),
    )
    .run_sweep(mach, altitude_m)
    .map_err(|error| format!("{label}-condition ALAS VLM sweep failed: {error}"))?;
    Ok(vlm_polar)
}

pub(super) fn takeoff_comparison_reference(
    report: &AnalysisReport,
    config: &AlasConfig,
) -> Result<AvlComparisonReference, String> {
    let (mach, altitude_m) = takeoff_condition(config)?;
    let vlm_polar = vlm_reference_sweep(report, config, mach, altitude_m, "takeoff")?;
    let geometric_alpha_deg = vlm_polar.geometric_alpha_deg.clone();
    Ok(AvlComparisonReference {
        phase: PHASE_TAKEOFF.to_owned(),
        mach,
        altitude_m,
        vlm_polar,
        geometric_alpha_deg,
    })
}

/// The cruise comparison reference: cruise altitude, and the highest Mach not
/// above the configured cruise Mach whose wing-normal Mach is inside AVL's
/// linear-valid domain. Transonic cruise accuracy is not decided here; it is
/// checked against MSES and published data.
pub(super) fn linear_cruise_comparison_reference(
    report: &AnalysisReport,
    config: &AlasConfig,
) -> Result<AvlComparisonReference, String> {
    let (mach, altitude_m, phase) = cruise_condition(report, config);
    let vlm_polar = vlm_reference_sweep(report, config, mach, altitude_m, "cruise")?;
    // At cruise Mach the VLM lift `cl[k]` is the incompressible value at the
    // geometric angle, reported at the compressibility-corrected angle
    // `alpha_deg[k]` (`compressible_report_alpha`). AVL applies its own
    // Prandtl-Glauert correction at the physical angle it is given, so it is
    // run at the reporting axis: the pair `(alpha_deg[k], cl[k])` and AVL at
    // `alpha_deg[k]` are the same physical state. At the geometric angles the
    // two differ by about 1/beta (A320 at Mach 0.76: CL 0.45 against 0.29 at
    // zero angle). The take-off comparison keeps the geometric axis, where
    // beta is within 3 percent of 1.
    let geometric_alpha_deg = vlm_polar.alpha_deg.clone();
    Ok(AvlComparisonReference {
        phase: phase.to_owned(),
        mach,
        altitude_m,
        vlm_polar,
        geometric_alpha_deg,
    })
}

pub(super) fn rejected_reference_result(
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
        linear_cruise: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_swept_wing_is_capped_at_the_highest_linear_valid_mach() {
        // 35 deg quarter-chord sweep: 0.69 / cos(35 deg) = 0.842 > cruise 0.78.
        assert_eq!(linear_valid_mach(0.78, 35.0), 0.78);
        // 20 deg: the wing-normal Mach limit caps the comparison below cruise.
        let capped = linear_valid_mach(0.78, 20.0);
        assert!(capped < 0.78);
        assert!((capped * 20.0_f64.to_radians().cos() - LINEAR_VALID_NORMAL_MACH).abs() < 1e-12);
        // The capped Mach is inside the admission gate, never at its edge.
        assert!(capped * 20.0_f64.to_radians().cos() < PG_NORMAL_MACH_LIMIT);
        // A straight wing never exceeds the limit either.
        assert!(linear_valid_mach(0.78, 0.0) < PG_NORMAL_MACH_LIMIT);
    }

    #[test]
    fn a_takeoff_mach_cannot_pass_as_a_cruise_reference() {
        let cruise = (0.78, 11_000.0);
        assert!(condition_difference(PHASE_CRUISE, (0.23, 100.0), cruise).is_some());
        assert!(condition_difference(PHASE_CRUISE, cruise, cruise).is_none());
        // Right Mach at the wrong altitude is also a different condition.
        assert!(condition_difference(PHASE_CRUISE, (0.78, 100.0), cruise).is_some());
    }
}
