// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The D02 review covers what the optimizer actually emits.
//!
//! `alas_config::optimizer::policy_review` records one determination per
//! residual identifier, and the value of that record depends entirely on it
//! being complete: a residual with no entry would be a limit no review ever
//! looked at, and a configuration naming it would be rejected as "unknown"
//! rather than as "reviewed and not eligible", which are different
//! statements to a user.
//!
//! These tests drive the real coupled assessment rather than a fixture, so
//! what they hold is the shipped path.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::optimizer::policy_review::{eligible_count, review_for, RelaxationReview};
use alas_config::optimizer::relaxation::{ConstraintRelaxation, RelaxableLimit};
use alas_config::{AlasConfig, DesignVector};
use alas_opt::assess_product_candidate;

#[test]
fn every_residual_the_reference_twin_emits_carries_a_d02_determination() {
    let assessment = assess_product_candidate(&AlasConfig::default(), &DesignVector::default())
        .expect("the reference twin is assessable");
    assert!(
        assessment.residuals.len() > 10,
        "the assessment produced almost nothing, so this test would pass vacuously"
    );
    let mut unreviewed = Vec::new();
    for residual in &assessment.residuals {
        match review_for(residual.id) {
            Some(reviewed) => assert!(
                !reviewed.rationale.trim().is_empty(),
                "{} is reviewed with no recorded reason",
                residual.id
            ),
            None => unreviewed.push(residual.id),
        }
    }
    assert!(
        unreviewed.is_empty(),
        "these residuals reach the search with no D02 determination: {unreviewed:?}"
    );
}

#[test]
fn the_shipped_review_admits_no_limit_the_reference_twin_can_produce() {
    // The D02 outcome, checked against the identifiers a real aircraft
    // actually generates rather than against the register in isolation.
    let assessment = assess_product_candidate(&AlasConfig::default(), &DesignVector::default())
        .expect("the reference twin is assessable");
    for residual in &assessment.residuals {
        let reviewed = review_for(residual.id).expect("covered by the test above");
        assert!(
            !matches!(reviewed.review, RelaxationReview::Eligible { .. }),
            "{} became eligible; that is an engineering decision, not a regression to absorb",
            residual.id
        );
    }
    assert_eq!(eligible_count(), 0);
}

#[test]
fn the_review_gate_rejects_the_policy_the_mechanism_tests_construct() {
    // `tests/constraint_relaxation.rs` exercises the mechanism with an
    // in-memory entry for `wing_area`. That entry is deliberately not a
    // configuration a user could load, and this is what holds the two
    // statements together: the mechanism works, and the reviewed policy
    // still refuses to enable it.
    let policy = ConstraintRelaxation {
        enabled: true,
        allowed_violated_groups: 1,
        eligible: vec![RelaxableLimit {
            id: "wing_area".to_owned(),
            tolerance_fraction: 0.02,
            provenance: "test fixture: not an engineering-reviewed tolerance".to_owned(),
        }],
    };
    assert!(policy.is_active(), "the mechanism accepts the entry");
    assert_eq!(policy.tolerance_for("wing_area"), Some(0.02));
    let message = policy
        .validate()
        .expect_err("but the D02 review refuses the configuration");
    assert!(message.contains("not eligible for relaxation"), "{message}");

    let mut config = AlasConfig::default();
    config.optimizer.relaxation = policy;
    let issues = alas_config::validate(&config);
    assert!(
        issues
            .iter()
            .any(|issue| issue.field_path == "optimizer.relaxation"),
        "loading such a configuration has to be a blocking error, got {issues:?}"
    );
}

#[test]
fn a_run_with_no_relaxation_configured_reports_nothing_relaxed() {
    let assessment = assess_product_candidate(&AlasConfig::default(), &DesignVector::default())
        .expect("the reference twin is assessable");
    assert!(assessment.relaxation.relaxed_ids.is_empty());
    assert_eq!(assessment.relaxation.violated_groups, 0);
    assert!(!AlasConfig::default().optimizer.relaxation.is_active());
}
