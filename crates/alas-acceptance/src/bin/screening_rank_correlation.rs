// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Does the screening model rank candidates like the full in-loop model?
//!
//! Scores the first `M` points of a product run's screening sample (the
//! baseline first, then the seeded space-filling batches over the
//! preset-anchored box) with a [`ScreeningFidelity`] and with the full model,
//! and reports per preset:
//!
//! - Spearman's rho of the search's own ranking key (tier, normalized
//!   violation, objective) between the two models, including separate
//!   analysed and feasible subsets so failed ties cannot mask inversions;
//! - the overlap of the two top-k sets, for k = 29 and k = 10;
//! - Spearman's rho per term over candidates both models analysed: the
//!   mission objective, the closed takeoff mass and each residual's
//!   normalized violation;
//! - feasibility agreement and the mean wall time per candidate of each
//!   model;
//! - the sizing-closure work per candidate of each model against the
//!   baseline's, and how many candidates the search's work cap
//!   (`alas_opt::WORK_CAP_MULTIPLE` times the baseline) would stop.
//!
//! Spearman's rho is the Pearson correlation of average ranks (C. Spearman,
//! "The Proof and Measurement of Association between Two Things," American
//! Journal of Psychology 15(1), 1904, DOI 10.2307/1412159). This is a
//! model-to-model consistency check, not physical validation.
//!
//! ```text
//! screening_rank_correlation [--preset A320-200] [--preset B787-9] [--samples 128]
//!     [--seed 20260930] [--workers 0] [--screening-tolerance-kg KG]
//!     [--chordwise-resolution N] [--steps-per-segment N] [--output FILE]
//!     [--fidelity shipped|full] [--induced-checks screening|full]
//!     [--solver-preset balanced]
//!     [--include-candidates]
//! ```
//!
//! Each `--screening-*`, `--chordwise-resolution` and `--steps-per-segment`
//! flag sets one field of the screening descriptor; the rest stay as
//! configured.

// A command-line experiment: its report goes to standard output and its
// failure to standard error.
#![allow(clippy::print_stderr, clippy::print_stdout)]

use std::collections::BTreeMap;
use std::io;
use std::process::ExitCode;
use std::time::Instant;

use alas_config::{presets, solver_presets, AlasConfig, MtowSizing};
use alas_opt::{
    compare_fidelities, CandidateScore, FidelityPair, ScreeningFidelity, WORK_CAP_MULTIPLE,
};
use serde_json::{json, Value};

#[path = "screening_rank_correlation/candidate_scores.rs"]
mod candidate_scores;
#[path = "screening_rank_correlation/ranking.rs"]
mod ranking;
#[path = "optimization_preset_audit/route_profile.rs"]
mod route_profile;

/// Fixed comparison set size, independent of the time-planned population.
const REFERENCE_TOP_K: usize = 29;

struct Args {
    presets: Vec<String>,
    samples: usize,
    seed: u64,
    workers: usize,
    fidelity: ScreeningFidelity,
    solver_preset: String,
    include_candidates: bool,
    output: Option<String>,
}

fn main() -> ExitCode {
    let values: Vec<String> = std::env::args().skip(1).collect();
    match parse(&values).and_then(|args| run(&args)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("screening_rank_correlation: {error}");
            ExitCode::FAILURE
        }
    }
}

