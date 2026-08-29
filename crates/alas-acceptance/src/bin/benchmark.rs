// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Benchmark runner binary for ALAS computational core.

// Benchmark driver binary outputs formatted performance tables to stdout.
#![allow(clippy::print_stdout)]

use alas_acceptance::{format_benchmark_report, run_benchmark_suite};

fn main() {
    println!("Running ALAS computational performance benchmarks...\n");
    let report = run_benchmark_suite();
    println!("{}", format_benchmark_report(&report));
}
