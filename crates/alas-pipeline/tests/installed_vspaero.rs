// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Opt-in first-hand evidence for the public OpenVSP-to-VSPAERO product path.

use std::fs;
use std::path::PathBuf;

use alas_config::AlasConfig;
use alas_exec::RunEnvironment;
use alas_pipeline::{
    DesignPipeline, PipelineOptions, VspaeroAnalysisStatus, VspaeroComparisonStatus,
};
use serde_json::json;

#[test]
#[ignore = "requires the installed OpenVSP 3.51.2 vspscript and VSPAERO executables"]
fn installed_product_path_generates_parses_and_classifies_vspaero() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let openvsp = std::env::var_os("ALAS_OPENVSP_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("external tools/OpenVSP-3.51.2-win64/vspscript.exe"));
    let vspaero = std::env::var_os("ALAS_VSPAERO_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("external tools/OpenVSP-3.51.2-win64/vspaero.exe"));
    assert!(openvsp.is_file(), "missing {}", openvsp.display());
    assert!(vspaero.is_file(), "missing {}", vspaero.display());
    let retained = std::env::var_os("ALAS_VSPAERO_OUTPUT").is_some();
    let output = std::env::var_os("ALAS_VSPAERO_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("alas-product-vspaero-{}", std::process::id()))
        });
    let _ = fs::remove_dir_all(&output);

    let mut config = AlasConfig::default();
    config.mission.enabled = false;
    config.structures.enabled = false;
    config.mses.enabled = false;
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: false,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: Some(output.clone()),
        save_plots: false,
        seed: None,
        quiet: true,
    };
    let environment = RunEnvironment {
        openvsp_exe: Some(openvsp.clone()),
        vspaero_exe: Some(vspaero.clone()),
        ..RunEnvironment::default()
    };
    let pipeline = DesignPipeline::new(config)
        .run(&options, &environment)
        .unwrap_or_else(|error| panic!("installed VSPAERO pipeline: {error}"));
    let result = pipeline
        .vspaero_result
        .unwrap_or_else(|| panic!("pipeline did not publish VSPAERO status"));
    assert_eq!(
        result.status,
        VspaeroAnalysisStatus::CompletedComparable,
        "{:?}",
        result.error
    );
    assert!(matches!(
        result.comparison,
        VspaeroComparisonStatus::Compatible(_)
    ));
    let polar = result
        .polar
        .as_ref()
        .unwrap_or_else(|| panic!("completed VSPAERO result has no polar"));
    assert_eq!(
        polar.points.len(),
        pipeline
            .optimized_report
            .as_ref()
            .map_or(0, |report| report.polar.alpha_deg.len())
    );
    assert!(polar.points.iter().all(|point| {
        point.lift_coefficient.is_finite()
            && point.pitching_moment_coefficient.is_finite()
            && point.total_drag_coefficient.is_finite()
    }));
    let artifact_path = |path: &std::path::Path| {
        path.strip_prefix(&output)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    };
    let runtime_path = vspaero
        .strip_prefix(&workspace)
        .unwrap_or(&vspaero)
        .to_string_lossy()
        .replace('\\', "/");

    let summary = json!({
        "runtime": "OpenVSP 3.51.2 / VSPAERO 7.2.2",
        "status": result.status.as_str(),
        "comparison": "CL(alpha) and Cm(alpha) share lifting-surface geometry, SI references, moment origin, frames, Mach, beta, and alpha schedule",
        "not_compared": "VSPAERO drag is inviscid and is not overlaid on the ALAS hybrid total-drag panel",
        "runtime_executable": runtime_path,
        "geometry_path": artifact_path(&result.geometry_path),
        "setup_path": artifact_path(&result.setup_path),
        "polar_path": artifact_path(&result.polar_path),
        "reference_area_m2": polar.reference.area_m2,
        "reference_chord_m": polar.reference.chord_m,
        "reference_span_m": polar.reference.span_m,
        "moment_reference_m": polar.reference.moment_reference_m,
        "alpha_deg": polar.points.iter().map(|point| point.alpha_deg).collect::<Vec<_>>(),
        "mach": polar.points.iter().map(|point| point.mach).collect::<Vec<_>>(),
        "cl": polar.points.iter().map(|point| point.lift_coefficient).collect::<Vec<_>>(),
        "cd_induced": polar.points.iter().map(|point| point.induced_drag_coefficient).collect::<Vec<_>>(),
        "cd_total_vspaero": polar.points.iter().map(|point| point.total_drag_coefficient).collect::<Vec<_>>(),
        "cm_pitch": polar.points.iter().map(|point| point.pitching_moment_coefficient).collect::<Vec<_>>(),
    });
    fs::write(
        output.join("vspaero_runtime_summary.json"),
        serde_json::to_string_pretty(&summary)
            .unwrap_or_else(|error| panic!("encode VSPAERO summary: {error}")),
    )
    .unwrap_or_else(|error| panic!("write VSPAERO summary: {error}"));

    if !retained {
        fs::remove_dir_all(&output)
            .unwrap_or_else(|error| panic!("remove {}: {error}", output.display()));
    }
}
