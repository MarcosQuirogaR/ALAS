// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Convergence has to agree with the aircraft the run delivers.
//!
//! The search's `converged` and the application's feasibility verdict are two
//! different statements, evaluated on two different meshes and two different
//! mission models. Three of four converged application runs measured before
//! this contract existed reported the delivered aircraft INFEASIBLE while
//! still labelling the search converged
//! (`opus-optimizer-independent-verification-handoff.md` §2.3).
//!
//! These tests hold the contract that closes that gap: a design the
//! application rejects is never returned as a converged result, a verified
//! fallback is never presented as the finalist's own certificate, and the
//! reason is always carried rather than dropped.

// A test asserts on values it constructed itself, so a failed unwrap there is
// the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::design_variables::DesignVector;
use alas_opt::{
    DeliveredAcceptance, OptimizationResult, SearchDiagnostics, REPORTING_FIDELITY_FALLBACK,
    REPORTING_FIDELITY_REJECTED,
};

fn converged_result() -> OptimizationResult {
    let mut history = alas_opt::history::OptimizationHistory::new();
    history.record_mission_sized(
        DesignVector::default(),
        true,
        1.0,
        17.0,
        60.0,
        2.5,
        380.0,
        0.0,
        "",
        37_000.0,
        230_000.0,
        37_000.0,
        0.0,
        0.0,
    );
    OptimizationResult {
        best_design: DesignVector::default(),
        best_cost: 1.0,
        best_valid: true,
        history,
        wall_time_s: 48.0,
        method: "mads".to_owned(),
        strategy: "progressive_barrier".to_owned(),
        termination: "converged".to_owned(),
        pareto_front: Vec::new(),
        search_diagnostics: Some(SearchDiagnostics {
            converged: true,
            analysis_evaluations: 225,
            cache_hits: 0,
            poll_iterations: 12,
            screening_evaluations: 32,
            screening_feasible: 4,
            verification_evaluations: 5,
            scan_wall_time_s: 1.5,
            search_wall_time_s: 46.5,
            workers: 8,
            poll_block_size: 16,
            first_feasible_cost: Some(2.0),
            relative_improvement: Some(0.5),
        }),
        delivered_acceptance: None,
    }
}

fn acceptance(verified: bool, delivered_is_search_finalist: bool) -> DeliveredAcceptance {
    DeliveredAcceptance {
        verified,
        finalist_rejected_by: if delivered_is_search_finalist && verified {
            Vec::new()
        } else {
            vec!["reported_cruise_attitude_outside_window".to_owned()]
        },
        delivered_rejected_by: if verified {
            Vec::new()
        } else {
            vec!["reported_cruise_attitude_outside_window".to_owned()]
        },
        rejection_messages: if verified && delivered_is_search_finalist {
            Vec::new()
        } else {
            vec!["the reported cruise body attitude is outside the design window".to_owned()]
        },
        candidates_evaluated: 1,
        delivered_is_search_finalist,
        wall_time_s: 1.2,
    }
}

#[test]
fn an_accepted_finalist_keeps_the_convergence_the_search_earned() {
    let mut result = converged_result();
    result.record_delivered_acceptance(acceptance(true, true));

    assert_eq!(result.termination, "converged");
    assert!(result.search_diagnostics.as_ref().unwrap().converged);
    assert!(result.delivered_acceptance.as_ref().unwrap().verified);
}

#[test]
fn a_design_the_application_rejects_is_never_reported_as_converged() {
    let mut result = converged_result();
    result.record_delivered_acceptance(acceptance(false, true));

    assert_eq!(result.termination, REPORTING_FIDELITY_REJECTED);
    assert!(!result.search_diagnostics.as_ref().unwrap().converged);
    // The reason survives; it is not replaced by a bare Boolean.
    assert_eq!(
        result
            .delivered_acceptance
            .as_ref()
            .unwrap()
            .delivered_rejected_by,
        vec!["reported_cruise_attitude_outside_window".to_owned()]
    );
}

#[test]
fn a_verified_fallback_is_usable_but_carries_no_convergence_certificate() {
    let mut result = converged_result();
    result.record_delivered_acceptance(acceptance(true, false));

    assert_eq!(result.termination, REPORTING_FIDELITY_FALLBACK);
    assert!(
        !result.search_diagnostics.as_ref().unwrap().converged,
        "the mesh-local certificate belongs to the finalist, not to a replacement"
    );
    assert!(result.delivered_acceptance.as_ref().unwrap().verified);
    assert!(!result
        .delivered_acceptance
        .as_ref()
        .unwrap()
        .finalist_rejected_by
        .is_empty());
}

#[test]
fn a_search_that_had_not_converged_is_not_promoted_by_being_accepted() {
    let mut result = converged_result();
    result.termination = "mesh_limit".to_owned();
    if let Some(diagnostics) = result.search_diagnostics.as_mut() {
        diagnostics.converged = false;
    }
    result.record_delivered_acceptance(acceptance(true, true));

    assert_eq!(result.termination, "mesh_limit");
    assert!(!result.search_diagnostics.as_ref().unwrap().converged);
}

#[test]
fn only_hard_feasible_candidates_are_offered_for_re_verification() {
    let mut result = converged_result();
    let mut feasible = DesignVector::default();
    feasible.span_m += 1.0;
    let mut violating = DesignVector::default();
    violating.span_m += 2.0;

    // A cheaper candidate that violates a hard residual must never be
    // offered: a fallback may only ever be a design the search itself
    // considered admissible.
    result.history.record_mission_sized(
        violating,
        true,
        0.1,
        17.0,
        62.0,
        2.5,
        380.0,
        0.0,
        "static_margin_floor",
        37_000.0,
        230_000.0,
        37_000.0,
        0.4,
        0.0,
    );
    result.history.record_mission_sized(
        feasible, true, 0.5, 17.0, 61.0, 2.5, 380.0, 0.0, "", 37_000.0, 230_000.0, 37_000.0, 0.0,
        0.0,
    );

    let candidates = result.ranked_hard_feasible_candidates(8);
    assert_eq!(
        candidates[0], result.best_design,
        "the finalist comes first"
    );
    assert!(candidates.contains(&feasible));
    assert!(!candidates.contains(&violating));
}

#[test]
fn the_candidate_list_is_bounded_and_free_of_duplicates() {
    let mut result = converged_result();
    for index in 0..40 {
        let mut design = DesignVector::default();
        design.span_m += f64::from(index) * 0.1;
        result.history.record_mission_sized(
            design,
            true,
            1.0 + f64::from(index),
            17.0,
            60.0,
            2.5,
            380.0,
            0.0,
            "",
            37_000.0,
            230_000.0,
            37_000.0,
            0.0,
            0.0,
        );
    }
    let candidates = result.ranked_hard_feasible_candidates(8);
    assert_eq!(candidates.len(), 8);
    for (index, candidate) in candidates.iter().enumerate() {
        assert!(
            !candidates[index + 1..].contains(candidate),
            "candidate {index} is repeated"
        );
    }
}
