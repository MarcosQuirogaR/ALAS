// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares every quantity `Atmosphere` reports against AeroSandbox's
//! `Atmosphere()` at its default `"differentiable"` method.
//!
//! This is the branch that matters. Upstream's default is the fit, so this is
//! the model every module that writes `asb.Atmosphere(altitude=...)` gets,
//! and it disagrees with the closed form by about a per cent: `parity_atmo.rs`
//! is the other branch and neither substitutes for the other.
//!
//! Compared at the `linalg` tier. The fit's coefficients come out of a dense
//! Gaussian elimination here and out of CasADi's own solver upstream, which
//! do not pivot identically; the tier table names exactly this case ("anything
//! through a factorization, spline fit or least squares"). Every case below
//! in fact agrees to better than 1e-12, which is what makes the `closed`
//! tier still reachable for the disciplines built on top of this, but the
//! tier describes the construction and not today's margin, and a knot grid
//! spanning seven million metres is not a place to bet on the last two digits
//! surviving a different platform's `pow`.
//!
//! The fixture covers points strictly between fitted altitudes, since at the
//! fitted altitudes themselves the fit reproduces the ISA and a spline with
//! the knots placed a data point over would agree there too.

// This file is itself a test binary, so an unwrap that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_atmo::{altitude_knots_m, Atmosphere, DensityAltitudeMethod, Method};
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
    altitude_knots_m: Vec<f64>,
    cases: Vec<Case>,
}

fn fixture() -> Fixture {
    alas_testkit::load("atmo", "differentiable")
}

/// The altitudes upstream's construction *assigns* rather than computes: the
/// sixteen hand-picked ones, and the two endpoints of each geometric fan,
/// which `numpy.geomspace` overwrites with its `start` and `stop` arguments
/// after the logarithmic pass. Anything else in the grid is `10**x` for some
/// interior `x`.
const ASSIGNED_ALTITUDES_M: [f64; 20] = [
    0.0,
    5e3,
    10e3,
    13e3,
    18e3,
    22e3,
    30e3,
    34e3,
    45e3,
    49e3,
    53e3,
    69e3,
    73e3,
    77e3,
    83e3,
    87e3,
    87e3 + 5e3,
    87e3 + 2000e3,
    -5e3,
    -5000e3,
];

#[test]
fn the_fitted_altitudes_are_the_ones_aerosandbox_fits_at() {
    // The grid is computed from upstream's construction rather than
    // transcribed, so this is what makes computing it safe: a different grid
    // is a different atmosphere, and every value below would move with it.
    //
    // Two tiers, because the grid is two different kinds of number. The
    // altitudes upstream assigns (the hand-picked list and each fan's
    // endpoints) are copied, not computed, and are compared at `exact`.
    // The interior fan points are `10**x` evaluated by two different libm
    // implementations, and one of the thirty-eight (418,445.4 m, an altitude
    // 400 km up that exists only to keep an optimizer's gradients finite)
    // lands one ulp apart. That is `closed`, not `exact`: `exact` is for
    // values that are copied rather than computed, and a transcendental
    // function's last bit is not portable. The resulting perturbation of the
    // fit is 1e-16 relative, four orders under the tier the values below are
    // compared at.
    let fixture = fixture();
    let actual = altitude_knots_m();

    assert_eq!(
        actual.len(),
        fixture.altitude_knots_m.len(),
        "the grid has {} altitudes, expected {}",
        actual.len(),
        fixture.altitude_knots_m.len()
    );

    let mut assigned = Comparison::new("alas-atmo::differentiable assigned knots", Tier::Exact);
    let mut computed = Comparison::new("alas-atmo::differentiable computed knots", Tier::Closed);
    let mut assigned_seen = 0;
    for (index, (&actual, &expected)) in actual.iter().zip(&fixture.altitude_knots_m).enumerate() {
        if ASSIGNED_ALTITUDES_M.contains(&expected) {
            assigned_seen += 1;
            assigned.scalar(&format!("altitude knot {index}"), actual, expected);
        } else {
            computed.scalar(&format!("altitude knot {index}"), actual, expected);
        }
    }
    assert_eq!(
        assigned_seen,
        ASSIGNED_ALTITUDES_M.len(),
        "the reference grid is missing an altitude the construction assigns \
         outright, so the split between the two tiers below is wrong"
    );
    assigned.finish();
    computed.finish();
}

#[test]
fn every_derived_quantity_matches_aerosandboxs_default_atmosphere() {
    let fixture = fixture();
    assert!(!fixture.cases.is_empty(), "the fixture has no cases");

    let mut comparison = Comparison::new("alas-atmo::differentiable", Tier::Linalg);
    for case in &fixture.cases {
        let atmo = Atmosphere::new(case.altitude_m)
            .with_temperature_deviation(case.temperature_deviation_k);
        assert_eq!(
            atmo.method,
            Method::Differentiable,
            "Atmosphere::new must reproduce upstream's default method"
        );
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
fn the_fixture_samples_between_the_fitted_altitudes_and_not_only_at_them() {
    // At a fitted altitude the spline returns the ISA value, which any
    // interpolating cubic through the same data would. Everything this
    // fixture actually proves lives between them.
    let fixture = fixture();
    let knots = &fixture.altitude_knots_m;

    let between = fixture
        .cases
        .iter()
        .filter(|case| !knots.contains(&case.altitude_m))
        .count();
    assert!(
        between > 50,
        "only {between} cases fall between fitted altitudes; the fixture \
         cannot distinguish this spline from one with the knots elsewhere"
    );

    // And the flight envelope specifically, which is what every consumer of
    // this crate samples.
    let in_envelope = fixture
        .cases
        .iter()
        .filter(|case| (0.0..=15_000.0).contains(&case.altitude_m))
        .count();
    assert!(
        in_envelope > 40,
        "only {in_envelope} cases fall in the 0-15 km band"
    );
}
