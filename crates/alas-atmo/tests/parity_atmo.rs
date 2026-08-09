// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares every quantity `Atmosphere` reports against AeroSandbox's
//! `Atmosphere(method="isa")`.
//!
//! The fixture walks every ISA table layer boundary, a point inside each
//! layer, a point below the table's first base altitude and two points above
//! its last one, so that a shifted layer index or an off-by-one in the table
//! walk shows up as a disagreement rather than passing on whichever cases
//! happened to be checked. Compared at the `closed` tier: both
//! implementations evaluate the same closed-form expressions in the same
//! order, so any difference has to be a handful of ulps of accumulated
//! rounding, not a translation error.
//!
//! Every case here constructs `Atmosphere::isa`, never `Atmosphere::new`.
//! The default model is the fitted one, and `parity_atmo_differentiable.rs`
//! is what checks that branch; a case that reached for the default here
//! would silently compare the fit against closed-form reference values and
//! fail by about a per cent.

// This file is itself a test binary, so an unwrap that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_atmo::{Atmosphere, DensityAltitudeMethod};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    altitude_m: f64,
    temperature_deviation_k: f64,
    knudsen_length_m: f64,
    pressure_pa: f64,
    temperature_k: f64,
    density_kg_m3: f64,
    speed_of_sound_m_s: f64,
    dynamic_viscosity_pa_s: f64,
    kinematic_viscosity_m2_s: f64,
    ratio_of_specific_heats: f64,
    mean_free_path_m: f64,
    knudsen_number: f64,
    density_altitude_m: f64,
}

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

#[test]
fn every_derived_quantity_matches_aerosandbox_isa() {
    let fixture: Fixture = alas_testkit::load("atmo", "isa");
    assert!(!fixture.cases.is_empty(), "the fixture has no cases");

    let mut comparison = Comparison::new("alas-atmo", Tier::Closed);
    for case in &fixture.cases {
        let atmo = Atmosphere::isa(case.altitude_m)
            .with_temperature_deviation(case.temperature_deviation_k);
        let label = format!(
            "altitude={} temperature_deviation={}",
            case.altitude_m, case.temperature_deviation_k
        );

        comparison
            .scalar(
                &format!("{label} pressure"),
                atmo.pressure(),
                case.pressure_pa,
            )
            .scalar(
                &format!("{label} temperature"),
                atmo.temperature(),
                case.temperature_k,
            )
            .scalar(
                &format!("{label} density"),
                atmo.density(),
                case.density_kg_m3,
            )
            .scalar(
                &format!("{label} speed_of_sound"),
                atmo.speed_of_sound(),
                case.speed_of_sound_m_s,
            )
            .scalar(
                &format!("{label} dynamic_viscosity"),
                atmo.dynamic_viscosity(),
                case.dynamic_viscosity_pa_s,
            )
            .scalar(
                &format!("{label} kinematic_viscosity"),
                atmo.kinematic_viscosity(),
                case.kinematic_viscosity_m2_s,
            )
            .scalar(
                &format!("{label} ratio_of_specific_heats"),
                atmo.ratio_of_specific_heats(),
                case.ratio_of_specific_heats,
            )
            .scalar(
                &format!("{label} mean_free_path"),
                atmo.mean_free_path(),
                case.mean_free_path_m,
            )
            .scalar(
                &format!("{label} knudsen"),
                atmo.knudsen(case.knudsen_length_m),
                case.knudsen_number,
            )
            .scalar(
                &format!("{label} density_altitude"),
                atmo.density_altitude(DensityAltitudeMethod::Approximate)
                    .expect("the approximate method is implemented"),
                case.density_altitude_m,
            );
    }
    comparison.finish();
}

#[test]
fn the_fixture_covers_every_isa_table_layer_boundary() {
    let fixture: Fixture = alas_testkit::load("atmo", "isa");
    let altitudes: Vec<f64> = fixture.cases.iter().map(|case| case.altitude_m).collect();

    for boundary in [
        0.0, 11_000.0, 20_000.0, 32_000.0, 47_000.0, 51_000.0, 71_000.0, 84_852.0,
    ] {
        assert!(
            altitudes.contains(&boundary),
            "the fixture is missing the layer boundary at {boundary} m"
        );
    }
    assert!(
        altitudes.iter().any(|&a| a < 0.0),
        "the fixture is missing a below-the-table extrapolation case"
    );
    assert!(
        altitudes.iter().any(|&a| a > 84_852.0),
        "the fixture is missing an above-the-table extrapolation case"
    );
}
