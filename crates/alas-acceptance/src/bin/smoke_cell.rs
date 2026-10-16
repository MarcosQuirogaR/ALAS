// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! One cell of `cargo xtask smoke`: a preset's native baseline analysis and a
//! short, evaluation-budgeted search, reported as one JSON object on stdout.
//!
//! `smoke_cell --list` prints the registered preset names, one per line, so
//! the task never keeps its own copy of the registry.
//!
//! `smoke_cell --preset NAME --evaluations N --workers W` runs
//!
//! * the baseline: `DesignPipeline::run` with `optimize = false` on the
//!   registered design, structures without NASTRAN, no route fetch;
//! * the search: the two-stage product search in `FixedRequirement` mode with
//!   `N` evaluations per stage, seed 7, stopping on evaluation counts only, so
//!   the outcome does not depend on load or on `W`.
//!
//! A search that finds no feasible design is a reported outcome
//! (`no_feasible_design`), not an error: a handful of evaluations seldom
//! reaches one. Only a failed baseline or an unexpected search error is a
//! smoke failure.

// The binary's output is one JSON object (or the list) on stdout; a usage error goes to stderr.
#![allow(clippy::print_stderr, clippy::print_stdout)]

use alas_config::{AlasConfig, DesignMode, MtowSizing};
use alas_opt::{DesignOptimizer, OptimizationError};
use alas_pipeline::{DesignPipeline, PipelineOptions, RunEnvironment};
use serde_json::{json, Value};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--list") {
        for name in alas_config::presets::available() {
            println!("{name}");
        }
        return;
    }
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|at| args.get(at + 1))
            .cloned()
    };
    let Some(preset) = value("--preset") else {
        eprintln!("usage: smoke_cell --list | --preset NAME [--evaluations N] [--workers W]");
        std::process::exit(2);
    };
    let evaluations = value("--evaluations")
        .and_then(|v| v.parse().ok())
        .unwrap_or(24);
    let workers = value("--workers").and_then(|v| v.parse().ok()).unwrap_or(1);
    let started = std::time::Instant::now();
    let baseline = baseline(&preset);
    let baseline_s = started.elapsed().as_secs_f64();
    let search = search(&preset, evaluations, workers);
    let cell = json!({
        "preset": preset,
        "baseline": baseline,
        "search": search,
        "baseline_wall_s": baseline_s,
        "wall_s": started.elapsed().as_secs_f64(),
    });
    println!("{cell}");
}

fn config(preset: &str) -> AlasConfig {
    let mut config = AlasConfig::from_value(&json!({ "preset": preset }))
        .unwrap_or_else(|error| panic!("{preset}: {error}"));
    config.structures.run_nastran = false;
    config.structures.run_patran_export = false;
    config
}

fn baseline(preset: &str) -> Value {
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: false,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: None,
        quiet: true,
    };
    match DesignPipeline::new(config(preset)).run(&options, &RunEnvironment::default()) {
        Ok(result) => match result.optimized_report {
            Some(report) => json!({
                "ok": true,
                "static_margin": report.static_margin,
                "total_mass_kg": report.component_masses.values().sum::<f64>(),
                "route_flown": result.mission_result.is_some(),
            }),
            None => json!({ "ok": false, "error": "the baseline run produced no analysis report" }),
        },
        Err(error) => json!({ "ok": false, "error": error }),
    }
}

fn search(preset: &str, evaluations: i64, workers: i64) -> Value {
    let mut config = config(preset);
    config.optimizer.solver.method = alas_config::optimizer::PRODUCT_DE_METHOD.to_owned();
    config.optimizer.design_space.mode = DesignMode::ReferenceAdaptation;
    config.optimizer.objective.mtow_sizing = MtowSizing::FixedRequirement;
    config.optimizer.solver.seed = Some(7);
    config.optimizer.solver.workers = workers;
    config.optimizer.solver.screening.max_evaluations = evaluations;
    config.optimizer.solver.refinement.max_evaluations = evaluations;
    config.optimizer.solver.stop_on_evaluations_only = true;
    let Ok(registered) = alas_config::presets::get(preset) else {
        return json!({ "outcome": "error", "error": format!("{preset} is not registered") });
    };
    let start = registered.design_vector;
    match DesignOptimizer::new(config).run(None, Some(&start), None) {
        Ok(result) => json!({
            "outcome": "feasible",
            "termination": result.termination,
            "best_cost": result.best_cost,
            "analysis_evaluations": result
                .search_diagnostics
                .as_ref()
                .map(|d| d.analysis_evaluations),
        }),
        Err(OptimizationError::NoFeasibleDesign(evidence)) => json!({
            "outcome": "no_feasible_design",
            "evaluated_candidates": evidence.evaluated_candidates,
        }),
        Err(error) => json!({ "outcome": "error", "error": error.to_string() }),
    }
}
