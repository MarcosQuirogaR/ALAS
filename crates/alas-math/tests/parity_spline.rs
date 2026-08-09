// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas_math::CubicSpline` against SciPy's `CubicSpline`.
//!
//! `alas-math::spline` solves a different linear system from SciPy's own
//! (second derivatives rather than first), so this is not a translation
//! check in the usual sense -- both describe the unique cubic spline through
//! the same knots, values and boundary conditions, and this fixture is what
//! proves the from-scratch implementation actually is that spline. Compared
//! at the `linalg` tier: two different linear solves of an equivalent system
//! agree to a handful of ulps, not bit for bit.

// This file is itself a test binary, so an unwrap that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_math::{Boundary, CubicSpline};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    y_shape: String,
    lower: String,
    upper: String,
    y: Vec<Vec<f64>>,
    lower_value: Vec<f64>,
    upper_value: Vec<f64>,
    query_x: Vec<f64>,
    evaluated: Vec<Vec<f64>>,
}

#[derive(Deserialize)]
struct Fixture {
    x: Vec<f64>,
    cases: Vec<Case>,
}

fn boundary<'a>(kind: &str, value: &'a [f64]) -> Boundary<'a> {
    match kind {
        "natural" => Boundary::SecondDerivative(value),
        "clamped" => Boundary::FirstDerivative(value),
        other => panic!("unknown boundary kind in fixture: {other}"),
    }
}

#[test]
fn every_case_matches_scipy_cubicspline() {
    let fixture: Fixture = alas_testkit::load("math", "spline");
    assert!(!fixture.cases.is_empty(), "the fixture has no cases");

    let mut comparison = Comparison::new("alas-math::spline", Tier::Linalg);
    for case in &fixture.cases {
        let spline = CubicSpline::new(
            &fixture.x,
            &case.y,
            boundary(&case.lower, &case.lower_value),
            boundary(&case.upper, &case.upper_value),
        )
        .expect("the fixture's cases are all well-formed");

        let label = format!(
            "y_shape={} lower={} upper={}",
            case.y_shape, case.lower, case.upper
        );
        for (query, expected_row) in case.query_x.iter().zip(&case.evaluated) {
            let actual_row = spline.evaluate(*query);
            assert_eq!(
                actual_row.len(),
                expected_row.len(),
                "{label} at x={query}: dimension mismatch"
            );
            for (dim, (&actual, &expected)) in actual_row.iter().zip(expected_row).enumerate() {
                comparison.scalar(&format!("{label} x={query} dim={dim}"), actual, expected);
            }
        }
    }
    comparison.finish();
}

#[test]
fn the_fixture_covers_every_boundary_condition_combination() {
    let fixture: Fixture = alas_testkit::load("math", "spline");
    for lower in ["natural", "clamped"] {
        for upper in ["natural", "clamped"] {
            assert!(
                fixture
                    .cases
                    .iter()
                    .any(|case| case.lower == lower && case.upper == upper),
                "the fixture is missing lower={lower} upper={upper}"
            );
        }
    }
}
