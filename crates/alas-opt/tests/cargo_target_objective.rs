// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The cargo capacity objective (clarified ledger App Features 2, D10).
//!
//! The entered cargo mass is a target the search is rewarded for matching, not
//! a floor it must clear and not an instruction to load without limit. What
//! these tests hold is exactly that: the target contributes to the ranking, it
//! does so from whichever side it is missed, and it never rejects a candidate
//! by itself. The physical limits that do reject an overloaded aircraft are the
//! mass, balance and volume residuals, and they are untouched here.
//!
//! Note on what "carried" means: `carried_cargo_payload_kg` is what the hold
//! layout actually loaded (`alas_payload::cargo::engine`), so it equals the
//! request whenever the hold can take it and saturates at the hold's capacity
//! when it cannot. The deviation therefore only becomes non-zero where the
//! aircraft, not the request, is the binding quantity — which is the whole
//! point of keeping the requested target separate from achieved capacity.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{AlasConfig, ConstraintPolicy};
use alas_opt::{assess_product_candidate, CandidateAssessment, ConstraintResidual};

/// A freighter configuration built on the reference twin, with `target_kg` as
/// the requested cargo payload.
fn assess_freighter(target_kg: f64) -> CandidateAssessment {
    let config = AlasConfig::from_value(&serde_json::json!({
        "preset": "AVE",
        "requirements": {
            "aircraft_type": "cargo",
            "cargo_payload_kg": target_kg,
        },
    }))
    .unwrap_or_else(|error| panic!("freighter configuration: {error}"));
    let design = alas_config::presets::get("AVE")
        .expect("registered preset")
        .design_vector;
    assess_product_candidate(&config, &design)
        .unwrap_or_else(|reason| panic!("freighter at {target_kg} kg did not size: {reason}"))
}

fn residual<'a>(assessment: &'a CandidateAssessment, id: &str) -> &'a ConstraintResidual {
    assessment
        .residuals
        .iter()
        .find(|residual| residual.id == id)
        .unwrap_or_else(|| panic!("{id} is not evaluated"))
}

/// What the hold actually takes when the request is far beyond it, kg.
fn saturated_capacity_kg() -> f64 {
    let carried = assess_freighter(1.0e6).sized.carried_cargo_payload_kg;
    assert!(
        carried.is_finite() && carried > 0.0,
        "the freighter hold takes {carried} kg"
    );
    carried
}

#[test]
fn the_cargo_target_is_reported_as_a_two_sided_soft_pair() {
    let target_kg = 40_000.0;
    let assessment = assess_freighter(target_kg);
    let shortfall = residual(&assessment, "cargo_target_shortfall");
    let excess = residual(&assessment, "cargo_target_excess");

    // D10: a target, not a hard minimum. Under the default hard geometry
    // policy it is still demoted to a ranking term.
    assert_eq!(shortfall.policy, ConstraintPolicy::Soft);
    assert_eq!(excess.policy, ConstraintPolicy::Soft);
    assert_eq!(shortfall.unit, "kg");
    assert_eq!(shortfall.limit, target_kg);
    assert_eq!(excess.limit, target_kg);

    // Both report the achieved payload against the request, and at most one
    // of them can be on the violating side.
    assert_eq!(shortfall.actual, excess.actual);
    assert_eq!(
        shortfall.actual, assessment.sized.carried_cargo_payload_kg,
        "the residual must report the carried payload, not the request"
    );
    assert!((shortfall.raw_residual + excess.raw_residual).abs() < 1.0e-9);
    assert!(shortfall.normalized_violation == 0.0 || excess.normalized_violation == 0.0);

    // Neither identifier may reject a candidate, and the retired one-sided
    // hard residual must be gone.
    let hard: Vec<&str> = assessment.violated_hard_ids();
    assert!(!hard.contains(&"cargo_target_shortfall"));
    assert!(!hard.contains(&"cargo_target_excess"));
    assert!(!assessment
        .residuals
        .iter()
        .any(|residual| residual.id == "cargo_shortfall"));
}

#[test]
fn a_request_the_hold_cannot_take_is_reported_as_a_shortfall_not_a_rejection() {
    // The case the pair exists for: the aircraft, not the request, is the
    // binding quantity. Before this change the same case was a *hard*
    // `cargo_shortfall` and rejected the candidate outright, which D10
    // explicitly rules out ("neither a new hard minimum").
    let capacity_kg = saturated_capacity_kg();
    let target_kg = 1.5 * capacity_kg;
    let assessment = assess_freighter(target_kg);
    let shortfall = residual(&assessment, "cargo_target_shortfall");

    assert!(
        (shortfall.actual - capacity_kg).abs() < 1.0,
        "carried {} against hold capacity {capacity_kg}",
        shortfall.actual
    );
    assert!(
        shortfall.raw_residual > 0.0,
        "raw shortfall {}",
        shortfall.raw_residual
    );
    assert!(shortfall.normalized_violation > 0.0);
    assert!(residual(&assessment, "cargo_target_excess").normalized_violation == 0.0);
    assert!(!assessment
        .violated_hard_ids()
        .contains(&"cargo_target_shortfall"));
}

