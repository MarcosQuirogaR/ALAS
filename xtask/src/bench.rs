// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Benchmark runner task for xtask.
//!
//! Invokes the optimized release build of the benchmark harness in
//! `alas-acceptance` to measure numerical core throughput against Python references.

use std::path::Path;
use std::process::Command;

/// Run the release performance benchmark suite.
pub fn run_benchmarks(root: &Path) -> Result<(), String> {
    println!("Building and running ALAS computational benchmarks (release mode)...\n");

    let status = Command::new(env!("CARGO"))
        .current_dir(root)
        .args([
            "run",
            "--release",
            "-p",
            "alas-acceptance",
            "--bin",
            "alas-bench",
        ])
        .status()
        .map_err(|e| format!("failed to execute benchmark binary: {e}"))?;

    if !status.success() {
        return Err("benchmarks failed".to_owned());
    }

    Ok(())
}
