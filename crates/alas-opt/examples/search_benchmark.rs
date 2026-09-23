// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Release-profile benchmark of the staged product search.
//!
//! This is a dispatch-local benchmark artifact, not a product entry point. It
//! measures, for AVE and for every registered aircraft preset:
//!
//! - the analysis-start instant of the optimization stage, and the wall-clock
//!   seconds from it to the search's own termination;
//! - whether the run converged, under the population-spread and
//!   best-feasible-cost stagnation criterion in `search_methods::lshade_de`,
//!   or stopped on the generation budget or a cancellation;
//! - the coupled analyses executed, the reduced-model screening analyses, and
//!   the cache hits that cost nothing;
//! - the winning design's objective, feasibility, takeoff mass, operating
//!   empty mass, block fuel and lift-to-drag, plus the improvement over the
//!   nominal design evaluated by the same objective.
//!
//! Every mass is kg, every length m, every time a wall-clock second measured
//! with [`Instant`] on the calling thread. Output is one JSON object per line
//! so a run can be appended to a log and diffed.
//!
//! Usage:
//!   `cargo run --release -p alas-opt --example search_benchmark -- \
//!      [preset|all|AVE] [workers] [seed] [mode] [washout|wash_in]`

// A diagnostic example: its output is the printed report, and a failed
// unwrap is the probe stopping on an input it cannot run.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use std::time::Instant;

use alas_config::AlasConfig;
use alas_opt::objective::DesignObjective;
use alas_opt::{assess_candidate, DesignOptimizer, OptimizationError};

