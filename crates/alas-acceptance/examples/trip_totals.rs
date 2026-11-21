// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Totals of the priced trip at the dispatched take-off mass, per preset and MTOW mode: trip
//! fuel and time, climb fuel, time and ground distance, descent ground distance. Uses only the
//! trip record every tree has, so it runs on a baseline tree to attribute a change by phase.
//!
//! `cargo run -p alas-acceptance --example trip_totals -- <preset> [...]`

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
    for preset in std::env::args().skip(1) {
        for mode in [MtowSizing::FixedRequirement, MtowSizing::PayloadAdjusted] {
            let Ok(result) = case_config(&preset, None, mode).and_then(|c| run_pipeline(&c)) else {
                continue;
            };
            let report = result
                .baseline_analysis
                .as_ref()
                .or(result.optimized_report.as_ref());
            let dispatch = result.feasibility.fuel_loading.dispatch.as_ref();
            let (Some(report), Some(d)) = (report, dispatch) else {
                continue;
            };
            let Some(range_m) = d.route_distance_m else {
                continue;
            };
            let Ok(model) =
                alas_pipeline::fuel_model::report_route_mission_model(&result.config, report)
            else {
                continue;
            };
            let Ok(f) = model.fly_trip(d.takeoff_mass_kg, range_m) else {
                continue;
            };
            println!(
                "TRIP {preset}@{mode:?} takeoff {:.1} kg range {:.0} km level {:.0} m: trip {:.1} kg {:.0} s; climb {:.1} kg {:.0} s {:.1} km; cruise+descent {:.1} kg; descent {:.1} km",
                d.takeoff_mass_kg,
                range_m / 1e3,
                f.cruise_altitude_m,
                f.leg.fuel_kg,
                f.leg.time_s,
                f.climb_fuel_kg,
                f.climb_time_s,
                f.climb_footprint_m / 1e3,
                f.leg.fuel_kg - f.climb_fuel_kg,
                f.descent_footprint_m / 1e3
            );
        }
    }
}
