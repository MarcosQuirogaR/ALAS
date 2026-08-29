// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Integration tests for the computational benchmark suite.

// Test suite uses assertions on benchmark measurements.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_acceptance::benchmarks::{
    bench_airfoil_screening, bench_figure_rendering, bench_full_analysis, bench_full_pipeline,
    bench_structural_sizing, bench_vlm_solve, format_benchmark_report, BenchmarkSuiteReport,
};

#[test]
fn benchmark_suite_executes_and_measures_speedup() {
    let b_vlm = bench_vlm_solve(2);
    assert_eq!(b_vlm.iterations, 2);
    assert!(b_vlm.mean_ms > 0.0);
    assert!(b_vlm.speedup_factor > 0.0);

    let b_struct = bench_structural_sizing(5);
    assert_eq!(b_struct.iterations, 5);
    assert!(b_struct.mean_ms > 0.0);
    assert!(b_struct.speedup_factor > 0.0);

    let b_full = bench_full_analysis(1);
    assert_eq!(b_full.iterations, 1);
    assert!(b_full.mean_ms > 0.0);
    assert!(b_full.speedup_factor > 0.0);

    let b_screen = bench_airfoil_screening(1);
    assert_eq!(b_screen.iterations, 1);
    assert!(b_screen.mean_ms > 0.0);
    assert!(b_screen.speedup_factor > 0.0);

    let b_pipe = bench_full_pipeline(1);
    assert_eq!(b_pipe.iterations, 1);
    assert!(b_pipe.mean_ms > 0.0);
    assert!(b_pipe.speedup_factor > 0.0);

    let b_fig = bench_figure_rendering(1);
    assert_eq!(b_fig.iterations, 1);
    assert!(b_fig.mean_ms > 0.0);
    assert!(b_fig.speedup_factor > 0.0);

    let report = BenchmarkSuiteReport {
        results: vec![b_pipe, b_full, b_vlm, b_struct, b_screen, b_fig],
        total_elapsed_s: 1.0,
    };
    let formatted = format_benchmark_report(&report);
    assert!(formatted.contains("ALAS COMPUTATIONAL BENCHMARK REPORT"));
    assert!(formatted.contains("Speedup"));
}
