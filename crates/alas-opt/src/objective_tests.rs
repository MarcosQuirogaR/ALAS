// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Unit tests for the scalar aircraft-design objective.

use alas_config::{AlasConfig, DesignVector};

use super::{apply_candidate_payload_load_case, DesignObjective};

#[test]
fn a_cruise_lift_above_the_configured_limit_is_rejected_before_trim() {
    let mut config = AlasConfig::default();
    config.requirements.max_cruise_cl = 0.0;
    let failure_cost = config.optimizer.weights.failure_cost;
    let mut objective = DesignObjective::new(config);

    let actual = objective.evaluate(&DesignVector::default().to_array());

    assert_eq!(actual, failure_cost);
    assert_eq!(objective.history.valid, vec![false]);
    assert_eq!(
        objective.history.reject_reason,
        vec!["stall_guard".to_owned()]
    );
}

#[test]
fn a_wing_larger_than_the_declared_limit_keeps_an_optimizer_gradient() {
    let mut relaxed = AlasConfig::default();
    relaxed.requirements.max_wing_area_m2 = 10_000.0;
    let mut constrained = relaxed.clone();
    constrained.requirements.max_wing_area_m2 = 1.0;
    let mut relaxed_objective = DesignObjective::new(relaxed);
    let mut constrained_objective = DesignObjective::new(constrained);

    let relaxed_cost = relaxed_objective.evaluate(&DesignVector::default().to_array());
    let constrained_cost = constrained_objective.evaluate(&DesignVector::default().to_array());

    assert!(relaxed_cost.is_finite());
    assert!(constrained_cost.is_finite());
    assert!(constrained_cost > relaxed_cost);
    assert!(constrained_objective.history.reject_reason[0].contains("wing_area_limit"));
    assert!(!constrained_objective.history.valid[0]);
}

#[test]
fn the_configured_body_angle_window_is_a_feasibility_condition_with_a_gradient() {
    let mut config = AlasConfig::default();
    config.optimizer.weights.geometric_body_alpha_min_deg = 100.0;
    config.optimizer.weights.geometric_body_alpha_max_deg = 101.0;
    let failure_cost = config.optimizer.weights.failure_cost;
    let mut objective = DesignObjective::new(config);

    let cost = objective.evaluate(&DesignVector::default().to_array());

    assert!(cost.is_finite());
    assert_ne!(cost, failure_cost);
    assert!(!objective.history.valid[0]);
    assert!(objective.history.reject_reason[0].contains("body_alpha_window"));
}

#[test]
fn the_product_transport_constraints_are_reached_at_the_default_area_requirement() {
    let config = AlasConfig::default();
    let failure_cost = config.optimizer.weights.failure_cost;
    let mut objective = DesignObjective::new(config);

    let cost = objective.evaluate(&DesignVector::default().to_array());
    let reason = objective
        .history
        .reject_reason
        .last()
        .map(String::as_str)
        .unwrap_or("");

    assert!(cost.is_finite());
    assert_ne!(cost, failure_cost);
    assert_ne!(reason, "transport_planform");
}

#[test]
fn subjective_shape_bounds_do_not_condition_the_default_product_search() {
    let mut relaxed = AlasConfig::default();
    relaxed.optimizer.weights.fuselage_floor_m = 0.0;
    let mut aggressive = relaxed.clone();
    aggressive.optimizer.weights.fuselage_floor_m = 1_000.0;

    let design = DesignVector::default().to_array();
    let relaxed_cost = DesignObjective::new(relaxed).evaluate(&design);
    let unconditioned_cost = DesignObjective::new(aggressive.clone()).evaluate(&design);

    assert_eq!(unconditioned_cost, relaxed_cost);

    aggressive.optimizer.weights.transport_shape_priors_enabled = true;
    let conditioned_cost = DesignObjective::new(aggressive).evaluate(&design);

    assert!(conditioned_cost > unconditioned_cost);
}

#[test]
fn reference_compatibility_restores_the_legacy_three_station_planform() {
    let config = AlasConfig::default();
    let product_objective = DesignObjective::new(config.clone());
    let reference_objective = DesignObjective::new_reference_compatibility(config);
    let design = DesignVector::default();

    assert!(product_objective
        .config
        .geometry
        .wing
        .side_of_body_span_fraction
        .is_some());
    assert!(product_objective
        .config
        .geometry
        .wing
        .kink_span_fraction
        .is_some());
    assert!(product_objective
        .config
        .geometry
        .wing
        .outboard_le_sweep_deg
        .is_none());
    let planform = reference_objective
        .config
        .geometry
        .wing
        .transport_planform(&design)
        .expect("the legacy compatibility planform is valid");

    assert!(reference_objective
        .config
        .geometry
        .wing
        .side_of_body_span_fraction
        .is_none());
    assert!(reference_objective
        .config
        .geometry
        .wing
        .side_of_body_chord_ratio
        .is_none());
    assert!(reference_objective
        .config
        .geometry
        .wing
        .kink_span_fraction
        .is_none());
    assert!(reference_objective
        .config
        .geometry
        .wing
        .outboard_le_sweep_deg
        .is_none());
    assert!(planform.side_of_body.is_none());
    assert_eq!(planform.stations().len(), 3);
    assert_eq!(planform.kink.span_fraction, 0.35);
    assert_eq!(
        planform.outboard_le_sweep_deg,
        design.sweep_deg
            - reference_objective
                .config
                .geometry
                .wing
                .outboard_sweep_decrement_deg
    );
}

#[test]
fn a_real_preset_keeps_its_passenger_target_during_candidate_preparation() {
    let mut config = AlasConfig::from_value(&serde_json::json!({"preset": "A220-300"}))
        .expect("the registered preset loads");
    let target = config.requirements.num_passengers;

    apply_candidate_payload_load_case(&mut config, &DesignVector::default())
        .expect("a fixed load case needs no geometry-derived capacity");

    assert_eq!(target, 130);
    assert_eq!(config.requirements.num_passengers, target);
    assert_eq!(config.requirements.cabin_preset, "Custom");
    assert!(!config.requirements.optimize_passenger_capacity);
}

#[test]
fn passenger_capacity_is_recomputed_only_after_explicit_opt_in() {
    let mut config = AlasConfig::from_value(&serde_json::json!({"preset": "A220-300"}))
        .expect("the registered preset loads");
    let design = alas_config::presets::get("A220-300")
        .expect("the registered preset has a design")
        .design_vector;
    let target = config.requirements.num_passengers;
    config.requirements.cabin_preset = "Ryanair".to_owned();
    config.requirements.optimize_passenger_capacity = true;

    apply_candidate_payload_load_case(&mut config, &design)
        .expect("the capacity load case resolves");

    assert_ne!(config.requirements.num_passengers, target);
}
