// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares the Chebyshev nodes and operators against SUAVE's
//! `chebyshev_data`.
//!
//! The fixture covers `N = 4`, `8` and `16` -- the last of which is what the
//! mission segment solver actually asks for -- with `integration=True`, plus
//! one `N = 8` run with `integration=False` to check that branch produces no
//! `I` at all rather than a zero one.
//!
//! Compared at the `linalg` tier: building `I` goes through a matrix inverse,
//! and SciPy/numpy's LAPACK-backed solve does not pivot identically to the
//! hand-written Gauss-Jordan elimination here, even though both start from
//! the same `D`.

use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Case {
    n: i64,
    integration: bool,
    x: Vec<f64>,
    #[serde(rename = "D")]
    d: Vec<Vec<f64>>,
    #[serde(rename = "I")]
    i: Option<Vec<Vec<f64>>>,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

#[test]
fn chebyshev_data_matches_suave() {
    let fixture: Fixture = alas_testkit::load("math", "chebyshev");
    assert!(!fixture.cases.is_empty(), "fixture has no cases to check");

    for case in &fixture.cases {
        let subject = format!(
            "chebyshev_data(N={}, integration={})",
            case.n, case.integration
        );
        let actual = alas_math::chebyshev_data(case.n, case.integration)
            .unwrap_or_else(|error| panic!("{subject} returned an error: {error}"));

        let mut comparison = Comparison::new(&subject, Tier::Linalg);
        comparison.slice("x", &actual.x, &case.x);

        assert_eq!(
            actual.differentiation.len(),
            case.d.len(),
            "{subject}: D has {} rows, reference has {}",
            actual.differentiation.len(),
            case.d.len()
        );
        for (row, (actual_row, expected_row)) in
            actual.differentiation.iter().zip(&case.d).enumerate()
        {
            comparison.slice(&format!("D[{row}]"), actual_row, expected_row);
        }

        match (&actual.integration, &case.i) {
            (Some(actual_i), Some(expected_i)) => {
                for (row, (actual_row, expected_row)) in actual_i.iter().zip(expected_i).enumerate()
                {
                    comparison.slice(&format!("I[{row}]"), actual_row, expected_row);
                }
            }
            (None, None) => {}
            (actual_i, expected_i) => panic!(
                "{subject}: integration matrix presence disagrees (actual is_some={}, \
                 reference is_some={})",
                actual_i.is_some(),
                expected_i.is_some()
            ),
        }

        comparison.finish();
    }
}
