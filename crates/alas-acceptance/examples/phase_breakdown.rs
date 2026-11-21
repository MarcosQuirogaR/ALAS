// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Fuel and time of the priced trip by phase for each preset and MTOW mode, at the dispatched
//! take-off mass: the ground roll, the take-off acceleration, the take-off band, the climb, the
//! level speed changes (restriction acceleration and deceleration), the cruise, the descent and
//! the landing band. Run on two trees and diff to attribute a change in trip fuel.
//!
//! `cargo run -p alas-acceptance --example phase_breakdown -- <preset> [...]`

#![allow(clippy::print_stdout, clippy::expect_used)]

use alas_acceptance::plausibility::case::{case_config, run_pipeline};
use alas_config::MtowSizing;
use alas_opt::mdo::mission_model::StepPhase;

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
                println!("PHASES {preset}@{mode:?} run failed");
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
            let Ok(leg) = model.fly_trip(d.takeoff_mass_kg, range_m) else {
                println!("PHASES {preset}@{mode:?} trip failed");
                continue;
            };
            let mut totals: Vec<(String, f64, f64)> = Vec::new();
            let mut add = |name: String, fuel: f64, time: f64| match totals
                .iter_mut()
                .find(|t| t.0 == name)
            {
                Some(t) => {
                    t.1 += fuel;
                    t.2 += time;
                }
                None => totals.push((name, fuel, time)),
            };
            for s in &leg.steps {
                let speed_change = (s.tas_m_s[1] - s.tas_m_s[0]).abs() > 1.0e-9;
                let level_change =
                    speed_change && (s.altitude_m[1] - s.altitude_m[0]).abs() < 1.0e-9;
                let name = match (s.phase, speed_change, level_change) {
                    (StepPhase::Ground, ..) => "ground",
                    (StepPhase::Takeoff, true, _) => "takeoff_climb",
                    (StepPhase::Takeoff, false, _) => "takeoff_band",
                    (StepPhase::Climb, true, true) => "restriction_acceleration",
                    (StepPhase::Climb, true, false) => "climb_acceleration",
                    (StepPhase::Climb, false, _) => "climb",
                    (StepPhase::Cruise, ..) => "cruise",
                    (StepPhase::Descent, true, true) => "restriction_deceleration",
                    (StepPhase::Descent, ..) => "descent",
                    (StepPhase::Landing, ..) => "landing",
                };
                add(
                    name.to_owned(),
                    s.mass_kg[0] - s.mass_kg[1],
                    s.time_s[1] - s.time_s[0],
                );
            }
            let line: Vec<String> = totals
                .iter()
                .map(|(n, f, t)| format!("{n} {f:.1} kg {t:.0} s"))
                .collect();
            println!(
                "PHASES {preset}@{mode:?} takeoff {:.1} kg: {}",
                d.takeoff_mass_kg,
                line.join("; ")
            );
        }
    }
}
