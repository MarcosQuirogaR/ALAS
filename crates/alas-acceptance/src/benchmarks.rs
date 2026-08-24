// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! High-precision computational performance benchmark harness.
//!
//! Measures execution latency and throughput of the numerical core across
//! aerodynamics, mission integration, structural sizing, airfoil database
//! screening, and end-to-end pipeline execution, comparing throughput against
//! the legacy Python baseline.

use std::time::Instant;

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
    /// Approximate legacy Python execution time in milliseconds.
    pub legacy_python_ms: f64,
    /// Measured speedup factor relative to Python.
    pub speedup_factor: f64,
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
fn measure<F>(name: &str, iters: usize, legacy_ms: f64, mut op: F) -> BenchmarkResult
where
    F: FnMut(),
{
    // Warmup
    op();

    let mut times_ms = Vec::with_capacity(iters);
    for _ in 0..iters {
        let start = Instant::now();
        op();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        times_ms.push(elapsed);
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
    }
}

/// Benchmark full baseline design pipeline execution.
pub fn bench_full_pipeline(iters: usize) -> BenchmarkResult {
    let config = AlasConfig::default();
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
        let _ = pipeline.run(&options, &alas_pipeline::RunEnvironment::default());
    })
}

/// Benchmark full aerodynamic polar sweep, trim, stability, and mass analysis.
pub fn bench_full_analysis(iters: usize) -> BenchmarkResult {
    let preset = match presets::get("AVE") {
        Ok(p) => p,
        Err(_) => {
            return BenchmarkResult {
                name: "Full Analysis (VLM/Polars/Trim)".to_owned(),
                iterations: iters,
                mean_ms: 0.0,
                min_ms: 0.0,
                max_ms: 0.0,
                legacy_python_ms: 3500.0,
                speedup_factor: 1.0,
            };
        }
    };
    let config = AlasConfig {
        geometry: preset.geometry.clone(),
        ..Default::default()
    };
    let full = FullAnalysis::new(config);

    // Python full_analysis takes ~3,500 ms
    measure("Full Analysis (VLM/Polars/Trim)", iters, 3500.0, || {
        let _ = full.run(&preset.design_vector, true);
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
            };
        }
    };
    let atmo = Atmosphere::isa(10000.0);
    let op_point = OperatingPoint::new(atmo, 230.0, 3.0, 0.0, 0.0, 0.0, 0.0);

    // The reference VLM solve takes ~250 ms.
    measure("Native VLM Solve", iters, 250.0, || {
        let _ = run_vlm(&airplane, &op_point, 1, 1);
    })
}

/// Benchmark wingbox analytical structural sizing and analysis.
pub fn bench_structural_sizing(iters: usize) -> BenchmarkResult {
    let preset = match presets::get("AVE") {
        Ok(p) => p,
        Err(_) => {
            return BenchmarkResult {
                name: "Wingbox Structural Sizing".to_owned(),
                iterations: iters,
                mean_ms: 0.0,
                min_ms: 0.0,
                max_ms: 0.0,
                legacy_python_ms: 80.0,
                speedup_factor: 1.0,
            };
        }
    };
    let config = AlasConfig {
        geometry: preset.geometry.clone(),
        ..Default::default()
    };
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
            };
        }
    };

    // Python structural sizing takes ~80 ms
    measure("Wingbox Structural Sizing", iters, 80.0, || {
        let _ = alas_pipeline::structural::run_structural_analysis(
            &config,
            &report,
            None,
            &alas_pipeline::RunEnvironment::default(),
        );
    })
}

/// Benchmark Selig airfoil database screening.
pub fn bench_airfoil_screening(iters: usize) -> BenchmarkResult {
    let config = AlasConfig::default();
    let options = AirfoilScreeningOptions {
        name_filter: "NACA*".to_string(),
        refine_3d: false,
        refine_top_n: 0,
        ..Default::default()
    };

    // Python airfoil screening for NACA subset takes ~3,000 ms
    measure("Airfoil DB Screening (NACA)", iters, 3000.0, || {
        let _ = alas_screen::run_airfoil_screening(&config, None, &options, None, None, None);
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
            };
        }
    };
    let config = AlasConfig::default();
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
            };
        }
    };

    // Python Matplotlib figure rendering across figures takes ~4,500 ms
    measure("Scene Generation & SVG Export", iters, 4500.0, || {
        let scenes = generate_scenes_for_preset(&res, &config, &airplane);
        for scene in &scenes {
            let _ = render_svg(scene);
        }
    })
}

/// Execute the complete ALAS performance benchmark suite.
pub fn run_benchmark_suite() -> BenchmarkSuiteReport {
    let start_all = Instant::now();

    let results = vec![
        bench_full_pipeline(3),
        bench_full_analysis(3),
        bench_vlm_solve(10),
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
        "{:<35} | {:>5} | {:>10} | {:>10} | {:>12} | {:>8}\n",
        "Workload", "Iters", "Mean (ms)", "Min (ms)", "Python (ms)", "Speedup"
    ));
    out.push_str("------------------------------------+-------+------------+------------+--------------+-----------\n");

    for r in &report.results {
        out.push_str(&format!(
            "{:<35} | {:>5} | {:>10.2} | {:>10.2} | {:>12.1} | {:>7.1}x\n",
            r.name, r.iterations, r.mean_ms, r.min_ms, r.legacy_python_ms, r.speedup_factor
        ));
    }
    out.push_str("=================================================================================================\n");
    out.push_str(&format!(
        "Total benchmark duration: {:.2} seconds\n",
        report.total_elapsed_s
    ));
    out
}
