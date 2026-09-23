// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Why the default optimisation route rejects a registered aircraft's
//! candidates, counted by the invariant each one broke.
//!
//! A user ran the A380-800 preset in the GUI with VLM optimisation and
//! default settings and got `NoFeasibleDesign { evaluated_candidates: 1540,
//! rejection_reason_counts: {"geometry_build": 32, "mass_coordinates": 1508} }`.
//! The nominal design itself is evaluable, so the rejection is a property of
//! the *population* the search draws around it, and 1508 identical labels say
//! which phase refused those candidates without saying which physical
//! invariant they broke.
//!
//! This probe draws the same candidates the search draws - the preset's own
//! reference-adaptation bounds, a fixed seed, the product objective - and
//! tallies the typed rejection reason for each. It reports the distribution,
//! and for the dominant reason it prints the first offending candidate's
//! design vector against its bounds, so the cause can be attributed to a
//! variable rather than restated as a search failure.
//!
//! SI throughout: lengths m, masses kg, angles deg where a spec says so.
//! Body frame origin at the fuselage nose, +x aft, +y starboard, +z up.
//!
//! Usage:
//!   `cargo run --release -p alas-opt --example a380_rejection_census -- \
//!      [preset] [samples] [seed]`

// A diagnostic example: its output is the printed report, and a failed
// unwrap is the probe stopping on an input it cannot run.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use std::collections::BTreeMap;

use alas_config::design_variables::{DesignVector, SPECS};
use alas_config::AlasConfig;
use alas_opt::objective::DesignObjective;
use alas_opt::{assess_candidate, sampling};

fn arg<T: std::str::FromStr>(index: usize, fallback: T) -> T {
    std::env::args()
        .nth(index)
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

fn main() {
    let preset_name: String = arg(1, "A380-800".to_owned());
    let samples: usize = arg(2, 400);
    let seed: u64 = arg(3, 20_260_922);

    let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset_name }))
        .unwrap_or_else(|error| panic!("the {preset_name} preset loads: {error}"));
    let nominal = alas_config::presets::get(&preset_name)
        .unwrap_or_else(|error| panic!("{preset_name} is registered: {error}"))
        .design_vector;
    let bounds = config.optimizer.design_space.bounds(&nominal);

    println!("================ {preset_name} rejection census ================");
    println!(
        "mode {:?} | samples {samples} | seed {seed} | variables {}",
        config.optimizer.design_space.mode,
        bounds.len()
    );

    let objective = DesignObjective::new(config);

    // The nominal first: if it is evaluable, every rejection below is a
    // statement about the neighbourhood rather than about the aircraft.
    match assess_candidate(&objective, &nominal.to_array()) {
        Ok(assessment) => println!(
            "nominal: EVALUABLE hard_feasible={} cost={:.6}",
            assessment.hard_feasible, assessment.cost
        ),
        Err(reason) => println!("nominal: REJECTED `{reason}`"),
    }

    let mut rng = sampling::Rng::seed(seed);
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut first_offender: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut accepted = 0usize;

    for _ in 0..samples {
        let candidate = sampling::draw_one(&bounds, &mut rng);
        let x = candidate.to_array();
        match assess_candidate(&objective, &x) {
            Ok(_) => accepted += 1,
            Err(reason) => {
                *counts.entry(reason.clone()).or_default() += 1;
                first_offender.entry(reason).or_insert(x);
            }
        }
    }

    println!("\naccepted {accepted} / {samples}");
    println!("rejection_reason_counts:");
    let mut ordered: Vec<(&String, &usize)> = counts.iter().collect();
    ordered.sort_by(|left, right| right.1.cmp(left.1).then_with(|| left.0.cmp(right.0)));
    for (reason, count) in &ordered {
        println!(
            "  {reason:<38} {count:>5}  ({:.1} %)",
            (**count as f64) * 100.0 / samples as f64
        );
    }

    // The dominant reason, variable by variable against its own bounds, so a
    // reader can see which coordinate left its domain rather than inferring
    // it. `at` marks a variable sitting on a bound.
    if let Some((reason, _)) = ordered.first() {
        let Some(x) = first_offender.get(*reason) else {
            return;
        };
        println!("\nfirst candidate rejected as `{reason}`:");
        println!(
            "  {:<28} {:>14} {:>14} {:>14}  unit",
            "variable", "lower", "value", "upper"
        );
        for (index, spec) in SPECS.iter().enumerate() {
            let (lower, upper) = bounds.get(index).copied().unwrap_or((f64::NAN, f64::NAN));
            let value = x.get(index).copied().unwrap_or(f64::NAN);
            let span = upper - lower;
            let at = if span > 0.0 && (value - lower).abs() <= span * 1.0e-9 {
                " at lower"
            } else if span > 0.0 && (upper - value).abs() <= span * 1.0e-9 {
                " at upper"
            } else {
                ""
            };
            println!(
                "  {:<28} {lower:>14.5} {value:>14.5} {upper:>14.5}  {}{at}",
                spec.name, spec.unit
            );
        }
        let _ = DesignVector::from_array(x);
    }
}
