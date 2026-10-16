// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Diagnostic: the dispatch of a preset's configured route at its declared
//! MTOW, printing the closure's iterates (the kappa lane's A380 baseline
//! flyability NotConverged case).
//!
//! Usage: `dispatch_probe PRESET`
#![allow(
    clippy::expect_used,
    clippy::print_stderr,
    clippy::print_stdout,
    missing_docs
)]

use alas_config::airports::get as get_airport;
use alas_config::AlasConfig;
use alas_mass::dispatch::DispatchLimits;
use alas_mass::fuel_plan::FuelBurnModel;
use alas_opt::mdo::solve_planned_dispatch;
use alas_pipeline::feasibility::assess_fuel_capacity;
use alas_pipeline::fuel_model::report_mission_model;
use alas_pipeline::quick_analysis::{payload_range_corners, report_oew_kg};
use alas_pipeline::FullAnalysis;

fn main() {
    let name = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "A380-800".to_owned());
    let preset = alas_config::presets::get(&name).expect("preset");
    let mut config =
        AlasConfig::from_value(&serde_json::json!({ "preset": name })).expect("config");
    if let Some(margin) = std::env::args()
        .nth(2)
        .and_then(|text| text.parse::<f64>().ok())
    {
        config.drag_model.viscous_margin = margin;
    }
    let quiet = std::env::args().nth(3).is_some();
    let report = FullAnalysis::new(config.clone())
        .run(&preset.design_vector, true)
        .expect("report");
    let model = report_mission_model(&config, &report).expect("model");
    let capacity_kg = assess_fuel_capacity(&config, &report.design, &report)
        .capacity_kg
        .expect("capacity");
    let oew_kg = report_oew_kg(&report);
    let laid_out = report.component_masses["Payload"];
    let origin = get_airport(&config.departure_airport).expect("origin");
    let destination = get_airport(&config.arrival_airport).expect("destination");
    let route_m = alas_route::route::haversine_m(
        origin.latitude_deg,
        origin.longitude_deg,
        destination.latitude_deg,
        destination.longitude_deg,
    );
    let max_payload_kg = payload_range_corners(&config, &report)
        .expect("corners")
        .max_payload_kg;
    let design_nmi = config.optimizer.objective.design_range_nmi;
    let design_m = if design_nmi > 0.0 {
        design_nmi * 1852.0
    } else {
        route_m
    };
    let policy = &config.fuel_policy;
    let mtow_kg = config.requirements.mtow_kg;
    let taxi_kg = model.taxi_fuel_flow_kg_s().expect("taxi") * policy.taxi_time_min * 60.0;
    if std::env::args().nth(3).as_deref() == Some("scan") {
        let tolerance_kg = config.optimizer.objective.sizing_tolerance_kg;
        let limits = DispatchLimits {
            mtow_kg,
            mzfw_kg: None,
            mlw_kg: None,
            usable_capacity_kg: Some(capacity_kg),
        };
        let fine: Vec<f64> = std::env::var("DISPATCH_SCAN_NMI")
            .ok()
            .map(|text| {
                text.split(',')
                    .filter_map(|item| item.parse().ok())
                    .collect()
            })
            .unwrap_or_else(|| {
                vec![
                    1000.0, 2000.0, 3000.0, 4000.0, 5000.0, 6000.0, 7000.0, 8000.0, 8500.0, 9000.0,
                ]
            });
        let zfms: Vec<f64> = std::env::var("DISPATCH_SCAN_ZFM")
            .ok()
            .map(|text| {
                text.split(',')
                    .filter_map(|item| item.parse().ok())
                    .collect()
            })
            .unwrap_or_else(|| vec![oew_kg, oew_kg + laid_out, oew_kg + max_payload_kg]);
        for &zfm in &zfms {
            for &nmi in &fine {
                let ceiling_kg = mtow_kg.min(zfm + capacity_kg - taxi_kg);
                let result = solve_planned_dispatch(
                    &model,
                    zfm,
                    ceiling_kg,
                    nmi * 1852.0,
                    policy,
                    &limits,
                    50,
                    tolerance_kg,
                );
                match result {
                    Ok(solution) => println!(
                        "zfm {zfm:.0} {nmi:.0} nmi: {:?} tow {:.0} iters {}",
                        solution.status,
                        solution.takeoff_mass_kg,
                        solution.iterates.len()
                    ),
                    Err(error) => println!("zfm {zfm:.0} {nmi:.0} nmi: ERROR {error:?}"),
                }
            }
        }
        return;
    }
    for (case, zfm, range_m) in [
        ("configured route", oew_kg + laid_out, route_m),
        ("design mission", oew_kg + max_payload_kg, design_m),
    ] {
        println!("== {case}");
        let ceiling_kg = mtow_kg.min(zfm + capacity_kg - taxi_kg);
        println!("zfm {zfm:.0} mtow {mtow_kg:.0} tank {capacity_kg:.0} ceiling {ceiling_kg:.0} range {:.0} nmi", range_m / 1852.0);
        let limits = DispatchLimits {
            mtow_kg,
            mzfw_kg: None,
            mlw_kg: None,
            usable_capacity_kg: Some(capacity_kg),
        };
        let tolerance_kg = config.optimizer.objective.sizing_tolerance_kg;
        let solution = solve_planned_dispatch(
            &model,
            zfm,
            ceiling_kg,
            range_m,
            policy,
            &limits,
            50,
            tolerance_kg,
        )
        .expect("dispatch");
        println!("status {:?}", solution.status);
        println!(
            "takeoff {:.1} fuel {:.1}",
            solution.takeoff_mass_kg,
            solution.plan.takeoff_fuel_kg()
        );
        for (index, iterate) in solution.iterates.iter().enumerate().filter(|_| !quiet) {
            println!(
                "  it {index:2}: tow {:.1} required fuel {:.1} change {:+.2}",
                iterate.takeoff_mass_kg, iterate.required_takeoff_fuel_kg, iterate.change_kg
            );
        }
    }
}
