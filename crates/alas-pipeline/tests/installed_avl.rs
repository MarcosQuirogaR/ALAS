// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Opt-in installed Athena Vortex Lattice product-path evidence.

use std::fs;
use std::path::PathBuf;

use alas_config::AlasConfig;
use alas_exec::RunEnvironment;
use alas_pipeline::{AvlAnalysisStatus, AvlComparisonStatus, DesignPipeline, PipelineOptions};

#[test]
#[ignore = "requires ALAS_AVL_EXE pointing to an installed native AVL executable"]
fn installed_avl_generates_and_parses_one_force_file_per_requested_alpha() {
    let executable = std::env::var_os("ALAS_AVL_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("ALAS_AVL_EXE is required"));
    assert!(executable.is_file(), "missing {}", executable.display());
    let output = std::env::var_os("ALAS_AVL_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("alas-product-avl-{}", std::process::id()))
        });
    fs::create_dir_all(&output)
        .unwrap_or_else(|error| panic!("create {}: {error}", output.display()));
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
        output_dir: Some(output),
        save_plots: false,
        seed: None,
        quiet: true,
    };
    let environment = RunEnvironment {
        avl_exe: Some(executable),
        ..RunEnvironment::default()
    };

    let pipeline = DesignPipeline::new(config)
        .run(&options, &environment)
        .unwrap_or_else(|error| panic!("installed AVL pipeline: {error}"));
    let result = pipeline
        .avl_result
        .unwrap_or_else(|| panic!("pipeline did not publish AVL status"));
    assert_eq!(
        result.status,
        AvlAnalysisStatus::CompletedComparable,
        "status={} error={:?}",
        result.status.as_str(),
        result.error
    );
    assert!(matches!(
        result.comparison,
        AvlComparisonStatus::Compatible(_)
    ));
    let polar = result
        .polar
        .unwrap_or_else(|| panic!("completed AVL run did not publish a polar"));
    let report = pipeline
        .optimized_report
        .unwrap_or_else(|| panic!("pipeline did not publish the analyzed report"));
    assert_eq!(polar.points.len(), report.polar.alpha_deg.len());
    assert_eq!(result.force_paths.len(), polar.points.len());
    assert!(result.force_paths.iter().all(|path| path.is_file()));
    assert!(polar.points.iter().all(|point| {
        point.alpha_deg.is_finite()
            && point.lift_coefficient.is_finite()
            && point.pitching_moment_coefficient.is_finite()
    }));
}
