// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Closed-form qualification of the NASTRAN-95 product path.
//!
//! A clamped thin-walled box beam has analytic deflections and flange stresses
//! (beam theory with the Timoshenko web-shear term) under a tip load, under
//! self-weight, and under the tip load introduced through a `CRBE3` spider.
//! The same shell model is solved through the product card writers at deck
//! length units of 1, 10, 100 and 1000 per metre, the range over which the
//! A320 wingbox stress moved by 25 percent with identical physics. The wingbox
//! comparison against MSC Nastran is code-to-code; this one has a reference
//! solution.
//!
//! The solver comes from `ALAS_NASTRAN95_DIR` / `ALAS_NASTRAN95_RUNTIME`; when
//! it is not set the solves skip, loudly, as the other external-solver rows
//! do. Run with `--nocapture` to see the table.
//!
//! `ALAS_NASTRAN95_BENCH_DIR` (default `C:/n95bench`) is the short work
//! directory: the solver's rigid-format path buffer holds 37 bytes.

#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)
)]

use std::path::PathBuf;

use alas_struct::nastran95::{
    run_cantilever_benchmark, BenchmarkOutcome, CantileverBox, Nastran95Solver,
};

/// Deck length units per metre exercised: SI, the product decimetre, and the
/// two scales at which the wingbox stress moved.
const SCALES: [f64; 4] = [1.0, 10.0, 100.0, 1000.0];

/// Relative tolerance on mid-length deflection against beam theory. The shell
/// model differs from an Euler-Bernoulli/Timoshenko beam by discretisation,
/// shear lag and the point-load introduction even for an exact solver.
const DEFLECTION_TOLERANCE: f64 = 0.05;

/// Relative tolerance on flange stress against beam theory.
const STRESS_TOLERANCE: f64 = 0.05;

fn solver() -> Option<Nastran95Solver> {
    let dir = PathBuf::from(std::env::var("ALAS_NASTRAN95_DIR").ok()?);
    let runtime = std::env::var_os("ALAS_NASTRAN95_RUNTIME").map(PathBuf::from);
    let stage = std::env::var_os("ALAS_NASTRAN95_RF_STAGE")
        .map_or_else(|| PathBuf::from("C:/n95bench-rf"), PathBuf::from);
    Nastran95Solver::from_paths(&dir, runtime.as_deref(), Some(&stage), None)
}

fn solve_all(solver: &Nastran95Solver) -> Vec<BenchmarkOutcome> {
    let base = PathBuf::from(
        std::env::var("ALAS_NASTRAN95_BENCH_DIR").unwrap_or_else(|_| "C:/n95bench".to_owned()),
    );
    let case = CantileverBox::reference();
    SCALES
        .iter()
        .map(|&scale| {
            let work = base.join(format!("L{scale:.0}"));
            run_cantilever_benchmark(solver, &case, scale, &work, 600.0)
                .unwrap_or_else(|error| panic!("L = {scale}: {error}"))
        })
        .collect()
}

#[test]
fn cantilever_box_matches_beam_theory_at_every_deck_length_unit() {
    let Some(solver) = solver() else {
        println!("skipped: ALAS_NASTRAN95_DIR is not set or the solver is incomplete");
        return;
    };
    let outcomes = solve_all(&solver);
    println!("L/m   case     defl[mm]  beam[mm]  err%    flange[MPa]  beam[MPa]  err%");
    for outcome in &outcomes {
        for case in &outcome.cases {
            println!(
                "{:<5} {:<8} {:8.4} {:8.4} {:+7.2} {:11.3} {:10.3} {:+7.2}",
                outcome.length_per_m,
                case.name,
                case.mid_deflection_m * 1e3,
                case.beam_mid_deflection_m * 1e3,
                100.0 * case.deflection_error(),
                case.flange_stress_pa / 1e6,
                case.beam_flange_stress_pa / 1e6,
                100.0 * case.stress_error(),
            );
        }
    }
    // Scale spread per case: the quantity the wingbox swing was reported in.
    for (index, name) in ["tip", "gravity", "spider"].into_iter().enumerate() {
        let spread = |value: fn(&alas_struct::nastran95::LoadCaseOutcome) -> f64| {
            let values = outcomes.iter().map(|o| value(&o.cases[index]));
            let max = values.clone().fold(f64::MIN, f64::max);
            let min = values.fold(f64::MAX, f64::min);
            (max - min) / min.abs()
        };
        println!(
            "scale spread {name:<8}: deflection {:.3} %, stress {:.3} %",
            100.0 * spread(|c| c.mid_deflection_m),
            100.0 * spread(|c| c.flange_stress_pa),
        );
    }
    for outcome in &outcomes {
        assert!(
            outcome.within(DEFLECTION_TOLERANCE, STRESS_TOLERANCE),
            "L = {}: {:?}",
            outcome.length_per_m,
            outcome
                .cases
                .iter()
                .map(|c| (
                    c.name,
                    100.0 * c.deflection_error(),
                    100.0 * c.stress_error()
                ))
                .collect::<Vec<_>>()
        );
    }
}
