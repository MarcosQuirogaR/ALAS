// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Replay the retained installed VSPAERO solve in the model-comparison view.

use alas_aero::fourier_lifting_line::AircraftFourierLiftingLine;
use alas_aero::vspaero::{parse_polar, ReferenceLengthUnit, VspaeroModel};
use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_pipeline::full_analysis::FullAnalysis;
use alas_pipeline::{
    classify_vspaero_comparison, VspaeroAnalysisResult, VspaeroAnalysisStatus,
    VspaeroComparisonStatus,
};
use alas_report::families::aerodynamics::figure_model_comparison;
use alas_report::svg::render_svg;
use std::error::Error;
use std::fs;
use std::io::{Error as IoError, ErrorKind};
use std::path::PathBuf;

const FOURIER_HARMONICS: usize = 12;

fn main() -> Result<(), Box<dyn Error>> {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let case_path = workspace.join("outputs/vspaero_runtime_audit/openvsp/optimized_aircraft");
    let setup_path = case_path.with_extension("vspaero");
    let polar_path = case_path.with_extension("polar");
    let setup = fs::read_to_string(&setup_path)?;
    let polar_text = fs::read_to_string(&polar_path)?;
    let polar = parse_polar(
        &polar_text,
        &setup,
        ReferenceLengthUnit::Meter,
        VspaeroModel::ALAS_VLM,
    )?;

    let config = AlasConfig::default();
    let report = FullAnalysis::new(config.clone())
        .run(&DesignVector::default(), true)
        .map_err(|error| IoError::new(ErrorKind::InvalidData, error))?;
    let fourier_model =
        AircraftFourierLiftingLine::from_airplane(&report.airplane, FOURIER_HARMONICS)?;
    let fourier_points = fourier_model.sweep(
        &report
            .polar
            .alpha_deg
            .iter()
            .map(|alpha| alpha.to_radians())
            .collect::<Vec<_>>(),
    )?;
    let fourier_summary = serde_json::json!({
        "method": "odd-Fourier Prandtl lifting-line",
        "vortex_lattice": false,
        "harmonic_count": FOURIER_HARMONICS,
        "reference_area_m2": fourier_model.reference_area_m2,
        "surface_names": fourier_model
            .surfaces
            .iter()
            .map(|surface| surface.name.as_str())
            .collect::<Vec<_>>(),
        "alpha_deg": fourier_points
            .iter()
            .map(|point| point.alpha_rad.to_degrees())
            .collect::<Vec<_>>(),
        "lift_coefficient": fourier_points
            .iter()
            .map(|point| point.lift_coefficient)
            .collect::<Vec<_>>(),
        "induced_drag_coefficient": fourier_points
            .iter()
            .map(|point| point.induced_drag_coefficient)
            .collect::<Vec<_>>(),
        "surface_span_efficiency": fourier_points
            .iter()
            .map(|point| point
                .surfaces
                .iter()
                .map(|surface| surface.span_efficiency)
                .collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        "comparison_scope": "CL(alpha) only; CDi retained but not overlaid on ALAS total CD",
    });
    fs::write(
        workspace.join("outputs/vspaero_runtime_audit/fourier_lifting_line_summary.json"),
        serde_json::to_string_pretty(&fourier_summary)?,
    )?;
    let comparison = classify_vspaero_comparison(&polar, &report, &config);
    if !matches!(comparison, VspaeroComparisonStatus::Compatible(_)) {
        return Err(IoError::new(
            ErrorKind::InvalidData,
            format!("retained VSPAERO solve is not comparable: {comparison:?}"),
        )
        .into());
    }
    let vspaero = VspaeroAnalysisResult {
        status: VspaeroAnalysisStatus::CompletedComparable,
        runtime_executable: Some(workspace.join("external tools/OpenVSP-3.51.2-win64/vspaero.exe")),
        geometry_path: case_path.with_extension("vspgeom"),
        setup_path,
        polar_path,
        stdout_path: case_path.with_extension("vspaero.stdout.txt"),
        stderr_path: case_path.with_extension("vspaero.stderr.txt"),
        case_path,
        polar: Some(polar),
        comparison,
        error: None,
    };
    let scene = figure_model_comparison(&report, None, None, Some(&vspaero), None, Some("dark"));
    let output = workspace.join("outputs/vspaero_runtime_audit/model_comparison.svg");
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output, render_svg(&scene))?;
    Ok(())
}
