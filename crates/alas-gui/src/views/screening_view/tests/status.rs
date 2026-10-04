// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Status-text tests for the screening result stages.

use super::super::results::screening_unavailable_reason;
use crate::state::AppState;
use alas_screen::types::{AirfoilCandidateResult, AirfoilScreeningResult};

fn result_with_candidate(candidate: AirfoilCandidateResult) -> AirfoilScreeningResult {
    AirfoilScreeningResult {
        n_total: 1,
        n_ok: usize::from(candidate.status == "ok"),
        n_error: usize::from(candidate.status != "ok"),
        n_refined: usize::from(candidate.refined),
        n_mses_verified: usize::from(candidate.mses_verified),
        candidates: vec![candidate],
        ..Default::default()
    }
}

#[test]
fn screening_without_a_completed_sweep_explains_how_to_produce_figures() {
    assert_eq!(
        screening_unavailable_reason(&AppState::default(), None, "trade_map"),
        "Not available: run the airfoil screening sweep to produce this figure."
    );
}

#[test]
fn completed_result_without_stage_1_data_does_not_ask_for_another_sweep() {
    let result = AirfoilScreeningResult {
        n_total: 1,
        n_error: 1,
        ..Default::default()
    };
    let message = screening_unavailable_reason(&AppState::default(), Some(&result), "trade_map");
    assert_eq!(
        message,
        "Not available: screening completed, but no candidate produced usable 2-D data."
    );
}

#[test]
fn completed_stage_2_failure_names_the_missing_stage_data() {
    let result = result_with_candidate(AirfoilCandidateResult {
        name: "failed-refinement".to_owned(),
        status: "ok".to_owned(),
        refine_error: Some("trim failed".to_owned()),
        ..Default::default()
    });
    assert_eq!(
        screening_unavailable_reason(&AppState::default(), Some(&result), "rerank_2d_3d"),
        "Not available: Stage 2 (3-D wing) completed, but no candidate produced usable data."
    );
}

#[test]
fn completed_mses_failure_names_the_missing_stage_3_data() {
    let result = result_with_candidate(AirfoilCandidateResult {
        name: "failed-mses".to_owned(),
        status: "ok".to_owned(),
        refined: true,
        mses_status: Some("error".to_owned()),
        mses_error: Some("no convergence".to_owned()),
        ..Default::default()
    });
    assert_eq!(
        screening_unavailable_reason(&AppState::default(), Some(&result), "mses_verification"),
        "Not available: MSES verification completed, but no candidate produced usable Stage 3 data."
    );
}

#[test]
fn failed_and_running_states_keep_priority_over_completed_context() {
    let result = AirfoilScreeningResult::default();
    let mut state = AppState::default();
    state.screening.error = Some("worker failed".to_owned());
    assert_eq!(
        screening_unavailable_reason(&state, Some(&result), "trade_map"),
        "Not available: screening failed: worker failed"
    );

    state.screening.error = None;
    state.screening.running = true;
    assert_eq!(
        screening_unavailable_reason(&state, Some(&result), "trade_map"),
        "Not available: the screening sweep is still running."
    );
}
