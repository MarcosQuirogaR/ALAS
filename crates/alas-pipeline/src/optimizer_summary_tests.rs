// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use alas_config::DesignVector;
use alas_opt::{
    BaselineComparison, DeliveredAcceptance, ReportingBaseline, SearchDiagnostics, StageSummary,
};

use super::*;

fn stage(name: &str, budget: usize, reserved: usize, used: usize, limited: bool) -> StageSummary {
    StageSummary {
        stage: name.to_owned(),
        max_evaluations: budget,
        planned_evaluations: budget,
        reserved_evaluations: reserved,
        time_limit_s: 30.0,
        time_limited: limited,
        evaluations: used,
        restoration_evaluations: 0,
        cancelled_unstarted: 0,
        pre_gate_rejects: 0,
        analysis_evaluations: used - used / 20,
        generations: 3,
        feasible: used / 2,
        elite_size: 4,
        wall_time_s: 12.0,
        candidate_time_s: 1.0,
        lane_utilization: 0.5,
        termination: if limited {
            "time_budget"
        } else {
            "evaluation_budget"
        }
        .to_owned(),
        sizing_work: None,
    }
}

/// A finished run whose winner moved the wing: the history records the
/// re-sized tail, so no row carries `best_design` itself.
fn moved_wing_result(limited: bool) -> OptimizationResult {
    let baseline = DesignVector::default();
    let winner = DesignVector {
        span_m: baseline.span_m * 0.97,
        ..baseline
    };
    let mut history = alas_opt::OptimizationHistory::new();
    for (design, fuel) in [(baseline, 9_000.0), (winner, 8_600.0)] {
        let recorded = DesignVector {
            tail_scale: design.tail_scale * 1.05,
            ..design
        };
        history.record_mission_sized(
            recorded, true, fuel, 17.0, 34.0, 2.0, 120.0, 0.0, "", fuel, 70_000.0, fuel, 0.0, 0.0,
        );
    }
    let comparison = BaselineComparison {
        baseline_feasible: true,
        baseline_objective_value: 9_000.0,
        winner_objective_value: 8_600.0,
        relative_change: Some(-400.0 / 9_000.0),
        baseline_block_fuel_kg: Some(9_000.0),
        winner_block_fuel_kg: Some(8_600.0),
    };
    let diagnostics = SearchDiagnostics {
        restoration: None,
        converged: false,
        analysis_evaluations: 2,
        cache_hits: 0,
        poll_iterations: 3,
        screening_evaluations: 64,
        screening_feasible: 30,
        verification_evaluations: 4,
        scan_wall_time_s: 12.0,
        search_wall_time_s: 12.0,
        workers: 8,
        poll_block_size: 24,
        first_feasible_cost: None,
        relative_improvement: None,
        feasible_fraction: 1.0,
        epsilon_level: 0.0,
        stages: vec![
            stage("screening", 2_000, 0, 64, limited),
            StageSummary {
                max_evaluations: 20_000,
                ..stage("refinement", 600, 10, 590, limited)
            },
        ],
        rejections: Vec::new(),
        seed: Some(1_234_567),
        scope: alas_opt::SEARCH_SCOPE.to_owned(),
        baseline: Some(comparison),
        winner_history_row: Some(1),
        baseline_clamped: false,
    };
    let mut result = OptimizationResult {
        best_design: winner,
        best_cost: 8_600.0,
        best_valid: true,
        history,
        wall_time_s: 30.0,
        method: "differential_evolution".to_owned(),
        strategy: String::new(),
        termination: diagnostics.stages[1].termination.clone(),
        pareto_front: Vec::new(),
        search_diagnostics: Some(diagnostics),
        delivered_acceptance: None,
    };
    result.record_delivered_acceptance(DeliveredAcceptance {
        verified: true,
        finalist_rejected_by: Vec::new(),
        delivered_rejected_by: Vec::new(),
        rejection_messages: Vec::new(),
        candidates_evaluated: 1,
        delivered_is_search_finalist: true,
        wall_time_s: 2.0,
        analyses: 2,
        baseline: Some(ReportingBaseline::new(
            true,
            Vec::new(),
            Some(5_000.0),
            Some(4_900.0),
        )),
    });
    result
}