fn parse(values: &[String]) -> io::Result<Args> {
    let invalid = |message: String| io::Error::new(io::ErrorKind::InvalidInput, message);
    let mut args = Args {
        presets: Vec::new(),
        samples: 128,
        seed: 20_260_930,
        workers: 0,
        fidelity: ScreeningFidelity::shipped(),
        solver_preset: "balanced".to_owned(),
        include_candidates: false,
        output: None,
    };
    let mut index = 0;
    while index < values.len() {
        let flag = values[index].as_str();
        if flag == "--include-candidates" {
            args.include_candidates = true;
            index += 1;
            continue;
        }
        let value = values
            .get(index + 1)
            .ok_or_else(|| invalid(format!("{flag} requires a value")))?;
        let number = || invalid(format!("{flag} requires a number, got {value:?}"));
        match flag {
            "--preset" => args.presets.push(value.clone()),
            "--samples" => args.samples = value.parse().map_err(|_| number())?,
            "--seed" => args.seed = value.parse().map_err(|_| number())?,
            "--workers" => args.workers = value.parse().map_err(|_| number())?,
            "--solver-preset" => args.solver_preset = value.clone(),
            "--fidelity" => {
                args.fidelity = match value.as_str() {
                    "full" => ScreeningFidelity::full(),
                    "shipped" => ScreeningFidelity::shipped(),
                    _ => return Err(invalid(format!("{flag} requires shipped or full"))),
                }
            }
            "--induced-checks" => {
                args.fidelity.reduced_induced_checks = match value.as_str() {
                    "screening" => true,
                    "full" => false,
                    _ => return Err(invalid(format!("{flag} requires screening or full"))),
                }
            }
            "--screening-tolerance-kg" => {
                args.fidelity.sizing_tolerance_kg = Some(value.parse().map_err(|_| number())?)
            }
            "--chordwise-resolution" => {
                args.fidelity.chordwise_resolution = Some(value.parse().map_err(|_| number())?)
            }
            "--steps-per-segment" => {
                args.fidelity.steps_per_segment = Some(value.parse().map_err(|_| number())?)
            }
            "--output" => args.output = Some(value.clone()),
            other => return Err(invalid(format!("unrecognized argument {other:?}"))),
        }
        index += 2;
    }
    if args.presets.is_empty() {
        args.presets = presets::registry()
            .iter()
            .map(|preset| preset.name.to_owned())
            .collect();
    }
    for preset in &args.presets {
        presets::get(preset).map_err(|error| invalid(error.to_string()))?;
    }
    if args.samples < 3 {
        return Err(invalid("--samples must be at least three".to_owned()));
    }
    if args.workers == 0 {
        args.workers = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    }
    Ok(args)
}

fn run(args: &Args) -> io::Result<()> {
    let fidelity = args.fidelity;
    let mut report = Vec::new();
    for preset in &args.presets {
        let mut config = AlasConfig::from_value(&json!({ "preset": preset }))
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?;
        config.optimizer.solver = solver_presets::get(&args.solver_preset)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?
            .settings
            .clone();
        config.optimizer.objective.mtow_sizing = MtowSizing::FixedRequirement;
        route_profile::initialize_route_profile(&mut config);
        eprintln!(
            "[rank] preset={preset} samples={} workers={}",
            args.samples, args.workers
        );
        let started = Instant::now();
        let pairs = compare_fidelities(&config, fidelity, args.samples, args.seed, args.workers)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?;
        let nominal = candidate_scores::fixed_nominal_assessment(&config, &pairs);
        let mut row = json!({
            "preset": preset,
            "wall_time_s": started.elapsed().as_secs_f64(),
            "statistics": statistics(&pairs),
            "nominal_assessment": nominal,
            "optimized_nominal_assessment": candidate_scores::nominal_assessment(&pairs),
        });
        if args.include_candidates {
            row["candidate_scores"] = candidate_scores::candidates(&pairs);
        }
        report.push(row);
    }
    let document = json!({
        "experiment": "screening_rank_correlation",
        "screening_fidelity": {
            "sizing_tolerance_kg": fidelity.sizing_tolerance_kg,
            "chordwise_resolution": fidelity.chordwise_resolution,
            "steps_per_segment": fidelity.steps_per_segment,
            "reduced_induced_checks": fidelity.reduced_induced_checks,
        },
        "solver_preset": args.solver_preset,
        "mtow_mode": "fixed_requirement",
        "samples": args.samples,
        "seed": args.seed,
        "workers": args.workers,
        "validation_status": "model-to-model consistency only; not physical validation",
        "presets": report,
    });
    let text = serde_json::to_string_pretty(&document).map_err(io::Error::other)?;
    match &args.output {
        Some(path) => std::fs::write(path, text),
        None => {
            println!("{text}");
            Ok(())
        }
    }
}