fn arg<T: std::str::FromStr>(index: usize, fallback: T) -> T {
    std::env::args()
        .nth(index)
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

fn json_number(value: f64) -> String {
    if value.is_finite() {
        format!("{value:.6}")
    } else {
        "null".to_owned()
    }
}

fn main() {
    let target: String = arg(1, "AVE".to_owned());
    let workers: i64 = arg(2, 8);
    let seed: i64 = arg(3, 42);
    // `clean_sheet` lets every variable roam its global bounds;
    // `reference_adaptation` applies the preset envelope (the D09 +/-10 %
    // window on lengths and scales) and keeps the locked variables fixed,
    // which is the mode a registered aircraft is actually optimized in.
    let mode: String = arg(4, "clean_sheet".to_owned());
    // `wash_in` restores the pre-2026-09-16 behaviour by opening the washout
    // window to the design variable's own upper bound, so the effect of the
    // `tip_washout_max` validity limit can be measured against the same build
    // rather than against a differently built tree.
    let twist_policy: String = arg(5, "washout".to_owned());

    let names: Vec<String> = if target.eq_ignore_ascii_case("all") {
        alas_config::presets::available()
            .into_iter()
            .map(ToOwned::to_owned)
            .collect()
    } else {
        vec![target]
    };

    for name in names {
        benchmark(&name, workers, seed, &mode, &twist_policy);
    }
}

// This binary example's only job is to report benchmark progress and
// results on the console for a human running it manually; there is no other
// channel to route this through, so stdout/stderr are the intended sinks,
// not a bypass of library logging.
#[allow(clippy::print_stderr)]
fn benchmark(preset: &str, workers: i64, seed: i64, mode: &str, twist_policy: &str) {
    let mut config = match AlasConfig::from_value(&serde_json::json!({ "preset": preset })) {
        Ok(config) => config,
        Err(error) => {
            println!(r#"{{"preset":"{preset}","error":"{error}"}}"#);
            return;
        }
    };
    config.optimizer.solver.workers = workers;
    config.optimizer.solver.seed = Some(seed);
    if twist_policy == "wash_in" {
        // The declared upper bound of `tip_twist_deg`; opening the window to
        // it is exactly "no washout requirement".
        config.optimizer.plausibility.max_tip_washout_deg = 1.0;
    }
    config.optimizer.design_space.mode = match mode {
        "reference_adaptation" => alas_config::DesignMode::ReferenceAdaptation,
        "baseline_sandbox" => alas_config::DesignMode::BaselineSandbox,
        _ => alas_config::DesignMode::CleanSheet,
    };

    let nominal = alas_config::presets::get(preset)
        .map(|entry| entry.design_vector)
        .unwrap_or_default();

    // The nominal design under the same objective, so an improvement claim is
    // a comparison and not an isolated number.
    let baseline = assess_candidate(&DesignObjective::new(config.clone()), &nominal.to_array());

    let analysis_start = Instant::now();
    let mut optimizer = DesignOptimizer::new(config.clone());
    let mut last_line = String::new();
    let mut progress = |line: &str| {
        if line.starts_with("differential evolution") || line.starts_with("staged scan") {
            eprintln!("  [{preset}] {line}");
        }
        last_line = line.to_owned();
    };
    let outcome = optimizer.run(None, Some(&nominal), Some(&mut progress));
    let elapsed_s = analysis_start.elapsed().as_secs_f64();

    let (baseline_objective, baseline_feasible) = match &baseline {
        Ok(assessment) => (assessment.objective_value, assessment.hard_feasible),
        Err(_) => (f64::NAN, false),
    };

    match outcome {
        Ok(result) => {
            let diagnostics = result.search_diagnostics.clone();
            let winner = alas_opt::assess_product_candidate(&config, &result.best_design);
            let (objective, feasible, tow, oew, fuel, l_over_d, violated) = match &winner {
                Ok(assessment) => (
                    assessment.objective_value,
                    assessment.hard_feasible,
                    assessment.sized.takeoff_mass_kg,
                    assessment.sized.operating_empty_mass_kg,
                    assessment.sized.block_fuel_kg,
                    assessment.sized.lift_to_drag,
                    assessment.violated_hard_ids().join("+"),
                ),
                Err(reason) => (
                    f64::NAN,
                    false,
                    f64::NAN,
                    f64::NAN,
                    f64::NAN,
                    f64::NAN,
                    reason.clone(),
                ),
            };
            let converged = diagnostics.as_ref().is_some_and(|value| value.converged);
            println!(
                r#"{{"preset":"{preset}","mode":"{mode}","twist_policy":"{twist_policy}","status":"ok","converged":{converged},"termination":"{}","analysis_start_to_termination_s":{},"scan_s":{},"search_s":{},"analyses":{},"screening_analyses":{},"screening_feasible":{},"verification_analyses":{},"cache_hits":{},"poll_iterations":{},"workers":{},"poll_block_size":{},"history_evaluations":{},"best_valid":{},"best_cost":{},"objective_value":{},"baseline_objective_value":{},"baseline_feasible":{},"feasible":{},"takeoff_mass_kg":{},"operating_empty_mass_kg":{},"block_fuel_kg":{},"lift_to_drag":{},"violated_hard":"{}","design":{}}}"#,
                result.termination,
                json_number(elapsed_s),
                json_number(
                    diagnostics
                        .as_ref()
                        .map_or(f64::NAN, |value| value.scan_wall_time_s)
                ),
                json_number(
                    diagnostics
                        .as_ref()
                        .map_or(f64::NAN, |value| value.search_wall_time_s)
                ),
                diagnostics
                    .as_ref()
                    .map_or(0, |value| value.analysis_evaluations),
                diagnostics
                    .as_ref()
                    .map_or(0, |value| value.screening_evaluations),
                diagnostics
                    .as_ref()
                    .map_or(0, |value| value.screening_feasible),
                diagnostics
                    .as_ref()
                    .map_or(0, |value| value.verification_evaluations),
                diagnostics.as_ref().map_or(0, |value| value.cache_hits),
                diagnostics
                    .as_ref()
                    .map_or(0, |value| value.poll_iterations),
                diagnostics.as_ref().map_or(0, |value| value.workers),
                diagnostics
                    .as_ref()
                    .map_or(0, |value| value.poll_block_size),
                result.history.n_evaluations(),
                result.best_valid,
                json_number(result.best_cost),
                json_number(objective),
                json_number(baseline_objective),
                baseline_feasible,
                feasible,
                json_number(tow),
                json_number(oew),
                json_number(fuel),
                json_number(l_over_d),
                violated,
                serde_json::to_string(&result.best_design).unwrap_or_default()
            );
        }
        Err(OptimizationError::NoFeasibleDesign(evidence)) => {
            println!(
                r#"{{"preset":"{preset}","mode":"{mode}","twist_policy":"{twist_policy}","status":"no_feasible_design","converged":false,"analysis_start_to_termination_s":{},"evaluated":{},"reasons":{}}}"#,
                json_number(elapsed_s),
                evidence.evaluated_candidates,
                serde_json::to_string(&evidence.rejection_reason_counts).unwrap_or_default()
            );
        }
        Err(error) => {
            println!(
                r#"{{"preset":"{preset}","mode":"{mode}","twist_policy":"{twist_policy}","status":"error","converged":false,"analysis_start_to_termination_s":{},"error":"{error}"}}"#,
                json_number(elapsed_s)
            );
        }
    }
}