#[test]
fn a_candidate_closer_to_the_cargo_target_scores_better() {
    // Two requests the same aircraft cannot meet, so the payload it flies
    // (and therefore its mass, its fuel and its mission objective) is
    // identical in both: only the distance to the target moves. The one
    // closer to what the aeroplane can actually carry must rank better.
    let capacity_kg = saturated_capacity_kg();
    let nearer = assess_freighter(1.2 * capacity_kg);
    let farther = assess_freighter(1.6 * capacity_kg);

    assert!(
        (nearer.sized.carried_cargo_payload_kg - farther.sized.carried_cargo_payload_kg).abs()
            < 1.0,
        "the two runs must fly the same payload"
    );
    assert!(
        (nearer.objective_value - farther.objective_value).abs()
            < 1.0e-6 * nearer.objective_value.abs().max(1.0),
        "the mission objective must be the one that did not move: {} against {}",
        nearer.objective_value,
        farther.objective_value
    );
    assert!(
        nearer.cost < farther.cost,
        "20 % over capacity cost {} against 60 % over capacity {}",
        nearer.cost,
        farther.cost
    );
}

#[test]
fn a_passenger_configuration_has_no_cargo_target_residual() {
    // The passenger path must be unchanged: no new identifiers, no new cost
    // contribution.
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": "AVE" }))
        .unwrap_or_else(|error| panic!("{error}"));
    let design = alas_config::presets::get("AVE")
        .expect("registered preset")
        .design_vector;
    let assessment = assess_product_candidate(&config, &design).expect("the reference twin sizes");
    for id in [
        "cargo_target_shortfall",
        "cargo_target_excess",
        "cargo_shortfall",
    ] {
        assert!(
            !assessment
                .residuals
                .iter()
                .any(|residual| residual.id == id),
            "{id} appeared on a passenger aircraft"
        );
    }
}

/// A freighter that asks the hold for `request_kg` (the capacity a cabin
/// preset owns and the load case uses) while the user's entered objective is
/// `objective_kg`. The two are deliberately different quantities: the first
/// loads the aeroplane, the second only scores it.
fn assess_freighter_with_objective(request_kg: f64, objective_kg: f64) -> CandidateAssessment {
    let config = AlasConfig::from_value(&serde_json::json!({
        "preset": "AVE",
        "requirements": {
            "aircraft_type": "cargo",
            "cargo_payload_kg": request_kg,
            "cargo_objective_kg": objective_kg,
        },
    }))
    .unwrap_or_else(|error| panic!("freighter configuration: {error}"));
    assert_eq!(config.requirements.cargo_target_kg(), objective_kg);
    let design = alas_config::presets::get("AVE")
        .expect("registered preset")
        .design_vector;
    assess_product_candidate(&config, &design).unwrap_or_else(|reason| {
        panic!("freighter at {request_kg} kg with a {objective_kg} kg objective did not size: {reason}")
    })
}

#[test]
fn the_entered_objective_is_the_target_the_residual_pair_is_scored_against() {
    // The capacity field is what a cabin preset computes and overwrites, so
    // the entered objective is a separate field, and it is the one the pair
    // must use as its limit.
    let capacity_kg = saturated_capacity_kg();
    let objective_kg = 0.5 * capacity_kg;
    let assessment = assess_freighter_with_objective(1.0e6, objective_kg);

    let shortfall = residual(&assessment, "cargo_target_shortfall");
    let excess = residual(&assessment, "cargo_target_excess");
    assert_eq!(shortfall.limit, objective_kg);
    assert_eq!(excess.limit, objective_kg);
    assert_eq!(shortfall.actual, assessment.sized.carried_cargo_payload_kg);

    // Asked for everything, the hold took its capacity, which is twice what
    // was requested as an objective: that is the excess side, and D10 makes
    // it a ranking term rather than a rejection.
    assert!((shortfall.actual - capacity_kg).abs() < 1.0);
    assert!(excess.raw_residual > 0.0);
    assert!(excess.normalized_violation > 0.0);
    assert_eq!(shortfall.normalized_violation, 0.0);
    assert!(!assessment
        .violated_hard_ids()
        .contains(&"cargo_target_excess"));
}