fn statistics(pairs: &[FidelityPair]) -> Value {
    let all_pairs: Vec<&FidelityPair> = pairs.iter().collect();
    let all_ranking = ranking::statistics(&all_pairs, REFERENCE_TOP_K);
    let both_analysed: Vec<&FidelityPair> = pairs
        .iter()
        .filter(|pair| pair.screening.analysed && pair.full.analysed)
        .collect();
    let term = |value: &dyn Fn(&CandidateScore) -> f64| {
        let (a, b): (Vec<f64>, Vec<f64>) = both_analysed
            .iter()
            .map(|pair| (value(&pair.screening), value(&pair.full)))
            .filter(|(a, b)| a.is_finite() && b.is_finite())
            .unzip();
        json!({ "n": a.len(), "spearman_rho": spearman(&a, &b), "max_abs_relative_difference": max_relative(&a, &b) })
    };
    let mut residual_ids: BTreeMap<&'static str, (Vec<f64>, Vec<f64>)> = BTreeMap::new();
    for pair in &both_analysed {
        for &(id, screening) in &pair.screening.violations {
            if let Some(&(_, full)) = pair.full.violations.iter().find(|(other, _)| *other == id) {
                let entry = residual_ids.entry(id).or_default();
                entry.0.push(screening);
                entry.1.push(full);
            }
        }
    }
    let residuals: BTreeMap<&str, Value> = residual_ids
        .iter()
        .filter(|(_, (a, b))| a.iter().chain(b).any(|value| *value > 0.0))
        .map(|(id, (a, b))| {
            let role = both_analysed.iter().find_map(|pair| {
                pair.full.residuals.iter().find(|residual| residual.id == *id)
                    .map(|residual| format!("{:?}", residual.role))
            });
            (*id, json!({ "role": role, "n": a.len(), "spearman_rho": spearman(a, b), "violated_screening": a.iter().filter(|v| **v > 0.0).count(), "violated_full": b.iter().filter(|v| **v > 0.0).count() }))
        })
        .collect();
    let mean_wall = |pick: &dyn Fn(&FidelityPair) -> f64| {
        pairs.iter().map(pick).sum::<f64>() / pairs.len().max(1) as f64
    };
    json!({
        "candidates": pairs.len(),
        "analysed_both": both_analysed.len(),
        "feasible_screening": pairs.iter().filter(|p| p.screening.feasible).count(),
        "feasible_full": pairs.iter().filter(|p| p.full.feasible).count(),
        "feasibility_agreement": pairs.iter().filter(|p| p.screening.feasible == p.full.feasible).count(),
        "ranking_key_spearman_rho": all_ranking["ranking_key_spearman_rho"],
        "top_k_overlap": all_ranking["top_k_overlap"],
        "ranking_among_analysed": ranking::statistics(&both_analysed, REFERENCE_TOP_K),
        "ranking_among_feasible": ranking::statistics(
            &pairs.iter().filter(|pair| pair.screening.feasible && pair.full.feasible).collect::<Vec<_>>(),
            REFERENCE_TOP_K),
        "terms": {
            "cost": term(&|score| score.cost),
            "objective_value": term(&|score| score.objective_value),
            "takeoff_mass_kg": term(&|score| score.takeoff_mass_kg),
            "hard_violation": term(&|score| score.hard_violation),
        },
        "residual_violations": residuals,
        "mean_wall_time_per_candidate_s": {
            "screening": mean_wall(&|pair| pair.screening.wall_time_s),
            "full": mean_wall(&|pair| pair.full.wall_time_s),
        },
        "sizing_work": {
            "screening": work(pairs, |pair| &pair.screening),
            "full": work(pairs, |pair| &pair.full),
        },
    })
}

/// Each sized candidate's closure work as a multiple of the baseline's (the
/// first pair), per counter, and how many exceed the search's work cap.
fn work(pairs: &[FidelityPair], pick: impl Fn(&FidelityPair) -> &CandidateScore) -> Value {
    let Some(baseline) = pairs.first().and_then(|pair| pick(pair).work) else {
        return Value::Null;
    };
    let ratio = |value: u64, nominal: u64| value as f64 / nominal.max(1) as f64;
    let (flights, evals): (Vec<f64>, Vec<f64>) = pairs
        .iter()
        .filter_map(|pair| pick(pair).work)
        .map(|work| {
            (
                ratio(work.trip_flights, baseline.trip_flights),
                ratio(work.deck_evals, baseline.deck_evals),
            )
        })
        .unzip();
    let cap = f64::from(WORK_CAP_MULTIPLE);
    let summary = |mut values: Vec<f64>| {
        values.sort_by(f64::total_cmp);
        let rank = |p: f64| {
            let index = (p * values.len() as f64).ceil() as usize;
            values.get(index.clamp(1, values.len()) - 1).copied()
        };
        json!({ "p50": rank(0.5), "p90": rank(0.9), "max": values.last() })
    };
    json!({
        "baseline": { "trip_flights": baseline.trip_flights, "deck_evals": baseline.deck_evals },
        "sized": flights.len(),
        "over_cap": flights.iter().zip(&evals).filter(|(f, e)| **f > cap || **e > cap).count(),
        "trip_flights_over_baseline": summary(flights),
        "deck_evals_over_baseline": summary(evals),
    })
}

