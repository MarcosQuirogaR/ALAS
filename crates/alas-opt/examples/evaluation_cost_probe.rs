// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Where one coupled candidate evaluation spends its wall-clock time.
//!
//! Dispatch-local probe. The search's cost is (analyses) times (seconds per
//! analysis), and the second factor is set by the multidisciplinary sizing
//! loop: every outer pass re-runs two mass analyses and, when the centre of
//! gravity has moved, a vortex-lattice trim. This probe reports the passes and
//! re-trims one evaluation actually took, alongside the wall-clock cost of the
//! same evaluation at several sizing-loop settings, so a claim about the
//! search's runtime can be traced to the analysis it is made of.
//!
//! Times are wall-clock seconds; masses kg.

// A diagnostic example: its output is the printed report, and a failed
// unwrap is the probe stopping on an input it cannot run.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use std::time::Instant;

use alas_config::AlasConfig;
use alas_opt::assess_candidate;
use alas_opt::objective::DesignObjective;

fn main() {
    let preset = std::env::args().nth(1).unwrap_or_else(|| "AVE".to_owned());
    // The design mode decides which aircraft is actually evaluated: a bare
    // preset document loads `CleanSheet`, which re-derives the fuselage from
    // the cabin and, on the regional and single-aisle types, produces a body
    // the preset does not have (see `tests/plausibility_residuals.rs`).
    // `reference_adaptation` is the mode a registered aircraft is optimized
    // in, so a per-evaluation cost meant to explain a preset benchmark has to
    // be taken there.
    let mode = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "clean_sheet".to_owned());
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
        .unwrap_or_else(|error| panic!("{preset}: {error}"));
    config.optimizer.design_space.mode = match mode.as_str() {
        "reference_adaptation" => alas_config::DesignMode::ReferenceAdaptation,
        "baseline_sandbox" => alas_config::DesignMode::BaselineSandbox,
        _ => alas_config::DesignMode::CleanSheet,
    };
    let config = config;
    let nominal = alas_config::presets::get(&preset)
        .map(|entry| entry.design_vector)
        .unwrap_or_default();
    let x = nominal.to_array();

    println!(
        "preset={preset} mode={mode} threads_visible={}",
        std::thread::available_parallelism()
            .map(|value| value.get())
            .unwrap_or(0)
    );

    for (label, passes, tolerance_kg, chordwise) in [
        ("product_default", 30_i64, 1.0_f64, 8_i64),
        ("tolerance_10kg", 30, 10.0, 8),
        ("tolerance_100kg", 30, 100.0, 8),
        ("passes_8", 8, 1.0, 8),
        ("passes_4", 4, 1.0, 8),
        ("chordwise_4", 30, 1.0, 4),
        ("chordwise_2", 30, 1.0, 2),
        ("screening", 3, 1000.0, 2),
    ] {
        let mut variant = config.clone();
        variant.optimizer.objective.sizing_max_iterations = passes;
        variant.optimizer.objective.sizing_tolerance_kg = tolerance_kg;
        variant.analysis.chordwise_resolution = chordwise;
        let objective = DesignObjective::new(variant);
        // One warm call so the report below is not the first-touch cost.
        let _ = assess_candidate(&objective, &x);
        let started = Instant::now();
        let repeats = 3;
        let mut last = None;
        for _ in 0..repeats {
            last = assess_candidate(&objective, &x).ok();
        }
        let mean_s = started.elapsed().as_secs_f64() / f64::from(repeats);
        match last {
            Some(assessment) => println!(
                "{label}: mean_s={mean_s:.4} passes={} retrims={} closed={} feasible={} objective={:.3} tow_kg={:.1} oew_kg={:.1} l_over_d={:.4}",
                assessment.sized.sizing_iterations,
                assessment.sized.retrim_count,
                assessment.sized.sizing_closed,
                assessment.hard_feasible,
                assessment.objective_value,
                assessment.sized.takeoff_mass_kg,
                assessment.sized.operating_empty_mass_kg,
                assessment.sized.lift_to_drag,
            ),
            None => println!("{label}: mean_s={mean_s:.4} evaluation_failed"),
        }
    }
}