fn value<'a>(lines: &'a [(&'static str, String)], label: &str) -> Option<&'a str> {
    lines
        .iter()
        .find(|(line, _)| *line == label)
        .map(|(_, value)| value.as_str())
}

#[test]
fn a_moved_wing_winner_shows_its_same_model_baseline_and_delta() {
    let result = moved_wing_result(true);
    assert!(!result.history.design_vectors.contains(&result.best_design));
    let summary = OptimizerRunSummary::from_result(&result, &AlasConfig::default(), None);
    let delta = summary
        .baseline
        .expect("the baseline comes from the diagnostics");
    assert!(delta.relative_objective_change().is_some_and(|c| c < 0.0));
    assert!(delta.winner_block_fuel_kg.is_some());
    let lines = summary.label_lines();
    assert!(value(&lines, "Same-model baseline").is_none());
    assert!(value(&lines, "Block fuel, result").is_some());
    let trip = value(&lines, "Trip fuel at reporting fidelity, result").expect("trip fuel line");
    assert!(trip.contains('%'), "{trip}");
}

#[test]
fn every_stage_states_its_own_budget_and_never_appears_over_it() {
    for limited in [true, false] {
        let summary = OptimizerRunSummary::from_result(
            &moved_wing_result(limited),
            &AlasConfig::default(),
            None,
        );
        let lines = summary.label_lines();
        for label in [
            "Screening, evaluations used / budget",
            "Refinement, evaluations used / budget",
        ] {
            let text = value(&lines, label).expect("stage line");
            let (used, budget) = text.split_once(" / ").expect("used / budget");
            let (used, budget): (usize, usize) =
                (used.parse().expect("count"), budget.parse().expect("count"));
            assert!(used <= budget, "{label}: {text}");
        }
        let rule = value(&lines, "Stopping rule").expect("rule");
        assert_eq!(
            rule,
            if limited {
                TIME_LIMITED_TEXT
            } else {
                EVALUATIONS_ONLY_TEXT
            }
        );
        // The replay count (pre-gate-passed, repeats included) is shown
        // apart from the coupled analyses, and the refinement's planned
        // budget, the one a replay sets, apart from its ceiling.
        assert_eq!(value(&lines, REPLAY_COUNT_LABEL), Some("64 / 590"));
        assert_eq!(
            value(&lines, "Coupled analyses, screening / refinement"),
            Some("61 / 561")
        );
        assert_eq!(value(&lines, PLANNED_BUDGET_LABEL), Some("600 / 20000"));
        assert_eq!(
            value(&lines, "Refinement, evaluations used / budget"),
            Some("590 / 590")
        );
        // The rejections the replay reproduces beside the analysed counts.
        assert_eq!(
            value(&lines, "Pre-gate rejections, screening / refinement"),
            Some("0 / 0")
        );
        let verification = summary.verification.expect("verification ran");
        assert!(verification.analyses <= verification.reserved);
    }
}

#[test]
fn the_seed_shown_is_the_one_the_run_used_even_unseeded() {
    let config = AlasConfig::default();
    assert_eq!(config.optimizer.solver.seed, None);
    let summary = OptimizerRunSummary::from_result(&moved_wing_result(true), &config, None);
    assert_eq!(summary.seed, Some(1_234_567));
}

#[test]
fn an_infeasible_run_is_not_completed_and_an_old_result_says_its_baseline_is_unavailable() {
    let mut result = moved_wing_result(true);
    result.best_valid = false;
    result.search_diagnostics = None;
    let summary = OptimizerRunSummary::from_result(&result, &AlasConfig::default(), Some(9));
    assert!(!summary.delivered_feasible);
    assert_ne!(summary.status, SolverOptimizationStatus::Completed);
    assert_eq!(summary.seed, Some(9));
    let lines = summary.label_lines();
    assert_eq!(value(&lines, "Same-model baseline"), Some("unavailable"));
    assert!(value(&lines, "Stopping rule").is_none());
}

/// `moved_wing_result` after the finalist was rejected and a lower-ranked
/// candidate, recorded in the history with its own objective and fuel, was
/// delivered instead. Returns the result and the delivered design as the
/// pipeline rebinds it (derived tail scale changed).
fn fallback_delivered_result() -> (OptimizationResult, DesignVector) {
    let mut result = moved_wing_result(false);
    let fallback = DesignVector {
        span_m: DesignVector::default().span_m * 0.99,
        ..DesignVector::default()
    };
    let recorded = DesignVector {
        tail_scale: fallback.tail_scale * 1.05,
        ..fallback
    };
    result.history.record_mission_sized(
        recorded, true, 8_800.0, 17.0, 34.0, 2.0, 120.0, 0.0, "", 8_800.0, 70_000.0, 8_750.0, 0.0,
        0.0,
    );
    let mut acceptance = result.delivered_acceptance.clone().expect("recorded");
    acceptance.delivered_is_search_finalist = false;
    acceptance.finalist_rejected_by = vec!["insufficient_static_margin".to_owned()];
    result.record_delivered_acceptance(acceptance);
    let delivered = DesignVector {
        tail_scale: fallback.tail_scale * 1.06,
        ..fallback
    };
    (result, delivered)
}

#[test]
fn a_fallback_delivery_shows_the_delivered_candidates_own_values() {
    let (result, delivered) = fallback_delivered_result();
    let summary = OptimizerRunSummary::from_delivered_result(
        &result,
        &AlasConfig::default(),
        None,
        Some(&delivered),
    );
    let delta = summary
        .baseline
        .expect("the delivered row is in the history");
    assert_eq!(delta.winner_objective, 8_800.0);
    assert_eq!(delta.winner_block_fuel_kg, Some(8_750.0));
    assert_eq!(delta.baseline_objective, 9_000.0);
    let change = delta.relative_objective_change().expect("finite change");
    assert!((change - (8_800.0 - 9_000.0) / 9_000.0).abs() < 1e-12);
    let lines = summary.label_lines();
    assert_eq!(value(&lines, "Block fuel, result"), Some("8750 kg"));
    assert!(value(&lines, "Objective, result")
        .expect("objective line")
        .starts_with("8800.0000"));
}

#[test]
fn a_fallback_delivery_never_shows_the_rejected_finalists_values() {
    let (result, _) = fallback_delivered_result();
    // The delivered design is unknown or absent from the history: the
    // comparison is withheld, not shown for the rejected finalist.
    let unknown = DesignVector {
        span_m: 1.0,
        ..DesignVector::default()
    };
    for delivered in [None, Some(&unknown)] {
        let summary = OptimizerRunSummary::from_delivered_result(
            &result,
            &AlasConfig::default(),
            None,
            delivered,
        );
        assert!(summary.baseline.is_none());
        let lines = summary.label_lines();
        assert_eq!(value(&lines, "Same-model baseline"), Some("unavailable"));
        assert!(value(&lines, "Objective, result").is_none());
    }
}

#[test]
fn an_accepted_finalist_keeps_the_search_comparison() {
    let result = moved_wing_result(false);
    let summary = OptimizerRunSummary::from_delivered_result(
        &result,
        &AlasConfig::default(),
        None,
        Some(&result.best_design),
    );
    assert_eq!(
        summary.baseline.expect("baseline").winner_objective,
        8_600.0
    );
}

fn geometry(
    (baseline_aspect_ratio, winner_aspect_ratio): (f64, f64),
    (baseline_sweep_deg, winner_sweep_deg): (f64, f64),
) -> GeometryComparison {
    GeometryComparison {
        baseline_aspect_ratio,
        winner_aspect_ratio,
        baseline_sweep_deg,
        winner_sweep_deg,
    }
}

#[test]
fn an_aspect_ratio_gain_over_ten_percent_is_flagged_and_a_smaller_one_is_not() {
    assert!(geometry((10.0, 11.5), (27.0, 27.0)).aspect_ratio_above_reference());
    assert!(!geometry((10.0, 10.5), (27.0, 27.0)).aspect_ratio_above_reference());
    assert!(!geometry((10.0, 9.0), (27.0, 27.0)).aspect_ratio_above_reference());
    assert!(!geometry((f64::NAN, 11.5), (27.0, 27.0)).aspect_ratio_above_reference());
}

#[test]
fn a_sweep_more_than_three_degrees_below_the_reference_is_flagged() {
    assert!(geometry((10.0, 10.0), (27.0, 22.1)).sweep_below_reference());
    assert!(!geometry((10.0, 10.0), (27.0, 24.5)).sweep_below_reference());
    assert!(!geometry((10.0, 10.0), (27.0, 30.0)).sweep_below_reference());
}

#[test]
fn the_plausibility_lines_show_both_values_and_are_absent_when_neither_fires() {
    let mut summary =
        OptimizerRunSummary::from_result(&moved_wing_result(false), &AlasConfig::default(), None);
    summary.geometry = Some(geometry((10.0, 10.5), (27.0, 26.0)));
    let lines = summary.label_lines();
    assert!(value(&lines, ASPECT_RATIO_FLAG_LABEL).is_none());
    assert!(value(&lines, SWEEP_FLAG_LABEL).is_none());
    summary.geometry = Some(geometry((10.0, 11.5), (27.0, 22.1)));
    let lines = summary.label_lines();
    assert_eq!(
        value(&lines, ASPECT_RATIO_FLAG_LABEL),
        Some("11.50 / 10.00")
    );
    assert_eq!(value(&lines, SWEEP_FLAG_LABEL), Some("22.1 deg / 27.0 deg"));
}

#[test]
fn a_constrained_start_is_labelled_as_such_and_a_preset_is_not() {
    let mut summary =
        OptimizerRunSummary::from_result(&moved_wing_result(false), &AlasConfig::default(), None);
    let preset = summary.label_lines();
    for label in [
        "Objective, preset",
        "Block fuel, preset",
        "Trip fuel at reporting fidelity, preset",
    ] {
        assert!(value(&preset, label).is_some(), "{label}");
    }
    summary.baseline_constrained = true;
    let constrained = summary.label_lines();
    for (preset_label, label) in [
        ("Objective, preset", "Objective, constrained start"),
        ("Block fuel, preset", "Block fuel, constrained start"),
        (
            "Trip fuel at reporting fidelity, preset",
            "Trip fuel at reporting fidelity, constrained start",
        ),
    ] {
        assert!(
            value(&constrained, preset_label).is_none(),
            "{preset_label}"
        );
        assert!(value(&constrained, label).is_some(), "{label}");
    }
}
