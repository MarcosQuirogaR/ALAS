// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! High-precision computational performance benchmark harness.
//!
//! Measures execution latency and throughput of the numerical core across
//! aerodynamics, mission integration, structural sizing, airfoil database
//! screening, and end-to-end pipeline execution, comparing throughput against
//! the Python reference baseline.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

/// Quick mode ([`set_quick`]): one timed iteration per workload and no
/// warm-up run, for a budget check where the suite's total time matters.
static QUICK: AtomicBool = AtomicBool::new(false);

/// Run every workload once, without its warm-up, instead of the default
/// iteration counts: the `--quick` budget check of `alas-bench`.
pub fn set_quick(quick: bool) {
    QUICK.store(quick, Ordering::Relaxed);
}

use alas_aero::operating_point::OperatingPoint;
use alas_aero::vlm::run as run_vlm;
use alas_atmo::Atmosphere;
use alas_config::presets;
use alas_config::AlasConfig;
use alas_geom::builder::AircraftBuilder;
use alas_pipeline::full_analysis::FullAnalysis;
use alas_pipeline::{DesignPipeline, PipelineOptions};
use alas_report::render_svg;
use alas_screen::types::AirfoilScreeningOptions;

use crate::matrix::generate_scenes_for_preset;

/// Statistical timing summary for a benchmarked operation.
#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    /// Benchmark task identifier.
    pub name: String,
    /// Number of iterations executed.
    pub iterations: usize,
    /// Average duration per iteration in milliseconds.
    pub mean_ms: f64,
    /// Minimum iteration duration in milliseconds.
    pub min_ms: f64,
    /// Maximum iteration duration in milliseconds.
    pub max_ms: f64,
    /// Approximate Python reference execution time in milliseconds.
    pub legacy_python_ms: f64,
    /// Measured speedup factor relative to Python.
    pub speedup_factor: f64,
    /// Timed iterations whose operation reported failure. A timing with
    /// failures measures an aborted computation and is not evidence of
    /// throughput; the report prints the count beside every row.
    pub failures: usize,
}

/// Aggregated benchmark suite report.
#[derive(Debug, Clone)]
pub struct BenchmarkSuiteReport {
    /// Individual benchmark results.
    pub results: Vec<BenchmarkResult>,
    /// Total duration of the benchmark suite in seconds.
    pub total_elapsed_s: f64,
}

/// Benchmark helper measuring execution times over multiple iterations.
///
/// `op` reports whether the operation succeeded; the timing of a failed
/// iteration is kept, so that the row still shows something, but the
/// failure is counted and printed, so that it cannot pass for throughput.
fn measure<F>(name: &str, iters: usize, legacy_ms: f64, mut op: F) -> BenchmarkResult
where
    F: FnMut() -> bool,
{
    let quick = QUICK.load(Ordering::Relaxed);
    let iters = if quick { 1 } else { iters };
    if !quick {
        // Warmup
        op();
    }

    let mut times_ms = Vec::with_capacity(iters);
    let mut failures = 0;
    for _ in 0..iters {
        let start = Instant::now();
        let succeeded = op();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        times_ms.push(elapsed);
        if !succeeded {
            failures += 1;
        }
    }

    let min_ms = times_ms.iter().copied().fold(f64::INFINITY, f64::min);
    let max_ms = times_ms.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let sum_ms: f64 = times_ms.iter().sum();
    let mean_ms = sum_ms / iters as f64;
    let speedup = if mean_ms > 0.0 {
        legacy_ms / mean_ms
    } else {
        1.0
    };

    BenchmarkResult {
        name: name.to_owned(),
        iterations: iters,
        mean_ms,
        min_ms,
        max_ms,
        legacy_python_ms: legacy_ms,
        speedup_factor: speedup,
        failures,
    }
}

/// Benchmark full baseline design pipeline execution.
pub fn bench_full_pipeline(iters: usize) -> BenchmarkResult {
    let mut config = AlasConfig::default();
    config.structures.run_nastran = false;
    config.structures.run_patran_export = false;
    config.optimizer.objective.mtow_sizing = alas_config::MtowSizing::FixedRequirement;
    let pipeline = DesignPipeline::new(config);
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: true,
        parallel: true,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: Some(42),
        quiet: true,
    };

    // Python pipeline typically takes ~12,000 ms for complete baseline run
    measure("Full Baseline Pipeline", iters, 12000.0, || {
        pipeline
            .run(&options, &alas_pipeline::RunEnvironment::default())
            .is_ok()
    })
}

