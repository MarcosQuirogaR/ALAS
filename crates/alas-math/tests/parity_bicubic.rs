// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas_math::BicubicSpline` against SciPy's
//! `RectBivariateSpline` with `kx = ky = 3` and `s = 0`, which is how SUAVE
//! builds every aerodynamic surrogate the mission flies against.
//!
//! Three levels are compared rather than one. The knot vectors are compared
//! at `exact`, because they are copied out of the data rather than computed
//! and any difference at all is a different interpolation problem. The
//! coefficients and the evaluated values are compared at `linalg`, the tier
//! for a result that has been through a factorization: both implementations
//! solve the same square collocation system, but not with the same pivoting.
//!
//! Comparing the values alone would be weaker than it looks. An
//! implementation that placed the knots at the wrong data points still
//! reproduces every grid value exactly -- interpolation is interpolation --
//! and disagrees only between the nodes, which is most of where the mission
//! actually samples the surface.

// This file is itself a test binary, so an unwrap that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_math::BicubicSpline;
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    name: String,
    x: Vec<f64>,
    y: Vec<f64>,
    z: Vec<Vec<f64>>,
    tx: Vec<f64>,
    ty: Vec<f64>,
    coefficients: Vec<Vec<f64>>,
    query_x: Vec<f64>,
    query_y: Vec<f64>,
    evaluated: Vec<f64>,
}

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

fn fixture() -> Fixture {
    alas_testkit::load("math", "bicubic")
}

fn built(case: &Case) -> BicubicSpline {
    BicubicSpline::interpolate(&case.x, &case.y, &case.z)
        .expect("the fixture's grids are all well-formed")
}

#[test]
fn the_knots_are_placed_where_fitpack_places_them() {
    let mut comparison = Comparison::new("alas-math::bicubic knots", Tier::Exact);
    for case in &fixture().cases {
        let spline = built(case);
        let (tx, ty) = spline.knots();
        comparison
            .slice(&format!("{}: tx", case.name), tx, &case.tx)
            .slice(&format!("{}: ty", case.name), ty, &case.ty);
    }
    comparison.finish();
}

#[test]
fn the_tensor_product_coefficients_match_scipy() {
    let mut comparison = Comparison::new("alas-math::bicubic coefficients", Tier::Linalg);
    for case in &fixture().cases {
        let spline = built(case);
        let expected: Vec<f64> = case.coefficients.iter().flatten().copied().collect();
        comparison.slice(
            &format!("{}: c", case.name),
            spline.coefficients(),
            &expected,
        );
    }
    comparison.finish();
}

#[test]
fn every_queried_point_matches_scipy() {
    let mut comparison = Comparison::new("alas-math::bicubic", Tier::Linalg);
    for case in &fixture().cases {
        let spline = built(case);
        assert_eq!(
            case.query_x.len(),
            case.evaluated.len(),
            "{}: the fixture's query and value lists disagree",
            case.name
        );
        for ((&x, &y), &expected) in case.query_x.iter().zip(&case.query_y).zip(&case.evaluated) {
            comparison.scalar(
                &format!("{} at ({x}, {y})", case.name),
                spline.evaluate(x, y),
                expected,
            );
        }
    }
    comparison.finish();
}

#[test]
fn the_fixture_covers_the_smallest_grid_and_suaves_own_training_grid() {
    // The 4x4 case has no interior knots, so it is the one case where the
    // placement loop runs zero times; the SUAVE grid is the shape every
    // mission number actually depends on. A fixture missing either would
    // leave the two cases that matter most untested.
    let fixture = fixture();
    let named = |name: &str| fixture.cases.iter().find(|case| case.name == name);

    let minimal = named("minimal").expect("the fixture is missing the 4x4 case");
    assert_eq!((minimal.x.len(), minimal.y.len()), (4, 4));

    let suave = named("suave_lift_surrogate").expect("the fixture is missing the SUAVE grid");
    assert_eq!((suave.x.len(), suave.y.len()), (10, 8));
}
