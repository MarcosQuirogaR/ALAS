// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Print the dispatch of every registered preset in the three MTOW modes: take-off mass, priced
//! trip fuel and time, native trip fuel. Run on two trees and diff the output to list what a
//! change moved.
//!
//! `cargo run -p alas-acceptance --example mode_matrix_dump -- [preset ...]`

#![allow(clippy::print_stdout, clippy::expect_used)]

use alas_acceptance::plausibility::case::{case_config, run_pipeline};
use alas_config::MtowSizing;

fn main() {
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(run)
        .expect("spawn")
        .join()
        .expect("join");
}

fn run() {
    let mut presets: Vec<String> = std::env::args().skip(1).collect();
    if presets.is_empty() {
        presets = [
            "A220-300",
            "A320-200",
            "A340-300",
            "A380-800",
            "A400M",
            "ATR72-600",
            "AVE",
            "B747-400",
            "B787-9",
            "C919",
            "DC-10",
            "E195-E2",
        ]
        .map(str::to_owned)
        .to_vec();
    }
    for preset in presets {
        for mode in [
            MtowSizing::FixedRequirement,
            MtowSizing::MtowBand,
            MtowSizing::PayloadAdjusted,
        ] {
            let label = format!("{preset}@{mode:?}");
            let result = case_config(&preset, None, mode).and_then(|c| run_pipeline(&c));
            match result {
                Ok(result) => {
                    let findings: Vec<_> = result
                        .feasibility
                        .findings
                        .iter()
                        .map(|f| f.code.as_str())
                        .collect();
                    match result.feasibility.fuel_loading.dispatch.as_ref() {
                        Some(d) => println!(
                            "MATRIX {label} outcome {} takeoff_kg {:.1} trip_kg {:.1} trip_min {:.2} native_trip_kg {:?} findings {findings:?}",
                            d.outcome.as_str(),
                            d.takeoff_mass_kg,
                            d.plan.map_or(f64::NAN, |p| p.trip.kg),
                            d.plan.map_or(f64::NAN, |p| p.trip_time_s / 60.0),
                            d.native_check.map(|c| c.native_trip_fuel_kg),
                        ),
                        None => println!("MATRIX {label} no dispatch findings {findings:?}"),
                    }
                }
                Err(error) => println!("MATRIX {label} error {error}"),
            }
        }
    }
}
