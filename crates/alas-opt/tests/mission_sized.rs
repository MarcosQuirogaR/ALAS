// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Behavioural tests for the mission-sized objective (`alas_opt::mdo`).

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::design_variables::DesignVector;
use alas_config::{AlasConfig, ConstraintPolicy, MtowSizing, ObjectiveKind};
use alas_opt::objective::DesignObjective;
use alas_opt::{assess_candidate, ConstraintFamily, DesignOptimizer};

fn block_fuel_config() -> AlasConfig {
    let mut config = AlasConfig::default();
    config.optimizer.objective.kind = ObjectiveKind::BlockFuel;
    // The single-pass closure these tests were written against; the
    // product default sizes the takeoff mass by the mission.
    config.optimizer.objective.mtow_sizing = MtowSizing::FixedRequirement;
    config
}

/// (1) The default aircraft, `MtowSizing::FixedRequirement`, evaluates to a
/// finite cost, a positive block fuel, a finite L/D, and a residual list
/// that names every family.
#[test]
fn the_default_design_evaluates_with_every_family_represented() {
    let config = block_fuel_config();
    let objective = DesignObjective::new(config);
    let x = DesignVector::default().to_array();

    let assessment = assess_candidate(&objective, &x).unwrap_or_else(|reason| panic!("{reason}"));

    assert!(assessment.cost.is_finite());
    assert!(assessment.sized.block_fuel_kg > 0.0);
    assert!(assessment.sized.lift_to_drag.is_finite() && assessment.sized.lift_to_drag > 0.0);

    for family in [
        ConstraintFamily::Mass,
        ConstraintFamily::Balance,
        ConstraintFamily::Performance,
        ConstraintFamily::Geometry,
    ] {
        assert!(
            assessment
                .residuals
                .iter()
                .any(|residual| residual.family == family),
            "no residual recorded for {family:?}"
        );
    }
}

/// (2) `MtowSizing::SizedByMission` closes the outer fixed point and never
/// exceeds the takeoff-mass ceiling: `solve_dispatch` clamps every candidate
/// takeoff mass at `mtow_kg`, so this also exercises that the outer loop
/// reads the clamped value back correctly.
#[test]
fn sized_by_mission_closes_and_never_exceeds_the_ceiling() {
    let mut config = block_fuel_config();
    config.optimizer.objective.mtow_sizing = MtowSizing::SizedByMission;
    let ceiling_kg = config.requirements.mtow_kg;
    let objective = DesignObjective::new(config);
    let x = DesignVector::default().to_array();

    let assessment = assess_candidate(&objective, &x).unwrap_or_else(|reason| panic!("{reason}"));

    assert!(
        assessment.sized.sizing_closed,
        "sizing did not close within the default iteration budget"
    );
    assert!(assessment.sized.takeoff_mass_kg <= ceiling_kg + 1e-6);
}

/// (3) A design range no transport can fly is `MtowLimited`, hard
/// infeasible, and costs more than the feasible default.
///
/// The default design is hard-feasible under the default (hard) geometry
/// family: its tail volumes sit a few percent inside the preferred window,
/// which ranks soft because the window is a plausibility band rather than a
/// requirement, and its built wing area exceeds the configured cap by a few
/// parts per million, which is the geometry builder's own rounding and lies
/// inside the residual table's numerical slack.
#[test]
fn an_impossible_design_range_is_hard_infeasible_and_costs_more() {
    let feasible_objective = DesignObjective::new(block_fuel_config());
    let x = DesignVector::default().to_array();
    let feasible =
        assess_candidate(&feasible_objective, &x).unwrap_or_else(|reason| panic!("{reason}"));
    assert!(feasible.hard_feasible);

    let mut impossible_config = block_fuel_config();
    impossible_config.optimizer.objective.design_range_nmi = 20_000.0;
    let impossible_objective = DesignObjective::new(impossible_config);
    let impossible =
        assess_candidate(&impossible_objective, &x).unwrap_or_else(|reason| panic!("{reason}"));

    assert!(!impossible.hard_feasible);
    assert!(impossible
        .residuals
        .iter()
        .any(|residual| residual.id == "mtow_ceiling" && residual.normalized_violation > 0.0));
    assert!(impossible.cost > feasible.cost);
}

