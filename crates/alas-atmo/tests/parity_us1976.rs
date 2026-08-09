// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares every quantity `us1976::compute_values` reports against SUAVE's
//! `Analyses.Atmospheric.US_Standard_1976.compute_values`.
//!
//! The fixture walks a spread of geometric altitudes covering every segment
//! and both clamped extremes; see `golden/generators/gen_atmo_us1976.py` for
//! why the altitudes are geometric rather than landing exactly on the
//! table's own geopotential breaks. Compared at the `closed` tier: both
//! implementations evaluate the same closed-form segment formula in the same
//! order, so any difference has to be a handful of ulps of accumulated
//! rounding, not a translation error.

// This file is itself a test binary, so an unwrap that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_atmo::us1976_compute_values;
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    altitude_m: f64,
    temperature_deviation_k: f64,
    pressure_pa: f64,
    temperature_k: f64,
    density_kg_m3: f64,
    speed_of_sound_m_s: f64,
    dynamic_viscosity_pa_s: f64,
    kinematic_viscosity_m2_s: f64,
    thermal_conductivity_w_m_k: f64,
    prandtl_number: f64,
}

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

#[test]
fn every_quantity_matches_suave_us1976() {
    let fixture: Fixture = alas_testkit::load("atmo", "us1976");
    assert!(!fixture.cases.is_empty(), "the fixture has no cases");

    let mut comparison = Comparison::new("alas-atmo::us1976", Tier::Closed);
    for case in &fixture.cases {
        let values = us1976_compute_values(case.altitude_m, case.temperature_deviation_k);
        let label = format!(
            "altitude={} temperature_deviation={}",
            case.altitude_m, case.temperature_deviation_k
        );

        comparison
            .scalar(
                &format!("{label} pressure"),
                values.pressure_pa,
                case.pressure_pa,
            )
            .scalar(
                &format!("{label} temperature"),
                values.temperature_k,
                case.temperature_k,
            )
            .scalar(
                &format!("{label} density"),
                values.density_kg_m3,
                case.density_kg_m3,
            )
            .scalar(
                &format!("{label} speed_of_sound"),
                values.speed_of_sound_m_s,
                case.speed_of_sound_m_s,
            )
            .scalar(
                &format!("{label} dynamic_viscosity"),
                values.dynamic_viscosity_pa_s,
                case.dynamic_viscosity_pa_s,
            )
            .scalar(
                &format!("{label} kinematic_viscosity"),
                values.kinematic_viscosity_m2_s,
                case.kinematic_viscosity_m2_s,
            )
            .scalar(
                &format!("{label} thermal_conductivity"),
                values.thermal_conductivity_w_m_k,
                case.thermal_conductivity_w_m_k,
            )
            .scalar(
                &format!("{label} prandtl_number"),
                values.prandtl_number,
                case.prandtl_number,
            );
    }
    comparison.finish();
}

#[test]
fn the_fixture_covers_clamping_at_both_extremes() {
    let fixture: Fixture = alas_testkit::load("atmo", "us1976");
    let altitudes: Vec<f64> = fixture.cases.iter().map(|case| case.altitude_m).collect();

    assert!(
        altitudes.iter().any(|&a| a < -2000.0),
        "the fixture is missing a below-the-floor clamping case"
    );
    assert!(
        altitudes.iter().any(|&a| a > 84_852.0),
        "the fixture is missing an above-the-ceiling clamping case"
    );
}
