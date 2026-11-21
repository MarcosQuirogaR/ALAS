// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! ALAS-polar against installed-AVL cruise comparison (A1 evidence).
//!
//! `ALAS_AVL_EXE=... cargo run --release -p alas-pipeline --example avl_cruise_probe <preset> [relabel]`
//! runs the full analysis, the AVL linear-cruise comparison, and prints the
//! status and, per schedule point, the ALAS and AVL lift coefficients and their
//! difference. `relabel` switches the compressible lattice off (the pre-A1
//! whole-wing Prandtl-Glauert angle axis) for the before/after comparison.
#![allow(clippy::print_stdout, clippy::print_stderr, missing_docs)]
// Diagnostic over curated fixtures: a failure aborts the run.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use alas_config::{presets, AlasConfig};
use alas_pipeline::avl::run_avl_linear_cruise_comparison_cancellable;
use alas_pipeline::full_analysis::FullAnalysis;
use serde_json::json;

fn main() {
    let mut args = std::env::args().skip(1);
    let name = args.next().expect("preset name");
    let relabel = args.next().is_some_and(|a| a == "relabel");
    let exe = PathBuf::from(std::env::var("ALAS_AVL_EXE").expect("ALAS_AVL_EXE"));
    let mut value = json!({"preset": name});
    if relabel {
        value["analysis"] = json!({"compressible_trim": false});
    }
    let config = AlasConfig::from_value(&value).unwrap();
    let design = presets::get(&name).unwrap().design_vector;
    let report = FullAnalysis::new(config.clone())
        .run(&design, true)
        .unwrap();
    let out = std::env::temp_dir().join(format!("avl-cruise-probe-{name}-{relabel}"));
    let result = run_avl_linear_cruise_comparison_cancellable(
        &report,
        &config,
        &out,
        Some(&exe),
        600.0,
        None,
    );
    println!(
        "{name} ({}): status {} comparison {:?} error {:?}",
        if relabel {
            "relabel"
        } else {
            "compressible lattice"
        },
        result.status.as_str(),
        result.comparison,
        result.error
    );
    let (Some(reference), Some(polar)) = (&result.comparison_reference, &result.polar) else {
        return;
    };
    println!("  phase {} Mach {:.3}", reference.phase, reference.mach);
    println!("  alpha_sent  CL_ALAS  CL_AVL  diff");
    let mut worst = 0.0_f64;
    for ((alpha, cl), point) in reference
        .geometric_alpha_deg
        .iter()
        .zip(&reference.vlm_polar.cl)
        .zip(&polar.points)
    {
        let diff = point.lift_coefficient - cl;
        worst = worst.max(diff.abs());
        println!(
            "  {alpha:8.3}  {cl:8.4}  {:8.4}  {diff:+.4}",
            point.lift_coefficient
        );
    }
    println!("  max |dCL| {worst:.4}");
}
