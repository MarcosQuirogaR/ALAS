// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Measure where the product design search spends its wall-clock time.
//!
//! This is a dispatch-local benchmark probe, not a product entry point. It
//! records the exact analysis-start instant, the cost of one full coupled
//! candidate evaluation, and the search's own evaluation/termination state, so
//! a claim of "converged in N seconds" can be checked against the evaluation
//! count it actually paid for.
//!
//! Usage:
//!   `cargo run --release -p alas-opt --example mads_timing_probe -- \
//!      [preset] [max_iterations] [population_multiplier] [workers] [seed]`
//!
//! `preset` is a registered aircraft name or `AVE` for the reference twin.
//! Every timing is wall-clock seconds measured with [`Instant`] on the
//! calling thread; masses are kg and lengths m (SI throughout).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use std::time::Instant;

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_opt::objective::DesignObjective;
use alas_opt::{assess_candidate, DesignOptimizer};

fn arg<T: std::str::FromStr>(index: usize, fallback: T) -> T {
    std::env::args()
        .nth(index)
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

fn main() {
    let preset: String = arg(1, "AVE".to_owned());
    let max_iterations: i64 = arg(2, 15);
    let population: i64 = arg(3, 6);
    let workers: i64 = arg(4, 1);
    let seed: i64 = arg(5, 42);

    let mut config = if preset.eq_ignore_ascii_case("none") {
        AlasConfig::default()
    } else {
        AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
            .unwrap_or_else(|error| panic!("preset {preset}: {error}"))
    };
    config.optimizer.solver.max_iterations = max_iterations;
    config.optimizer.solver.population_size = population;
    config.optimizer.solver.workers = workers;
    config.optimizer.solver.seed = Some(seed);

    let nominal = if config.preset.is_empty() {
        DesignVector::default()
    } else {
        alas_config::presets::get(&config.preset)
            .map(|entry| entry.design_vector)
            .unwrap_or_default()
    };

    println!(
        "# probe preset={preset} mode={:?}",
        config.optimizer.design_space.mode
    );
    println!(
        "# solver max_iterations={max_iterations} population_multiplier={population} workers={workers} seed={seed}"
    );

    // One coupled evaluation, from a cold objective, then a warm repeat.
    let objective = DesignObjective::new(config.clone());
    let single_start = Instant::now();
    let assessment = assess_candidate(&objective, &nominal.to_array());
    let single_s = single_start.elapsed().as_secs_f64();
    match &assessment {
        Ok(value) => println!(
            "single_evaluation_s={single_s:.4} feasible={} cost={:.6} objective={:.6} tow_kg={:.1} oew_kg={:.1} l_over_d={:.3}",
            value.hard_feasible,
            value.cost,
            value.objective_value,
            value.sized.takeoff_mass_kg,
            value.sized.operating_empty_mass_kg,
            value.sized.lift_to_drag
        ),
        Err(reason) => println!("single_evaluation_s={single_s:.4} failed={reason}"),
    }

    let repeats = 5;
    let repeat_start = Instant::now();
    for _ in 0..repeats {
        let _ = assess_candidate(&objective, &nominal.to_array());
    }
    let mean_s = repeat_start.elapsed().as_secs_f64() / f64::from(repeats);
    println!("mean_evaluation_s={mean_s:.4} over {repeats} repeats");

    // The analysis-start instant for the optimization stage.
    let analysis_start = Instant::now();
    let mut optimizer = DesignOptimizer::new(config.clone());
    let mut progress_lines = 0usize;
    let mut progress = |line: &str| {
        progress_lines += 1;
        if progress_lines % 5 == 1 || line.starts_with("mads termination") {
            println!("  [{:8.3}s] {line}", analysis_start.elapsed().as_secs_f64());
        }
    };
    let outcome = optimizer.run(None, Some(&nominal), Some(&mut progress));
    let elapsed_s = analysis_start.elapsed().as_secs_f64();

    match outcome {
        Ok(result) => {
            let history = &result.history;
            let feasible = history.valid.iter().filter(|value| **value).count();
            println!(
                "search_s={elapsed_s:.3} evaluations={} feasible={} termination={} best_cost={:.6} best_valid={}",
                history.n_evaluations(),
                feasible,
                result.termination,
                result.best_cost,
                result.best_valid
            );
            println!(
                "per_evaluation_s={:.4}",
                elapsed_s / (history.n_evaluations().max(1) as f64)
            );
            println!(
                "best_design={}",
                serde_json::to_string(&result.best_design).unwrap_or_default()
            );
        }
        Err(error) => {
            println!("search_s={elapsed_s:.3} error={error}");
        }
    }
}