/// Benchmark full aerodynamic polar sweep, trim, stability, and mass analysis.
pub fn bench_full_analysis(iters: usize) -> BenchmarkResult {
    let (preset, config) = match presets::get("AVE").ok().and_then(|preset| {
        AlasConfig::from_value(&serde_json::json!({ "preset": preset.name }))
            .ok()
            .map(|config| (preset, config))
    }) {
        Some(pair) => pair,
        None => {
            return BenchmarkResult {
                name: "Full Analysis (VLM/Polars/Trim)".to_owned(),
                iterations: iters,
                mean_ms: 0.0,
                min_ms: 0.0,
                max_ms: 0.0,
                legacy_python_ms: 3500.0,
                speedup_factor: 1.0,
                failures: iters,
            };
        }
    };
    let full = FullAnalysis::new(config);

    // Python full_analysis takes ~3,500 ms
    measure("Full Analysis (VLM/Polars/Trim)", iters, 3500.0, || {
        full.run(&preset.design_vector, true).is_ok()
    })
}

/// Benchmark a single-point native vortex-lattice solve.
pub fn bench_vlm_solve(iters: usize) -> BenchmarkResult {
    let preset = match presets::get("AVE") {
        Ok(p) => p,
        Err(_) => {
            return BenchmarkResult {
                name: "Native VLM Solve".to_owned(),
                iterations: iters,
                mean_ms: 0.0,
                min_ms: 0.0,
                max_ms: 0.0,
                legacy_python_ms: 250.0,
                speedup_factor: 1.0,
                failures: iters,
            };
        }
    };
    let builder = AircraftBuilder::new(Some(preset.geometry.clone()));
    let airplane = match builder.build(Some(&preset.design_vector), true) {
        Ok(a) => a,
        Err(_) => {
            return BenchmarkResult {
                name: "Native VLM Solve".to_owned(),
                iterations: iters,
                mean_ms: 0.0,
                min_ms: 0.0,
                max_ms: 0.0,
                legacy_python_ms: 250.0,
                speedup_factor: 1.0,
                failures: iters,
            };
        }
    };
    let atmo = Atmosphere::new(10000.0);
    let op_point = OperatingPoint::new(atmo, 230.0, 3.0, 0.0, 0.0, 0.0, 0.0);

    // The reference VLM solve takes ~250 ms.
    measure("Native VLM Solve (1x1)", iters, 250.0, || {
        run_vlm(&airplane, &op_point, 1, 1).is_ok()
    })
}

/// Benchmark one vortex-lattice solve at the fine product mesh (2 spanwise
/// by 8 chordwise per section, 800 panels on the reference preset): the
/// resolution the full analysis runs at, and where the dense factorization
/// dominates.
pub fn bench_vlm_solve_fine(iters: usize) -> BenchmarkResult {
    let name = "Native VLM Solve (fine 2x8)";
    let Ok(preset) = presets::get("AVE") else {
        return unavailable(name, iters, 250.0);
    };
    let builder = AircraftBuilder::new(Some(preset.geometry.clone()));
    let Ok(airplane) = builder.build(Some(&preset.design_vector), true) else {
        return unavailable(name, iters, 250.0);
    };
    let atmo = Atmosphere::new(10000.0);
    let op_point = OperatingPoint::new(atmo, 230.0, 3.0, 0.0, 0.0, 0.0, 0.0);
    measure(name, iters, 250.0, || {
        run_vlm(&airplane, &op_point, 2, 8).is_ok()
    })
}

/// The row for a benchmark whose fixture could not be set up: no timing,
/// every iteration counted as failed.
fn unavailable(name: &str, iters: usize, legacy_ms: f64) -> BenchmarkResult {
    BenchmarkResult {
        name: name.to_owned(),
        iterations: iters,
        mean_ms: 0.0,
        min_ms: 0.0,
        max_ms: 0.0,
        legacy_python_ms: legacy_ms,
        speedup_factor: 1.0,
        failures: iters,
    }
}

