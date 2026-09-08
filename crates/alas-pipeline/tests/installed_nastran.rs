// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Opt-in installed-MSC-Nastran smoke evidence for the public pipeline path.
//!
//! The regular suite covers deck construction, parser behavior and every
//! missing/timeout failure outcome without requiring a commercial install. This
//! test is deliberately ignored: it proves the remaining boundary that only a
//! real licensed solver can prove.

use std::path::PathBuf;

use alas_config::AlasConfig;
use alas_exec::RunEnvironment;
use alas_pipeline::{DesignPipeline, PipelineOptions};
use alas_struct::nastran::ResultStatus;

fn smoke_output_dir() -> PathBuf {
    std::env::temp_dir().join(format!("alas-installed-nastran-{}", std::process::id()))
}

#[test]
#[ignore = "requires ALAS_MSC_NASTRAN to name a licensed installed solver"]
fn installed_msc_nastran_sol101_runs_through_the_public_pipeline() {
    let executable = std::env::var_os("ALAS_MSC_NASTRAN")
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set ALAS_MSC_NASTRAN to the MSC Nastran launcher"));
    assert!(
        executable.is_file(),
        "solver does not exist: {}",
        executable.display()
    );

    let mut config = AlasConfig::default();
    if let Some(solver) = std::env::var_os("ALAS_MSC_SOLVER").map(PathBuf::from) {
        assert!(
            solver.is_file(),
            "ALAS_MSC_SOLVER does not exist: {}",
            solver.display()
        );
        config.structures.nastran_solver_path = solver.display().to_string();
    }
    config.mission.enabled = false;
    config.structures.run_sol_static = true;
    config.structures.run_sol_modes = false;
    config.structures.run_sol_vibration_sine = false;
    config.structures.run_sol_vibration_random = false;
    config.structures.run_patran_export = false;
    config.structures.timeout_s = 300.0;
    let output_dir = smoke_output_dir();
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: false,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: Some(output_dir.clone()),
        save_plots: false,
        seed: None,
        quiet: true,
    };
    let environment = RunEnvironment {
        mses_dir: None,
        nastran_exe: Some(executable),
        nastran_solver: None,
        patran_exe: None,
        openvsp_exe: None,
        vspaero_exe: None,
        avl_exe: None,
        flowunsteady_exe: None,
    };

    let result = DesignPipeline::new(config)
        .run(&options, &environment)
        .unwrap_or_else(|error| panic!("public pipeline run: {error}"));
    let structures = result
        .structural_result
        .unwrap_or_else(|| panic!("pipeline did not publish structural results"));
    let Some(nastran) = structures.nastran.as_ref() else {
        panic!(
            "pipeline did not retain NASTRAN results in {}",
            output_dir.display()
        );
    };
    assert_eq!(
        nastran.static_solve.status,
        ResultStatus::Ok,
        "SOL 101 failed in {}: {:?}",
        output_dir.display(),
        nastran.static_solve.error
    );

    let _ = std::fs::remove_dir_all(output_dir);
}
