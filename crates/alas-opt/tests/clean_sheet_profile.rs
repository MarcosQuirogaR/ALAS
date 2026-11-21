// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Clean-sheet evaluation cost and bit-identity harness (E6).
//!
//! `E6_PRESETS=A320-200,ATR72-600` (default: a representative set) selects the
//! presets; every one is assessed once in `DesignMode::CleanSheet` with its
//! registered design vector. Prints the wall time, the stage profile
//! (`ALAS_EVAL_PROFILE` is switched on in-process) and `DUMPCS` lines of every
//! output (`{:?}`, so equal text is equal bits) for `cmp`/sha256 across trees.
//!
//! `cargo test -p alas-opt --test clean_sheet_profile --release -- --ignored --nocapture`

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use alas_config::optimizer::DesignMode;
use alas_config::{presets, AlasConfig};
use alas_opt::{assess_product_candidate, canonicalize_design, stage_profile};
use serde_json::json;
use std::time::Instant;

#[test]
#[ignore = "profile and bit-identity harness: prints, asserts nothing"]
fn clean_sheet_evaluation_cost_and_bits() {
    let only = std::env::var("E6_PRESETS")
        .unwrap_or_else(|_| "DEFAULT,A320-200,ATR72-600,A400M,B787-9,A220-300,DC-10".to_owned());
    stage_profile::enable();
    // `DEFAULT` is the bare `AlasConfig::default()` with the default design
    // vector: the candidate the alas-pipeline dual-solver tests evaluate.
    let mut cases: Vec<(String, AlasConfig, alas_config::DesignVector)> = Vec::new();
    if only.split(',').any(|name| name == "DEFAULT") {
        cases.push((
            "DEFAULT".to_owned(),
            AlasConfig::default(),
            alas_config::DesignVector::default(),
        ));
    }
    for preset in presets::registry() {
        if !only.split(',').any(|name| name == preset.name) {
            continue;
        }
        let mut config = AlasConfig::from_value(&json!({ "preset": preset.name })).unwrap();
        config.optimizer.design_space.mode = DesignMode::CleanSheet;
        cases.push((preset.name.to_owned(), config, preset.design_vector));
    }
    for (name, config, vector) in cases {
        let design = canonicalize_design(&config, vector).unwrap();
        let label = format!("{name}/clean_sheet");
        stage_profile::reset();
        let start = Instant::now();
        let result = assess_product_candidate(&config, &design);
        let wall_s = start.elapsed().as_secs_f64();
        println!("TIME {label} {wall_s:.3} s");
        let report = stage_profile::report();
        for (key, seconds, calls, inclusive) in &report.stages {
            if *calls > 0 {
                println!("STAGE {label} {key} excl {seconds:.3} incl {inclusive:.3} calls {calls}");
            }
        }
        for (key, n) in &report.counters {
            println!("COUNT {label} {key} {n}");
        }
        match result {
            Ok(a) => {
                println!("DUMPCS {label} sized {:?}", a.sized);
                println!("DUMPCS {label} masses {:?}", a.resolved.masses);
                println!("DUMPCS {label} coords {:?}", a.resolved.coords);
                println!(
                    "DUMPCS {label} state {:?} {:?} {:?} {:?} {:?}",
                    a.resolved.design,
                    a.resolved.tail_sizing,
                    a.resolved.cg_x_m,
                    a.resolved.x_neutral_point_m,
                    a.resolved.takeoff_mass_kg
                );
                println!(
                    "DUMPCS {label} scalars {:?} {:?} {:?} {:?} {:?}",
                    a.cost,
                    a.objective_value,
                    a.hard_violation_sum,
                    a.soft_violation_sum,
                    a.hard_feasible
                );
                println!("DUMPCS {label} engine {:?}", a.resolved.sized_engine);
                for row in &a.residuals {
                    println!("DUMPCS {label} row {row:?}");
                }
            }
            Err(reason) => println!("DUMPCS {label} failed {reason}"),
        }
    }
}
