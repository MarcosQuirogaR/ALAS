// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Build the shared takeoff-condition reference or its typed rejection.

use std::path::Path;

use alas_aero::analysis::AeroAnalysis;
use alas_atmo::Atmosphere;
use alas_config::airports::get as get_airport;
use alas_config::AlasConfig;

use super::{AvlAnalysisResult, AvlAnalysisStatus, AvlComparisonReference, AvlComparisonStatus};
use crate::full_analysis::AnalysisReport;

pub(super) fn takeoff_comparison_reference(
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
        AeroAnalysis::quarter_chord_sweep_deg(&report.airplane, report.design.sweep_deg),
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
    }
}
