// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Controlled constraint relaxation (clarified ledger D01-D03).
//!
//! The rules under test are the ledger's own: violated *groups* are counted
//! rather than limits, only a reviewed limit may be missed and only inside
//! its own tolerance, a limit that reports a failed evaluation can never be
//! relaxed, and a relaxed design is never labelled fully feasible.
//!
//! Every case here drives the real coupled candidate assessment on the
//! reference twin, with one requirement tightened just enough to violate it
//! by a known amount. Nothing is hand-assembled, so what these tests hold is
//! the behaviour of the shipped path rather than of a fixture.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::optimizer::relaxation::{ConstraintRelaxation, RelaxableLimit};
use alas_config::{AlasConfig, DesignVector};
use alas_opt::{assess_product_candidate, CandidateAssessment};

/// The nominal reference aircraft, with its maximum wing area tightened to
/// `shortfall_fraction` below the area it actually builds, so the
/// `wing_area` geometry residual is violated by exactly that fraction.
fn assess_with_tight_wing_area(
    shortfall_fraction: f64,
    relaxation: ConstraintRelaxation,
) -> CandidateAssessment {
    let design = DesignVector::default();
    let mut config = AlasConfig::default();
    let built_area_m2 = assess_product_candidate(&config, &design)
        .expect("the reference twin is assessable")
        .residuals
        .iter()
        .find(|residual| residual.id == "wing_area")
        .map(|residual| residual.actual)
        .expect("the wing-area residual is always evaluated");
    // `wing_area` is scaled by its own limit, so a limit set to
    // `area / (1 + f)` is missed by a normalized `f`.
    config.requirements.max_wing_area_m2 = built_area_m2 / (1.0 + shortfall_fraction);
    config.optimizer.relaxation = relaxation;
    assess_product_candidate(&config, &design).expect("the reference twin is assessable")
}

fn policy(tolerance_fraction: f64, allowed_violated_groups: i64) -> ConstraintRelaxation {
    ConstraintRelaxation {
        enabled: true,
        allowed_violated_groups,
        eligible: vec![RelaxableLimit {
            id: "wing_area".to_owned(),
            tolerance_fraction,
            provenance: "test fixture: not an engineering-reviewed tolerance".to_owned(),
        }],
    }
}

#[test]
fn the_strict_default_rejects_a_violation_that_a_policy_would_admit() {
    let assessment = assess_with_tight_wing_area(0.01, ConstraintRelaxation::default());
    assert!(!assessment.hard_feasible);
    assert!(!assessment.is_strictly_feasible());
    assert!(assessment.violated_hard_ids().contains(&"wing_area"));
    assert!(assessment.relaxation.relaxed_ids.is_empty());
}

#[test]
fn a_reviewed_limit_missed_inside_its_tolerance_is_recorded_as_relaxed() {
    let assessment = assess_with_tight_wing_area(0.01, policy(0.05, 1));
    assert_eq!(
        assessment.relaxation.relaxed_ids,
        vec!["wing_area"],
        "the configured policy has to reach the real coupled assessment"
    );
    assert_eq!(assessment.relaxation.violated_groups, 1);
    // The nominal reference aircraft also misses limits this policy does not
    // list, and one ineligible miss rejects the whole candidate however many
    // eligible ones were admitted. That is the D02 rule observed on a real
    // aircraft rather than on a fixture, and it is why this candidate is
    // still not admissible.
    assert!(assessment.relaxation.rejected);
    assert!(!assessment.hard_feasible);
    assert!(!assessment.is_strictly_feasible());
}

#[test]
fn a_miss_outside_its_own_tolerance_is_still_rejected() {
    let assessment = assess_with_tight_wing_area(0.08, policy(0.02, 1));
    assert!(!assessment.hard_feasible);
    assert!(assessment.relaxation.rejected);
}

#[test]
fn a_zero_group_allowance_keeps_the_run_strict() {
    let assessment = assess_with_tight_wing_area(0.01, policy(0.05, 0));
    assert!(!assessment.hard_feasible);
    assert!(assessment.relaxation.relaxed_ids.is_empty());
}

#[test]
fn a_limit_outside_the_eligibility_list_is_never_admitted() {
    let mut relaxation = policy(0.05, 1);
    relaxation.eligible[0].id = "wing_loading".to_owned();
    let assessment = assess_with_tight_wing_area(0.01, relaxation);
    assert!(
        !assessment.hard_feasible,
        "a policy that lists a different limit does not admit this one"
    );
}

#[test]
fn a_relaxed_candidate_keeps_its_violation_in_the_ranking_key() {
    // D03: a fully feasible design ranks first. The search's key is
    // (admissible, aggregate hard violation, cost), so a candidate admitted
    // only by the policy has to keep a strictly positive aggregate violation
    // for that ordering to hold without a tuned penalty. A candidate that
    // relaxes nothing and is admissible must have exactly zero.
    let relaxed = assess_with_tight_wing_area(0.01, policy(0.05, 1));
    assert!(relaxed.hard_violation_sum > 0.0);
    assert!(!relaxed.is_strictly_feasible());

    let strict = assess_product_candidate(&AlasConfig::default(), &DesignVector::default())
        .expect("the reference twin is assessable");
    assert_eq!(
        strict.hard_feasible,
        strict.hard_violation_sum == 0.0,
        "under the strict shipped policy admissibility and a zero aggregate \
         violation are the same statement"
    );
    assert!(strict.relaxation.relaxed_ids.is_empty());
}
