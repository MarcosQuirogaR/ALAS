// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native AVL absence remains explicit while its inspectable deck is exported.

use std::fs;

use alas_config::AlasConfig;
use alas_exec::RunEnvironment;
use alas_pipeline::{AvlAnalysisStatus, DesignPipeline, PipelineOptions};

#[test]
fn output_enabled_pipeline_exports_avl_geometry_without_claiming_a_solver_run() {
    let output = std::env::temp_dir().join(format!(
        "alas-avl-status-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
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

    let pipeline = DesignPipeline::new(config)
        .run(&options, &RunEnvironment::default())
        .unwrap_or_else(|error| panic!("AVL absence pipeline: {error}"));
    let result = pipeline
        .avl_result
        .unwrap_or_else(|| panic!("output-enabled pipeline must publish AVL status"));
    assert_eq!(result.status, AvlAnalysisStatus::NotConfigured);
    assert!(result.geometry_path.is_file());
    assert!(result.polar.is_none());
    assert!(result.error.as_deref().is_some_and(|error| {
        error.contains("not configured") && error.contains("deck exported")
    }));
    let deck = fs::read_to_string(&result.geometry_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", result.geometry_path.display()));
    assert!(deck.contains("SURFACE\nMain Wing"));
    assert!(deck.contains("YDUPLICATE\n0.0"));
    assert!(deck.contains("AIRFOIL"));

    fs::remove_dir_all(&output)
        .unwrap_or_else(|error| panic!("remove {}: {error}", output.display()));
}
