// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Parity test for `alas-opt::sampling`.

// A test asserts on values it constructed or loaded from a fixture it controls, so a failed unwrap is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::AlasConfig;
use alas_opt::sampling::{draw_one, error_issues, sample_design, widened_bounds, Rng};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct BoundPair {
    lo: f64,
    hi: f64,
}

#[derive(Debug, Deserialize)]
struct WidenedCase {
    label: String,
    slack: f64,
    bounds: Vec<BoundPair>,
    widened_lo: Vec<f64>,
    widened_hi: Vec<f64>,
}

#[derive(Debug, Deserialize)]
struct ValidationTest {
    default_config_error_count: usize,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    widened_cases: Vec<WidenedCase>,
    validation_test: ValidationTest,
}

#[test]
fn parity_sampling() {
    let fixture: Fixture = alas_testkit::load("opt", "sampling");

    // 1. Widened bounds test
    let mut comp = Comparison::new("widened_bounds", Tier::Closed);
    for case in &fixture.widened_cases {
        let input_bounds: Vec<(f64, f64)> = case.bounds.iter().map(|b| (b.lo, b.hi)).collect();
        let widened = widened_bounds(&input_bounds, case.slack);

        for (i, &(w_lo, w_hi)) in widened.iter().enumerate() {
            comp.scalar(
                &format!("{}_lo_{}", case.label, i),
                w_lo,
                case.widened_lo[i],
            );
            comp.scalar(
                &format!("{}_hi_{}", case.label, i),
                w_hi,
                case.widened_hi[i],
            );
        }
    }
    comp.finish();

    // 2. Validation error count test
    let config = AlasConfig::default();
    let issues = error_issues(&config);
    assert_eq!(
        issues.len(),
        fixture.validation_test.default_config_error_count
    );

    // 3. Sampling reproducibility test
    let mut rng1 = Rng::seed(42);
    let mut rng2 = Rng::seed(42);
    let dv1 = draw_one(
        &alas_config::design_variables::DesignVector::bounds(),
        &mut rng1,
    );
    let dv2 = draw_one(
        &alas_config::design_variables::DesignVector::bounds(),
        &mut rng2,
    );
    assert_eq!(dv1, dv2);

    let mut rng_sample = Rng::seed(123);
    let sampled = sample_design(None, Some(&config), &mut rng_sample, 100);
    assert!(sampled.span_m >= 60.0 && sampled.span_m <= 80.0);
}