#[test]
fn a_candidate_closer_to_the_entered_objective_scores_better() {
    // One aeroplane, one load case, one flown payload: only the entered
    // objective moves, so every other cost contribution is equal by
    // construction and the ranking must follow the distance to the target.
    let capacity_kg = saturated_capacity_kg();
    let nearer = assess_freighter_with_objective(1.0e6, 1.1 * capacity_kg);
    let farther = assess_freighter_with_objective(1.0e6, 1.6 * capacity_kg);

    assert!(
        (nearer.sized.carried_cargo_payload_kg - farther.sized.carried_cargo_payload_kg).abs()
            < 1.0,
        "the two runs must fly the same payload"
    );
    assert!(
        (nearer.objective_value - farther.objective_value).abs()
            < 1.0e-9 * nearer.objective_value.abs().max(1.0),
        "the mission objective must be the one that did not move: {} against {}",
        nearer.objective_value,
        farther.objective_value
    );
    assert!(
        nearer.cost < farther.cost,
        "10 % short of the objective cost {} against 60 % short {}",
        nearer.cost,
        farther.cost
    );

    // And symmetrically on the other side of the target: carrying more than
    // was asked for is ranked worse the further past it the aeroplane goes,
    // which is what makes this a target rather than a payload maximizer.
    let over_a_little = assess_freighter_with_objective(1.0e6, 0.9 * capacity_kg);
    let over_a_lot = assess_freighter_with_objective(1.0e6, 0.5 * capacity_kg);
    assert!(
        over_a_little.cost < over_a_lot.cost,
        "11 % over the objective cost {} against 100 % over {}",
        over_a_little.cost,
        over_a_lot.cost
    );
}

#[test]
fn entering_an_objective_changes_no_other_residual_and_no_limit_outcome() {
    // The target is a ranking term. Every physical limit, its normalization
    // and its verdict have to come out of the two runs identical, because
    // the aeroplane and its load case are identical.
    let capacity_kg = saturated_capacity_kg();
    let without = assess_freighter(1.0e6);
    let with = assess_freighter_with_objective(1.0e6, 0.5 * capacity_kg);

    assert_eq!(without.hard_feasible, with.hard_feasible);
    assert_eq!(without.violated_hard_ids(), with.violated_hard_ids());
    assert_eq!(without.hard_violation_sum, with.hard_violation_sum);
    assert_eq!(
        without.sized.takeoff_mass_kg, with.sized.takeoff_mass_kg,
        "the objective must not resize the aeroplane"
    );
    assert_eq!(
        without.sized.carried_cargo_payload_kg, with.sized.carried_cargo_payload_kg,
        "the objective must not load the hold"
    );
    assert_eq!(without.objective_value, with.objective_value);

    let ids: Vec<&str> = without.residuals.iter().map(|r| r.id).collect();
    assert_eq!(
        ids,
        with.residuals.iter().map(|r| r.id).collect::<Vec<&str>>(),
        "no residual appears or disappears"
    );
    for (left, right) in without.residuals.iter().zip(with.residuals.iter()) {
        if left.id.starts_with("cargo_target_") {
            continue;
        }
        assert_eq!(left, right, "{} moved with the cargo objective", left.id);
    }
}

#[test]
fn no_registered_preset_can_be_reached_by_the_cargo_objective() {
    // The preset-invariance argument, held as a test: every registered
    // aircraft is a passenger aircraft and none of them enters a cargo
    // objective, so the target resolves to the capacity every one of them
    // already carried and the residual pair is not even built. A registered
    // preset's default run therefore cannot move.
    for name in alas_config::presets::available() {
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(
            config.requirements.aircraft_type, "passenger",
            "{name} is a freighter; its default run would reach the cargo target"
        );
        assert_eq!(config.requirements.cargo_objective_kg, 0.0, "{name}");
        assert_eq!(
            config.requirements.cargo_target_kg(),
            config.requirements.cargo_payload_kg,
            "{name}: the resolved target must be the capacity it always was"
        );
    }
}

#[test]
fn two_registered_passenger_presets_carry_no_cargo_residual_and_no_cargo_term() {
    // The named pins for the all-preset matrix: A320-200 and B787-9 size,
    // rank and report exactly as before, with no cargo identifier in their
    // residual tables and nothing added to their soft-violation sum.
    for name in ["A320-200", "B787-9"] {
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let design = alas_config::presets::get(name)
            .expect("registered preset")
            .design_vector;
        let assessment = assess_product_candidate(&config, &design)
            .unwrap_or_else(|reason| panic!("{name} did not size: {reason}"));
        for residual in &assessment.residuals {
            assert!(
                !residual.id.starts_with("cargo_"),
                "{name} grew a cargo residual: {}",
                residual.id
            );
        }
        let soft: f64 = assessment
            .residuals
            .iter()
            .filter(|residual| residual.policy == ConstraintPolicy::Soft)
            .map(|residual| residual.normalized_violation)
            .sum();
        assert!(
            (assessment.soft_violation_sum - soft).abs() < 1.0e-12,
            "{name}: the soft sum must still be exactly its soft residuals"
        );
    }
}
