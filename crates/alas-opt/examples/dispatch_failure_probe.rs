// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What the coupled dispatch closure actually says when it fails.
//!
//! `dispatch_model_failed` and `sizing_not_closed` are boolean residuals: the
//! residual table records that the closure did not complete, not the reason
//! the burn model gave. This probe re-runs the same
//! `alas_mass::dispatch::solve_dispatch` the sizing loop runs, on a preset's
//! own nominal design and its own mission model, and prints the typed status
//! with its reason string.
//!
//! Usage:
//!   `cargo run --release -p alas-opt --example dispatch_failure_probe -- \
//!      [preset|all] [clean_sheet|reference_adaptation]`

// A diagnostic example: its output is the printed report, and a failed
// unwrap is the probe stopping on an input it cannot run.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use alas_config::AlasConfig;
use alas_opt::assess_product_candidate;

fn main() {
    let target = std::env::args().nth(1).unwrap_or_else(|| "all".to_owned());
    let mode = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "reference_adaptation".to_owned());
    let presets: Vec<String> = if target.eq_ignore_ascii_case("all") {
        alas_config::presets::available()
            .into_iter()
            .map(ToOwned::to_owned)
            .collect()
    } else {
        vec![target]
    };
    for preset in presets {
        probe(&preset, &mode);
    }
}

fn probe(preset: &str, mode: &str) {
    let config = AlasConfig::from_value(&serde_json::json!({
        "preset": preset,
        "optimizer": {"design_space": {"mode": mode}},
    }))
    .expect("the registered preset loads");
    let design = alas_config::presets::get(preset)
        .expect("registered preset")
        .design_vector;
    println!("================ {preset} ({mode}) ================");
    match assess_product_candidate(&config, &design) {
        Ok(assessment) => {
            println!("  dispatch status : {:?}", assessment.sized.dispatch.status);
            println!(
                "  takeoff mass    : {:.1} kg | block fuel {:.1} kg",
                assessment.sized.takeoff_mass_kg, assessment.sized.block_fuel_kg
            );
            println!(
                "  design range    : {:.1} m | minimum profile range {:.1} m",
                assessment.sized.design_range_m, assessment.sized.design_range_m
            );
            let violated = assessment.violated_hard_ids();
            println!("  violated hard   : {}", violated.join(", "));
        }
        Err(reason) => println!("  candidate rejected: {reason}"),
    }
}
