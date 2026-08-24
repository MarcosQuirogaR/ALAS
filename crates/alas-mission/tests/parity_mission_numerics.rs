// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! `alas-mission::numerics` against `golden/mission/numerics.json`.
//!
//! Three checks, at two tiers. The container's defaults are copied constants,
//! compared at `exact`. The dimensionless Chebyshev operators and their
//! time-rescaled forms ride a matrix inverse, so they are compared at `linalg`,
//! the tier the row and the underlying `alas-math::chebyshev` both carry. Each
//! time case feeds the exact inertial-time array the reference used, so the
//! rescale is checked on the same span rather than a reconstructed one.

use alas_mission::Numerics;
use alas_testkit::{load_json, Comparison, Tier};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    defaults: Defaults,
    dimensionless: Ops,
    time_cases: Vec<TimeCase>,
}

#[derive(Deserialize)]
struct Defaults {
    number_control_points: i64,
    tolerance_solution: f64,
    solver_jacobian: String,
    max_evaluations: f64,
    tag: String,
}

#[derive(Deserialize)]
struct Ops {
    control_points: Vec<f64>,
    differentiate: Vec<Vec<f64>>,
    integrate: Vec<Vec<f64>>,
}

#[derive(Deserialize)]
struct TimeCase {
    time: Vec<f64>,
    control_points: Vec<f64>,
    differentiate: Vec<Vec<f64>>,
    integrate: Vec<Vec<f64>>,
}

fn compare_matrix(cmp: &mut Comparison, name: &str, actual: &[Vec<f64>], expected: &[Vec<f64>]) {
    if actual.len() != expected.len() {
        cmp.exact(&format!("{name}.rows"), &actual.len(), &expected.len());
        return;
    }
    for (row, (a, e)) in actual.iter().zip(expected).enumerate() {
        cmp.slice(&format!("{name}[{row}]"), a, e);
    }
}

// The test asserts on values it built from a fixture it wrote, so a failed
// expect is a broken checkout rather than a library invariant.
#[allow(clippy::expect_used)]
#[test]
fn numerics_container_and_operators_match_the_reference() {
    let fixture: Fixture =
        serde_json::from_value(load_json("mission", "numerics")).expect("fixture shape");

    let defaults = Numerics::default();
    let mut discrete = Comparison::new("numerics defaults", Tier::Exact);
    discrete.exact(
        "number_control_points",
        &defaults.number_control_points,
        &fixture.defaults.number_control_points,
    );
    discrete.exact(
        "solver_jacobian",
        &defaults.solver_jacobian,
        &fixture.defaults.solver_jacobian,
    );
    discrete.exact("tag", &Numerics::TAG.to_owned(), &fixture.defaults.tag);
    discrete.scalar(
        "tolerance_solution",
        defaults.tolerance_solution,
        fixture.defaults.tolerance_solution,
    );
    discrete.scalar(
        "max_evaluations",
        defaults.max_evaluations,
        fixture.defaults.max_evaluations,
    );
    discrete.finish();

    let mut operators = Comparison::new("numerics operators", Tier::Linalg);

    let mut numerics = Numerics::default();
    numerics
        .initialize_differentials_dimensionless()
        .expect("sixteen control points");
    operators.slice(
        "dimensionless.control_points",
        &numerics.dimensionless.control_points,
        &fixture.dimensionless.control_points,
    );
    compare_matrix(
        &mut operators,
        "dimensionless.differentiate",
        &numerics.dimensionless.differentiate,
        &fixture.dimensionless.differentiate,
    );
    compare_matrix(
        &mut operators,
        "dimensionless.integrate",
        &numerics.dimensionless.integrate,
        &fixture.dimensionless.integrate,
    );

    for (index, case) in fixture.time_cases.iter().enumerate() {
        numerics.update_differentials_time(&case.time);
        operators.slice(
            &format!("time[{index}].control_points"),
            &numerics.time.control_points,
            &case.control_points,
        );
        compare_matrix(
            &mut operators,
            &format!("time[{index}].differentiate"),
            &numerics.time.differentiate,
            &case.differentiate,
        );
        compare_matrix(
            &mut operators,
            &format!("time[{index}].integrate"),
            &numerics.time.integrate,
            &case.integrate,
        );
    }

    operators.finish();
}