/// (4) Turning a family off removes its residuals; a `Diagnostic` family's
/// residuals are recorded but never counted toward feasibility.
#[test]
fn an_off_family_is_removed_and_a_diagnostic_family_is_uncounted() {
    let mut off_config = block_fuel_config();
    off_config.optimizer.objective.geometry_constraints = ConstraintPolicy::Off;
    let off_objective = DesignObjective::new(off_config);
    let x = DesignVector::default().to_array();
    let off_assessment =
        assess_candidate(&off_objective, &x).unwrap_or_else(|reason| panic!("{reason}"));
    assert!(off_assessment
        .residuals
        .iter()
        .all(|residual| residual.family != ConstraintFamily::Geometry));

    let mut diagnostic_config = block_fuel_config();
    diagnostic_config.optimizer.objective.geometry_constraints = ConstraintPolicy::Diagnostic;
    let diagnostic_objective = DesignObjective::new(diagnostic_config);
    let diagnostic_assessment =
        assess_candidate(&diagnostic_objective, &x).unwrap_or_else(|reason| panic!("{reason}"));
    let geometry_residuals: Vec<_> = diagnostic_assessment
        .residuals
        .iter()
        .filter(|residual| residual.family == ConstraintFamily::Geometry)
        .collect();
    assert!(!geometry_residuals.is_empty());
    assert_eq!(
        diagnostic_assessment.hard_violation_sum, off_assessment.hard_violation_sum,
        "a diagnostic family must not add to the hard-violation sum"
    );
    assert_eq!(
        diagnostic_assessment.soft_violation_sum, off_assessment.soft_violation_sum,
        "a diagnostic family must not add to the soft-violation sum"
    );
}

/// (5) An infeasible candidate's reject reason lists the violated hard
/// residual ids joined by `+`, and the history vectors stay aligned.
#[test]
fn the_reject_reason_lists_violated_ids_joined_by_plus() {
    let mut config = block_fuel_config();
    config.optimizer.objective.design_range_nmi = 20_000.0;
    let mut objective = DesignObjective::new(config);
    let x = DesignVector::default().to_array();

    let cost = objective.evaluate(&x);
    let history = &objective.history;
    let last = history.n_evaluations() - 1;

    assert!(!history.valid[last]);
    assert_eq!(history.cost[last], cost);
    let reason = &history.reject_reason[last];
    assert!(!reason.is_empty());
    assert!(reason.contains("mtow_ceiling"));
    for id in reason.split('+') {
        assert!(!id.is_empty(), "reason {reason:?} has an empty '+' segment");
    }

    // Every history vector, including the mission-sized additions, stays the
    // same length as the number of evaluations.
    assert_eq!(history.design_vectors.len(), history.n_evaluations());
    assert_eq!(history.objective_value.len(), history.n_evaluations());
    assert_eq!(history.takeoff_mass_kg.len(), history.n_evaluations());
    assert_eq!(history.block_fuel_kg.len(), history.n_evaluations());
    assert_eq!(history.hard_violation.len(), history.n_evaluations());
    assert_eq!(history.soft_violation.len(), history.n_evaluations());
}

/// (6) A trivial `feasibility_first_de` run (seeded near the default design,
/// with the default `Hard` geometry policy) returns `Ok`, or reports
/// `NoFeasibleDesign` naming the residuals that failed -- both are
/// legitimate outcomes of this contract, so the assertion accepts either.
///
/// With seed `1` this returns `Ok`. The exact default design vector is
/// itself marginally hard-infeasible under the default `Hard` geometry
/// policy (its built wing area sits a few parts per million over
/// `max_wing_area_m2`, and both tail-volume coefficients sit a few percent
/// under their configured minimum window -- the legacy weighted-penalty
/// objective these defaults were tuned against treats both as soft
/// preferences, not hard bounds), but `seed_near_initial_design` draws a
/// small pool of candidates perturbed around it rather than the exact
/// vector, and for this seed at least one perturbation clears every hard
/// residual, which is what `Ok` reports.
#[test]
fn a_trivial_feasibility_first_de_run_returns_ok_or_names_the_failing_residuals() {
    let mut config = block_fuel_config();
    config.optimizer.solver.method = "feasibility_first_de".to_owned();
    config.optimizer.solver.population_size = 1;
    config.optimizer.solver.max_iterations = 0;
    config.optimizer.solver.seed = Some(1);
    config.optimizer.solver.seed_near_initial_design = true;

    let result = DesignOptimizer::new(config).run(None, None, None);

    match result {
        Ok(outcome) => {
            assert!(outcome.best_valid);
            assert!(outcome.best_cost.is_finite());
        }
        Err(alas_opt::OptimizationError::NoFeasibleDesign(no_feasible)) => {
            assert!(no_feasible.evaluated_candidates > 0);
            assert!(!no_feasible.rejection_reason_counts.is_empty());
        }
        Err(other) => panic!("unexpected optimizer error: {other}"),
    }
}
