// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares the mission turbofan translation against the reference
//! `turbofan_sizing` component network. This unpublished parity test retains
//! the historical fixture and test filename for reproducibility.
//!
//! Every reported quantity is closed-form compressible-flow arithmetic over the
//! ambient state -- read here through `alas_atmo::us1976_compute_values`, the
//! same US Standard 1976 model SUAVE's mission stack uses -- so the whole cycle
//! is checked at `Tier::Closed`, matching `docs/PORTING.md`. The fixture records
//! every intermediate station of both the cruise sizing pass and the
//! sea-level-static replay, on two presets (AVE/GE9X and A320-200/LEAP-1A), so
//! a wrong component anywhere in the chain shows up as its own line rather than
//! hiding inside the final thrust. The per-station `*Record` types and the
//! `compare_*` helpers live in `support`, so neither file crosses the
//! source-length limit.

// This file is a test binary, so an unwrap or expect that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod support;

use alas_prop::mission_turbofan::{size_turbofan, TurbofanInputs, VehicleBuilderParams};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;
use support::{compare_station_set, StationSetRecord};

#[derive(Debug, Deserialize)]
struct Case {
    label: String,
    n_engines: f64,
    bypass_ratio: f64,
    overall_pressure_ratio: f64,
    fan_pressure_ratio: f64,
    turbine_inlet_temperature_k: f64,
    cruise_mach: f64,
    cruise_altitude_m: f64,
    design_thrust_n: f64,
    mass_flow_rate_design_kg_s: f64,
    compressor_nondimensional_massflow: f64,
    sealevel_static_thrust_n_per_engine: f64,
    cruise: StationSetRecord,
    sea_level_static: StationSetRecord,
    sea_level_static_thrust_force_n: f64,
    sea_level_static_vehicle_mass_rate_kg_s: f64,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

#[test]
fn mission_turbofan_sizing_matches_python_across_two_presets() {
    let fixture: Fixture = alas_testkit::load("prop", "suave_turbofan");
    let params = VehicleBuilderParams::reference_compatibility();
    let mut comparison = Comparison::new("alas-prop::mission_turbofan", Tier::Closed);

    for case in &fixture.cases {
        let inputs = TurbofanInputs {
            number_of_engines: case.n_engines,
            bypass_ratio: case.bypass_ratio,
            overall_pressure_ratio: case.overall_pressure_ratio,
            fan_pressure_ratio: case.fan_pressure_ratio,
            turbine_inlet_temperature_k: case.turbine_inlet_temperature_k,
            cruise_mach: case.cruise_mach,
            cruise_altitude_m: case.cruise_altitude_m,
            design_thrust_total_n: case.design_thrust_n,
        };
        let got = size_turbofan(&inputs, &params);
        let l = &case.label;

        comparison.scalar(
            &format!("{l}: design_thrust_n"),
            got.design_thrust_n,
            case.design_thrust_n,
        );
        comparison.scalar(
            &format!("{l}: mass_flow_rate_design_kg_s"),
            got.mass_flow_rate_design_kg_s,
            case.mass_flow_rate_design_kg_s,
        );
        comparison.scalar(
            &format!("{l}: compressor_nondimensional_massflow"),
            got.compressor_nondimensional_massflow,
            case.compressor_nondimensional_massflow,
        );
        comparison.scalar(
            &format!("{l}: sealevel_static_thrust_n_per_engine"),
            got.sealevel_static_thrust_n_per_engine,
            case.sealevel_static_thrust_n_per_engine,
        );
        comparison.scalar(
            &format!("{l}: sea_level_static_thrust_force_n"),
            got.sea_level_static_thrust_force_n,
            case.sea_level_static_thrust_force_n,
        );
        comparison.scalar(
            &format!("{l}: sea_level_static_vehicle_mass_rate_kg_s"),
            got.sea_level_static_vehicle_mass_rate_kg_s,
            case.sea_level_static_vehicle_mass_rate_kg_s,
        );

        compare_station_set(
            &mut comparison,
            &format!("{l}: cruise"),
            &got.cruise,
            &case.cruise,
        );
        compare_station_set(
            &mut comparison,
            &format!("{l}: sea_level_static"),
            &got.sea_level_static,
            &case.sea_level_static,
        );
    }

    comparison.finish();
}
