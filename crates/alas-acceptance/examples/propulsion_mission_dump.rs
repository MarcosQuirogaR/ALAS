// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Per-preset propulsion-relevant mission numbers of the native flown mission:
//! the initial climb, the cruise throttle (how much of the
//! maximum-climb rating the cruise needs), the descents and the trip totals.
//!
//! `cargo run -p alas-acceptance --example propulsion_mission_dump [name...]`
//! runs the registered presets (or those named) in the fixed-requirement mode,
//! prints one block per preset and nothing else.

#![allow(clippy::print_stdout, clippy::expect_used)]

use alas_acceptance::plausibility::case::{case_config, run_pipeline};
use alas_config::{presets, MtowSizing};

const FT: f64 = 0.3048;

fn main() {
    // The pipeline needs more than the main thread's stack.
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(run)
        .expect("spawn")
        .join()
        .expect("join");
}

fn run() {
    let wanted: Vec<String> = std::env::args().skip(1).collect();
    for name in presets::available() {
        if !wanted.is_empty() && !wanted.iter().any(|w| w == name) {
            continue;
        }
        let config = match case_config(name, None, MtowSizing::FixedRequirement) {
            Ok(config) => config,
            Err(error) => {
                println!("{name}: CONFIG ERROR {error}");
                continue;
            }
        };
        let result = match run_pipeline(&config) {
            Ok(result) => result,
            Err(error) => {
                println!("{name}: RUN ERROR {error}");
                continue;
            }
        };
        let Some(mission) = result.mission_result.as_ref() else {
            println!("{name}: no mission");
            continue;
        };
        println!("== {name}");
        let summary = mission.completed_summary();
        if let Some(s) = summary {
            println!(
                "  block {:.1} min, trip fuel {:.0} kg, distance {:.0} km",
                s.block_time_s / 60.0,
                s.trip_fuel_kg,
                s.distance_flown_m / 1000.0
            );
        }
        for segment in &mission.segments {
            let c = &segment.conditions;
            let n = c.time_s.len();
            if n == 0 {
                continue;
            }
            let dt_min = (c.time_s[n - 1] - c.time_s[0]) / 60.0;
            let dh_ft = (c.altitude_m[n - 1] - c.altitude_m[0]) / FT;
            let rate = if dt_min > 0.0 { dh_ft / dt_min } else { 0.0 };
            let (mut thr_min, mut thr_max) = (f64::INFINITY, f64::NEG_INFINITY);
            for &t in &c.throttle {
                thr_min = thr_min.min(t);
                thr_max = thr_max.max(t);
            }
            println!(
                "  {:<26} h {:>7.0} -> {:>7.0} ft  t {:>6.1} min  rate {:>7.0} ft/min  TAS {:>6.1} -> {:>6.1} m/s  M {:.3} -> {:.3}  throttle {:.3}..{:.3}  mass {:.0} -> {:.0} kg",
                segment.spec.tag,
                c.altitude_m[0] / FT,
                c.altitude_m[n - 1] / FT,
                dt_min,
                rate,
                c.velocity_m_s[0],
                c.velocity_m_s[n - 1],
                c.mach[0],
                c.mach[n - 1],
                thr_min,
                thr_max,
                c.total_mass_kg[0],
                c.total_mass_kg[n - 1],
            );
        }
    }
}
