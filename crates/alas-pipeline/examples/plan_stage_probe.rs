// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Flies a config (JSON, with a mission plan) on a great-circle route between two airports
//! (`plan_stage_probe <config.json> <ORIG> <DEST>`) and prints the stage history of the run:
//! distance, mass and the route closure. Diagnostic; no physics.

#![allow(clippy::expect_used, clippy::print_stdout)]

use alas_exec::RunEnvironment;
use alas_pipeline::{DesignPipeline, LoadCaseSelection, PipelineOptions};
use alas_route::route::{Route, RouteSource};

fn main() {
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(run)
        .expect("thread")
        .join()
        .expect("run");
}

fn run() {
    let args: Vec<String> = std::env::args().collect();
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&args[1]).expect("config")).expect("json");
    let mut config = alas_config::AlasConfig::from_value(&value).expect("config loads");
    config.structures.run_nastran = false;
    config.structures.run_patran_export = false;
    config.departure_airport = args[2].clone();
    config.arrival_airport = args[3].clone();
    let origin = alas_config::airports::get(&config.departure_airport).expect("origin");
    let destination = alas_config::airports::get(&config.arrival_airport).expect("destination");
    let mut route = Route::great_circle(
        origin,
        destination,
        config.mission.great_circle_points as usize,
    );
    route.source = RouteSource::SimbriefApi;
    let route_m = route.total_distance_m();
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: false,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: Some(20_260_922),
        quiet: true,
    };
    let result = DesignPipeline::new(config)
        .run_with_environment_and_route(&options, &RunEnvironment::default(), Some(route))
        .expect("the run completes");
    println!("route {:.1} km", route_m / 1e3);
    let mission = result.mission_result.as_ref().expect("mission");
    println!("completed: {}", mission.completed_summary().is_some());
    segments(mission);
    if let Some(case) = &result.mission_load_case {
        match &case.selection {
            LoadCaseSelection::Plan(plan) => println!("plan closure {:?}", plan.closure),
            _ => println!("load case is not a plan"),
        }
        println!("native_error {:?}", case.native_error);
    }
    let history = result
        .feasibility
        .mass_balance
        .as_ref()
        .and_then(|balance| balance.history.as_ref());
    match history {
        Some(history) => {
            let mut total = 0.0;
            for stage in &history.stages {
                total += stage.distance_m;
                println!(
                    "{:>24} {:8.1} km  cum {:8.1} km  mass {:8.0} -> {:8.0}",
                    stage.id,
                    stage.distance_m / 1e3,
                    total / 1e3,
                    stage.mass_start_kg,
                    stage.mass_end_kg
                );
            }
        }
        None => println!(
            "NO HISTORY (mass_balance present: {})",
            result.feasibility.mass_balance.is_some()
        ),
    }
    for finding in &result.feasibility.findings {
        if format!("{:?}", finding.code).contains("Mass")
            || format!("{:?}", finding.code).contains("Plan")
        {
            println!(
                "finding {:?} {:?}: {}",
                finding.code,
                finding.severity,
                finding.message.chars().take(300).collect::<String>()
            );
        }
    }
}

/// Per-segment still-air range and altitudes of a flown mission.
#[allow(dead_code)]
fn segments(mission: &alas_mission::MissionResult) {
    for segment in &mission.segments {
        let c = &segment.conditions;
        let range = c
            .aircraft_range_m
            .last()
            .zip(c.aircraft_range_m.first())
            .map_or(f64::NAN, |(a, b)| a - b);
        let kind = match segment.spec.kind {
            alas_mission::segments::SegmentKind::Cruise { distance_m, .. } => {
                format!("cruise {:.1} km", distance_m / 1e3)
            }
            alas_mission::segments::SegmentKind::Climb { .. } => "climb".to_owned(),
            alas_mission::segments::SegmentKind::Descent { .. } => "descent".to_owned(),
        };
        println!(
            "  seg {:>22} {:>16} flown {:7.1} km  alt {:7.0}->{:7.0} m  v {:5.1}->{:5.1}",
            segment.spec.tag,
            kind,
            range / 1e3,
            c.altitude_m.first().copied().unwrap_or(f64::NAN),
            c.altitude_m.last().copied().unwrap_or(f64::NAN),
            c.velocity_m_s.first().copied().unwrap_or(f64::NAN),
            c.velocity_m_s.last().copied().unwrap_or(f64::NAN),
        );
    }
}
