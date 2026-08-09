// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas_math::CubicBSpline` against CasADi's `bspline` interpolant,
//! which is the object AeroSandbox's default atmosphere is made of.
//!
//! Two comparisons, for the reason the fixture's generator states: the
//! evaluated values are CasADi's own, and the knot vector and coefficients
//! are SciPy's representation of the same not-a-knot spline. Values alone
//! would not distinguish this spline from one with the knots placed a data
//! point over, since both reproduce the data they were built from.
//!
//! Compared at the `linalg` tier: the coefficients come out of a dense
//! elimination here and out of a banded LAPACK solve in SciPy, and neither
//! pivots the way CasADi's own solver does.

// This file is itself a test binary, so an unwrap that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_math::CubicBSpline;
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    name: String,
    x: Vec<f64>,
    y: Vec<f64>,
    query_x: Vec<f64>,
    evaluated: Vec<f64>,
    knots: Vec<f64>,
    coefficients: Vec<f64>,
}

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

fn fixture() -> Fixture {
    alas_testkit::load("math", "bspline")
}

fn build(case: &Case) -> CubicBSpline {
    CubicBSpline::interpolate(&case.x, &case.y).expect("the fixture's cases are all well-formed")
}

#[test]
fn every_queried_point_matches_casadi() {
    let fixture = fixture();
    assert!(!fixture.cases.is_empty(), "the fixture has no cases");

    let mut comparison = Comparison::new("alas-math::bspline values", Tier::Linalg);
    for case in &fixture.cases {
        let spline = build(case);
        for (query, &expected) in case.query_x.iter().zip(&case.evaluated) {
            comparison.scalar(
                &format!("{} at x={query}", case.name),
                spline.evaluate(*query),
                expected,
            );
        }
    }
    comparison.finish();
}

#[test]
fn the_knots_are_placed_where_the_not_a_knot_rule_places_them() {
    // The knots are chosen, not computed, so they are compared at `exact`:
    // any difference at all is a different spline space, not a rounding
    // difference in the same one.
    let fixture = fixture();

    let mut comparison = Comparison::new("alas-math::bspline knots", Tier::Exact);
    for case in &fixture.cases {
        let spline = build(case);
        let actual = spline.knots();
        assert_eq!(
            actual.len(),
            case.knots.len(),
            "{}: knot vector has {} entries, expected {}",
            case.name,
            actual.len(),
            case.knots.len()
        );
        for (index, (&actual, &expected)) in actual.iter().zip(&case.knots).enumerate() {
            comparison.scalar(&format!("{} knot {index}", case.name), actual, expected);
        }
    }
    comparison.finish();
}

#[test]
fn the_coefficients_match_scipys_representation_of_the_same_spline() {
    let fixture = fixture();

    let mut comparison = Comparison::new("alas-math::bspline coefficients", Tier::Linalg);
    for case in &fixture.cases {
        let spline = build(case);
        let actual = spline.coefficients();
        assert_eq!(
            actual.len(),
            case.coefficients.len(),
            "{}: {} coefficients, expected {}",
            case.name,
            actual.len(),
            case.coefficients.len()
        );
        for (index, (&actual, &expected)) in actual.iter().zip(&case.coefficients).enumerate() {
            comparison.scalar(
                &format!("{} coefficient {index}", case.name),
                actual,
                expected,
            );
        }
    }
    comparison.finish();
}

#[test]
fn the_fixture_covers_the_smallest_dataset_and_the_atmospheres_own_grid() {
    // The atmosphere grid is the case the module exists for, and the
    // four-point case is the one with no interior knots at all. A fixture
    // that lost either would still pass every comparison above.
    let fixture = fixture();
    for required in [
        "smallest",
        "atmosphere_temperature",
        "atmosphere_log_pressure",
    ] {
        assert!(
            fixture.cases.iter().any(|case| case.name == required),
            "the fixture is missing the {required} case"
        );
    }
    let atmosphere = fixture
        .cases
        .iter()
        .find(|case| case.name == "atmosphere_temperature")
        .expect("checked just above");
    assert_eq!(
        atmosphere.x.len(),
        38,
        "the differentiable atmosphere is fitted at 38 altitudes"
    );
}