/// Average ranks (1-based), ties sharing the mean of their positions.
fn average_ranks(values: &[f64]) -> Vec<f64> {
    let mut order: Vec<usize> = (0..values.len()).collect();
    order.sort_by(|&a, &b| values[a].total_cmp(&values[b]));
    let mut ranks = vec![0.0; values.len()];
    let mut start = 0;
    while start < order.len() {
        let mut end = start + 1;
        while end < order.len() && values[order[end]] == values[order[start]] {
            end += 1;
        }
        let rank = (start + end + 1) as f64 / 2.0;
        for &index in &order[start..end] {
            ranks[index] = rank;
        }
        start = end;
    }
    ranks
}

/// Spearman's rho; `None` with fewer than three pairs or a constant side.
fn spearman(a: &[f64], b: &[f64]) -> Option<f64> {
    if a.len() != b.len() || a.len() < 3 {
        return None;
    }
    let (ra, rb) = (average_ranks(a), average_ranks(b));
    let n = ra.len() as f64;
    let (ma, mb) = (ra.iter().sum::<f64>() / n, rb.iter().sum::<f64>() / n);
    let (mut covariance, mut va, mut vb) = (0.0, 0.0, 0.0);
    for (x, y) in ra.iter().zip(&rb) {
        covariance += (x - ma) * (y - mb);
        va += (x - ma).powi(2);
        vb += (y - mb).powi(2);
    }
    (va > 0.0 && vb > 0.0).then(|| covariance / (va * vb).sqrt())
}

fn max_relative(a: &[f64], b: &[f64]) -> Option<f64> {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).abs() / y.abs().max(1.0e-12))
        .fold(None, |acc: Option<f64>, value| {
            Some(acc.map_or(value, |m| m.max(value)))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_preset_is_rejected_before_configuration_fallback() {
        let input = ["--preset".to_owned(), "unregistered-aircraft".to_owned()];
        let result = parse(&input);
        assert!(result.is_err());
        if let Err(error) = result {
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
            assert!(error.to_string().contains("unregistered-aircraft"));
            assert!(error.to_string().contains("available:"));
        }
    }

    #[test]
    fn every_registered_preset_is_accepted_by_the_cli() {
        for preset in presets::registry() {
            let input = ["--preset".to_owned(), preset.name.to_owned()];
            assert!(parse(&input).is_ok(), "{}", preset.name);
        }
    }

    #[test]
    fn spearman_is_one_for_monotone_data_and_minus_one_when_reversed() {
        let a = [1.0, 2.0, 3.0, 10.0];
        assert_eq!(spearman(&a, &[2.0, 4.0, 8.0, 9.0]), Some(1.0));
        assert_eq!(spearman(&a, &[9.0, 8.0, 4.0, 2.0]), Some(-1.0));
        assert_eq!(spearman(&a, &[1.0, 1.0, 1.0, 1.0]), None);
        assert_eq!(average_ranks(&[5.0, 1.0, 5.0]), vec![2.5, 1.0, 2.5]);
    }

    #[test]
    fn failed_tied_candidates_do_not_establish_rank_correlation() {
        let failed = CandidateScore {
            cost: 1.0,
            feasible: false,
            analysed: false,
            hard_violation: f64::INFINITY,
            objective_value: f64::NAN,
            takeoff_mass_kg: f64::NAN,
            violations: Vec::new(),
            residuals: Vec::new(),
            failure_reason: Some("failed".to_owned()),
            wall_time_s: 0.0,
            work: None,
        };
        let pairs: Vec<FidelityPair> = (0..10)
            .map(|index| FidelityPair {
                design: vec![f64::from(index)],
                screening: failed.clone(),
                full: failed.clone(),
            })
            .collect();
        assert!(statistics(&pairs)["ranking_key_spearman_rho"].is_null());

        let mut reversed = pairs;
        for (index, pair) in reversed.iter_mut().take(3).enumerate() {
            pair.screening.analysed = true;
            pair.full.analysed = true;
            pair.screening.hard_violation = index as f64;
            pair.full.hard_violation = (2 - index) as f64;
        }
        let result = statistics(&reversed);
        assert_eq!(result["ranking_among_analysed"]["n"], 3);
        assert_eq!(
            result["ranking_among_analysed"]["ranking_key_spearman_rho"],
            -1.0
        );
    }
}