/// Benchmark wingbox analytical structural sizing and analysis.
pub fn bench_structural_sizing(iters: usize) -> BenchmarkResult {
    let (preset, config) = match presets::get("AVE").ok().and_then(|preset| {
        AlasConfig::from_value(&serde_json::json!({ "preset": preset.name }))
            .ok()
            .map(|config| (preset, config))
    }) {
        Some(pair) => pair,
        None => {
            return BenchmarkResult {
                name: "Wingbox Structural Sizing".to_owned(),
                iterations: iters,
                mean_ms: 0.0,
                min_ms: 0.0,
                max_ms: 0.0,
                legacy_python_ms: 80.0,
                speedup_factor: 1.0,
                failures: iters,
            };
        }
    };
    let mut config = config;
    config.structures.run_nastran = false;
    config.structures.run_patran_export = false;
    let full = FullAnalysis::new(config.clone());
    let report = match full.run(&preset.design_vector, false) {
        Ok(r) => r,
        Err(_) => {
            return BenchmarkResult {
                name: "Wingbox Structural Sizing".to_owned(),
                iterations: iters,
                mean_ms: 0.0,
                min_ms: 0.0,
                max_ms: 0.0,
                legacy_python_ms: 80.0,
                speedup_factor: 1.0,
                failures: iters,
            };
        }
    };

    // Python structural sizing takes ~80 ms
    measure("Wingbox Structural Sizing", iters, 80.0, || {
        alas_pipeline::structural::run_structural_analysis(
            &config,
            &report,
            None,
            &alas_pipeline::RunEnvironment::default(),
        )
        .status
            != "error"
    })
}

/// Benchmark Selig airfoil database screening.
pub fn bench_airfoil_screening(iters: usize) -> BenchmarkResult {
    let mut config = AlasConfig::default();
    config.optimizer.objective.mtow_sizing = alas_config::MtowSizing::FixedRequirement;
    let options = AirfoilScreeningOptions {
        name_filter: "NACA*".to_string(),
        refine_3d: false,
        refine_top_n: 0,
        ..Default::default()
    };

    // Python airfoil screening for NACA subset takes ~3,000 ms
    measure("Airfoil DB Screening (NACA)", iters, 3000.0, || {
        alas_screen::run_airfoil_screening(&config, None, &options, None, None, None).is_ok()
    })
}

/// Benchmark figure scene generation and SVG rendering export.
pub fn bench_figure_rendering(iters: usize) -> BenchmarkResult {
    let preset = match presets::get("AVE") {
        Ok(p) => p,
        Err(_) => {
            return BenchmarkResult {
                name: "Scene Generation & SVG Export".to_owned(),
                iterations: iters,
                mean_ms: 0.0,
                min_ms: 0.0,
                max_ms: 0.0,
                legacy_python_ms: 4500.0,
                speedup_factor: 1.0,
                failures: iters,
            };
        }
    };
    let builder = AircraftBuilder::new(Some(preset.geometry.clone()));
    let airplane = match builder.build(Some(&preset.design_vector), true) {
        Ok(a) => a,
        Err(_) => {
            return BenchmarkResult {
                name: "Scene Generation & SVG Export".to_owned(),
                iterations: iters,
                mean_ms: 0.0,
                min_ms: 0.0,
                max_ms: 0.0,
                legacy_python_ms: 4500.0,
                speedup_factor: 1.0,
                failures: iters,
            };
        }
    };
    let mut config = AlasConfig::default();
    config.structures.run_nastran = false;
    config.structures.run_patran_export = false;
    config.optimizer.objective.mtow_sizing = alas_config::MtowSizing::FixedRequirement;
    let pipeline = DesignPipeline::new(config.clone());
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: true,
        parallel: true,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: Some(42),
        quiet: true,
    };
    let res = match pipeline.run(&options, &alas_pipeline::RunEnvironment::default()) {
        Ok(r) => r,
        Err(_) => {
            return BenchmarkResult {
                name: "Scene Generation & SVG Export".to_owned(),
                iterations: iters,
                mean_ms: 0.0,
                min_ms: 0.0,
                max_ms: 0.0,
                legacy_python_ms: 4500.0,
                speedup_factor: 1.0,
                failures: iters,
            };
        }
    };

    // Python Matplotlib figure rendering across figures takes ~4,500 ms
    measure("Scene Generation & SVG Export", iters, 4500.0, || {
        let scenes = generate_scenes_for_preset(&res, &config, &airplane);
        let rendered = scenes
            .iter()
            .filter(|scene| !render_svg(scene).is_empty())
            .count();
        !scenes.is_empty() && rendered == scenes.len()
    })
}

