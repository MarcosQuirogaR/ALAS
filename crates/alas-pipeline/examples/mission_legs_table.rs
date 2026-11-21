// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Before/after table of the standard mission on every registered preset's
//! default route: one JSON object per line with the fuel plan the flown load
//! case carries and the native flight's own block time.
//!
//! ```text
//! cargo run --release -p alas-pipeline --example mission_legs_table [PRESET...]
//! ```
//!
//! Units: kilograms and seconds. `native_block_time_s` is the native flight's
//! summed segment time (brake release to touchdown); the plan fuels are the
//! policy quantities of the flown load case (`FuelPlan`): `block_fuel_kg` is
//! taxi-out plus trip plus taxi-in, `reserve_fuel_kg` the contingency,
//! alternate, final reserve and additional fuel. The `native_*` reserve fields
//! are the flown reserve program (absent before the reserves were flown).
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a probe prints its table"
)]

use alas_config::AlasConfig;
use alas_pipeline::{DesignPipeline, PipelineOptions, RunEnvironment};
use serde_json::{json, Value};

fn main() {
    let mut names: Vec<String> = std::env::args().skip(1).collect();
    if names.is_empty() {
        names = alas_config::presets::available()
            .iter()
            .map(|name| (*name).to_owned())
            .collect();
    }
    for name in names {
        println!("{}", row(&name));
    }
}

fn row(name: &str) -> Value {
    let mut value = json!({ "preset": name });
    if std::env::var("MISSION_OLD_FLAPS").is_ok() {
        value["performance"] =
            json!({ "oei_climb_delta_cd": 0.015, "landing_flap_delta_cd": 0.040 });
    }
    let Ok(mut config) = AlasConfig::from_value(&value) else {
        return json!({ "preset": name, "error": "config" });
    };
    config.structures.run_nastran = false;
    config.structures.run_patran_export = false;
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: false,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: None,
        quiet: true,
    };
    let started = std::time::Instant::now();
    let result = match DesignPipeline::new(config).run(&options, &RunEnvironment::default()) {
        Ok(result) => result,
        Err(error) => return json!({ "preset": name, "error": error }),
    };
    let summary = result
        .mission_result
        .as_ref()
        .and_then(|mission| mission.completed_summary());
    let case = result.mission_load_case.as_ref();
    let plan = case.and_then(|case| case.route_case()).map(|c| c.plan);
    let supplement = case.and_then(|case| case.native_supplement.as_deref());
    let reserves = supplement.and_then(|s| s.reserves.as_ref());
    json!({
        "preset": name,
        "feasible": result.feasibility.is_feasible(),
        "takeoff_mass_kg": case.map(|case| case.takeoff_mass_kg),
        "trip_fuel_kg": plan.map(|plan| plan.trip.kg),
        "reserve_fuel_kg": plan.map(|plan| plan.reserve_fuel_kg()),
        "block_fuel_kg": plan.map(|plan| plan.block_fuel_kg()),
        "plan_trip_time_s": plan.map(|plan| plan.trip_time_s),
        "native_trip_fuel_kg": summary.map(|s| s.trip_fuel_kg),
        "native_block_time_s": summary.map(|s| s.block_time_s),
        "native_segments": result.mission_result.as_ref().map(|m| m.segments.len()),
        "native_error": case.and_then(|case| case.native_error.clone()),
        "priced_alternate_kg": plan.map(|plan| plan.alternate.kg),
        "priced_final_reserve_kg": plan.map(|plan| plan.final_reserve.kg),
        "priced_taxi_kg": plan.map(|plan| plan.taxi.kg),
        "native_alternate_kg": reserves.map(|r| r.alternate_fuel_kg),
        "native_hold_kg": reserves.map(|r| r.hold_fuel_kg()),
        "native_taxi_out_kg": supplement.and_then(|s| s.taxi_out.fuel_kg),
        "native_taxi_in_kg": supplement.and_then(|s| s.taxi_in.fuel_kg),
        "reserve_error": supplement.and_then(|s| s.reserve_error.clone()),
        "reserve_phases": reserves.map(|r| r.phases.len()),
        "reserve_level_scale": reserves.map(|r| r.level_scale),
        "wall_s": started.elapsed().as_secs_f64(),
    })
}
