// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Run a small installed-AVL dual-optimizer smoke case and retain its summary.

use std::fs;
use std::path::PathBuf;

use alas_config::{presets, AlasConfig};
use alas_exec::RunEnvironment;
use alas_pipeline::{run_solver_optimizations, OptimizationSolverMode, SolverOptimizationSet};

fn main() -> Result<(), String> {
    let executable = std::env::var_os("ALAS_AVL_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("external tools/avl352.exe"));
    if !executable.is_file() {
        return Err(format!(
            "AVL executable not found: {}",
            executable.display()
        ));
    }
    let preset = presets::get("A220-300").map_err(|error| error.to_string())?;
    let mut config = AlasConfig {
        preset: preset.name.to_owned(),
        geometry: preset.geometry.clone(),
        requirements: preset.requirements.clone(),
        ..AlasConfig::default()
    };
    if let Some(mass_model) = preset.mass_model.clone() {
        config.mass_model = mass_model;
    }
    config.mission.enabled = false;
    // The smoke case exercises both optimizer branches and the installed
    // process boundary; it is deliberately lower fidelity than a user run so
    // CI and local verification do not spend hours evaluating sixteen initial
    // candidates per branch.
    config.analysis.sweep_n_points = 3;
    config.analysis.spanwise_resolution = 1;
    config.analysis.chordwise_resolution = 1;
    config.analysis.fine_spanwise_resolution = 1;
    config.analysis.fine_chordwise_resolution = 1;
    config.optimizer.solver.max_iterations = 0;
    config.optimizer.solver.population_size = 1;
    config.optimizer.solver.seed_near_initial_design = true;
    config.optimizer.solver.seed = Some(42);
    let output = PathBuf::from("tmp/dual_solver_smoke_fast_20260822");
    fs::create_dir_all(&output).map_err(|error| error.to_string())?;
    let environment = RunEnvironment {
        avl_exe: Some(executable),
        ..RunEnvironment::default()
    };
    let set = run_solver_optimizations(
        &config,
        OptimizationSolverMode::Both,
        true,
        Some(42),
        &environment,
        &preset.design_vector,
        Some(&alas_config::design_variables::DesignVector::bounds()),
        Some(&output),
        None,
        None,
    );
    write_summary(&output, &set)?;
    tracing::info!(
        path = %output.join("summary.txt").display(),
        "dual solver smoke summary"
    );
    Ok(())
}

fn write_summary(output: &std::path::Path, set: &SolverOptimizationSet) -> Result<(), String> {
    let lines = [
        format!("vlm_status={}", set.vlm.status.as_str()),
        format!("vlm_error={}", set.vlm.error.as_deref().unwrap_or("")),
        format!("avl_status={}", set.avl.status.as_str()),
        format!("avl_error={}", set.avl.error.as_deref().unwrap_or("")),
        format!("vlm_design={}", set.vlm.design.is_some()),
        format!("avl_design={}", set.avl.design.is_some()),
    ];
    fs::write(output.join("summary.txt"), lines.join("\n")).map_err(|error| error.to_string())
}