/// Execute the complete ALAS performance benchmark suite.
pub fn run_benchmark_suite() -> BenchmarkSuiteReport {
    let start_all = Instant::now();

    let results = vec![
        bench_full_pipeline(3),
        bench_full_analysis(3),
        bench_vlm_solve(10),
        bench_vlm_solve_fine(5),
        bench_structural_sizing(20),
        bench_airfoil_screening(2),
        bench_figure_rendering(3),
    ];

    let total_elapsed = start_all.elapsed().as_secs_f64();

    BenchmarkSuiteReport {
        results,
        total_elapsed_s: total_elapsed,
    }
}

/// Formats the benchmark results as a clean comparative terminal table.
pub fn format_benchmark_report(report: &BenchmarkSuiteReport) -> String {
    let mut out = String::new();
    out.push_str("=================================================================================================\n");
    out.push_str("                              ALAS COMPUTATIONAL BENCHMARK REPORT                                \n");
    out.push_str("=================================================================================================\n");
    out.push_str(&format!(
        "{:<35} | {:>5} | {:>4} | {:>10} | {:>10} | {:>12} | {:>8}\n",
        "Workload", "Iters", "Fail", "Mean (ms)", "Min (ms)", "Python (ms)", "Speedup"
    ));
    out.push_str("------------------------------------+-------+------+------------+------------+--------------+-----------\n");

    for r in &report.results {
        out.push_str(&format!(
            "{:<35} | {:>5} | {:>4} | {:>10.2} | {:>10.2} | {:>12.1} | {:>7.1}x\n",
            r.name,
            r.iterations,
            r.failures,
            r.mean_ms,
            r.min_ms,
            r.legacy_python_ms,
            r.speedup_factor
        ));
    }
    if report.results.iter().any(|r| r.failures > 0) {
        out.push_str(
            "Rows with failures timed an aborted computation; their means are not throughput.\n",
        );
    }
    out.push_str("=================================================================================================\n");
    out.push_str(&format!(
        "Total benchmark duration: {:.2} seconds\n",
        report.total_elapsed_s
    ));
    out
}

/// Benchmarks whose mean exceeds its ceiling in `budget`, or that failed.
///
/// The budget is plain text, one `Workload name = ceiling_ms` per line,
/// `#` starting a comment; the name is the report's workload column. A
/// workload with a ceiling but no result is itself a violation, so that a
/// renamed or dropped benchmark cannot silently leave the guard.
pub fn check_benchmark_budget(report: &BenchmarkSuiteReport, budget: &str) -> Vec<String> {
    let mut violations = Vec::new();
    for line in budget.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((name, ceiling)) = line.split_once('=') else {
            continue;
        };
        let (name, ceiling) = (name.trim(), ceiling.trim());
        let Ok(ceiling_ms) = ceiling.parse::<f64>() else {
            violations.push(format!("{name}: unreadable ceiling `{ceiling}`"));
            continue;
        };
        match report.results.iter().find(|r| r.name == name) {
            None => violations.push(format!("{name}: no such benchmark in the report")),
            Some(r) if r.failures > 0 => {
                violations.push(format!(
                    "{name}: {} of {} runs failed",
                    r.failures, r.iterations
                ));
            }
            Some(r) if r.mean_ms.is_nan() || r.mean_ms > ceiling_ms => violations.push(format!(
                "{name}: mean {:.1} ms exceeds the {ceiling_ms:.1} ms ceiling",
                r.mean_ms
            )),
            Some(_) => {}
        }
    }
    violations
}

#[cfg(test)]
mod budget_tests {
    use super::*;

    fn report(mean_ms: f64, failures: usize) -> BenchmarkSuiteReport {
        BenchmarkSuiteReport {
            results: vec![BenchmarkResult {
                name: "Native VLM Solve".to_owned(),
                iterations: 2,
                mean_ms,
                min_ms: mean_ms,
                max_ms: mean_ms,
                legacy_python_ms: 1.0,
                speedup_factor: 1.0,
                failures,
            }],
            total_elapsed_s: 0.0,
        }
    }

    #[test]
    fn the_budget_flags_slow_failed_and_missing_workloads() {
        let budget = "# ceilings\nNative VLM Solve = 10\n";
        assert!(check_benchmark_budget(&report(9.0, 0), budget).is_empty());
        assert_eq!(check_benchmark_budget(&report(11.0, 0), budget).len(), 1);
        assert_eq!(check_benchmark_budget(&report(1.0, 1), budget).len(), 1);
        assert_eq!(check_benchmark_budget(&report(1.0, 0), "Gone = 5").len(), 1);
        assert_eq!(
            check_benchmark_budget(&report(f64::NAN, 0), budget).len(),
            1
        );
    }
}
