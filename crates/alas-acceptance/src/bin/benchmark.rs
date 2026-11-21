// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Benchmark runner binary for ALAS computational core.
//!
//! `alas-bench` prints the timing table; `--quick` times each workload once
//! without a warm-up. `alas-bench --budget <file>` also
//! compares each benchmark's mean against the per-benchmark ceiling in the
//! budget file and exits non-zero when one is exceeded or failed, so a CI job
//! or a pre-merge check catches a run-time regression.

// Benchmark driver binary outputs formatted performance tables to stdout.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use alas_acceptance::{
    check_benchmark_budget, format_benchmark_report, run_benchmark_suite, set_quick,
};

/// The full analysis recurses deeper than the 1 MiB Windows main-thread
/// stack (the GUI's analysis worker and the dump examples use the same
/// size): on the main thread the suite overflowed before its first result.
const BENCH_STACK_BYTES: usize = 256 << 20;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let budget = args
        .iter()
        .position(|arg| arg == "--budget")
        .and_then(|index| args.get(index + 1).cloned());
    set_quick(args.iter().any(|arg| arg == "--quick"));
    let worker = std::thread::Builder::new()
        .name("alas-bench".to_owned())
        .stack_size(BENCH_STACK_BYTES)
        .spawn(move || {
            println!("Running ALAS computational performance benchmarks...\n");
            let report = run_benchmark_suite();
            println!("{}", format_benchmark_report(&report));
            match budget {
                Some(path) => match std::fs::read_to_string(&path) {
                    Ok(text) => {
                        let violations = check_benchmark_budget(&report, &text);
                        for line in &violations {
                            eprintln!("BUDGET EXCEEDED: {line}");
                        }
                        i32::from(!violations.is_empty())
                    }
                    Err(error) => {
                        eprintln!("cannot read budget file {path}: {error}");
                        2
                    }
                },
                None => 0,
            }
        });
    let code = match worker.map(std::thread::JoinHandle::join) {
        Ok(Ok(code)) => code,
        _ => 3,
    };
    std::process::exit(code);
}
