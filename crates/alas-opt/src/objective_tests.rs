// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Unit tests for the scalar aircraft-design objective.

use alas_config::{AlasConfig, ConstraintPolicy, DesignVector};
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::spacing::linspace;
use alas_geom::aircraft::wing::{Wing, WingXSec};

use super::{
    apply_candidate_payload_load_case, wing_fuel_volume_m3,
    wing_fuel_volume_m3_reference_compatibility, DesignObjective,
};

fn dihedral_wing() -> Wing {
    let airfoil = Airfoil::from_name("naca0012").expect("valid four-digit NACA section");
    Wing::new(
        "Reference test wing",
        vec![
            WingXSec::new([0.0, 0.0, 0.0], 2.0, 0.0, airfoil.clone()),
            WingXSec::new([0.0, 10.0, 5.0], 2.0, 0.0, airfoil),
        ],
        true,
    )
}

#[test]
fn fuel_volume_uses_projected_reference_area_and_span() {
    let wing = dihedral_wing();
    let usable_fraction = 0.8;
    let product = wing_fuel_volume_m3(&wing, usable_fraction);
    let taper = wing.taper_ratio();
    let term_taper = (1.0 + taper + taper.powi(2)) / (1.0 + taper).powi(2);
    let root_t_over_c = wing.xsecs[0]
        .airfoil
        .max_thickness(&linspace(0.0, 1.0, 101));
    let expected = 0.54
        * (wing.reference_area().powi(2) / wing.reference_span())
        * root_t_over_c
        * term_taper
        * usable_fraction;
    assert!(
        (product - expected).abs() < 1e-12,
        "product={product}, expected={expected}"
    );

    let compatibility = wing_fuel_volume_m3_reference_compatibility(&wing, usable_fraction);
    assert!(
        (compatibility - product).abs() > 1e-6,
        "dihedral must distinguish the explicit parity convention"
    );
}

/// The frozen objective's stall guard, replayed through the parity
/// constructor that is its only remaining entry point.
#[test]
fn a_cruise_lift_above_the_configured_limit_is_rejected_before_trim() {
    let mut config = AlasConfig::default();
    config.requirements.max_cruise_cl = 0.0;
    let failure_cost = config.optimizer.weights.failure_cost;
    let mut objective = DesignObjective::new_reference_compatibility(config);

    let actual = objective.evaluate(&DesignVector::default().to_array());

    assert_eq!(actual, failure_cost);
    assert_eq!(objective.history.valid, vec![false]);
    assert_eq!(
        objective.history.reject_reason,
        vec!["stall_guard".to_owned()]
    );
}

#[test]
fn malformed_design_vectors_record_the_failure_cost_in_objective_history() {
    let config = AlasConfig::default();
    let failure_cost = config.optimizer.weights.failure_cost;
    let mut objective = DesignObjective::new(config);

    let actual = objective.evaluate(&[]);

    assert_eq!(actual, failure_cost);
    assert_eq!(objective.history.cost, vec![failure_cost]);
    assert_eq!(objective.history.valid, vec![false]);
    assert_eq!(
        objective.history.reject_reason,
        vec!["geometry_build".to_owned()]
    );
}

/// A wing-area limit the candidate exceeds is a hard geometry residual: the
/// candidate is infeasible, names the residual, and still costs a finite
/// amount that orders it against other infeasible candidates.
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
    assert!(constrained_objective.history.reject_reason[0].contains("wing_area"));
    assert!(!constrained_objective.history.valid[0]);
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

/// With every requirement family diagnostic, a physical miss is reported
/// but does not make the candidate infeasible.
#[test]
fn diagnostic_families_accept_physical_misses_after_a_completed_analysis() {
    let mut config = AlasConfig::default();
    config.requirements.max_wing_area_m2 = 1.0;
    config.optimizer.objective.mass_constraints = ConstraintPolicy::Diagnostic;
    config.optimizer.objective.balance_constraints = ConstraintPolicy::Diagnostic;
    config.optimizer.objective.performance_constraints = ConstraintPolicy::Diagnostic;
    config.optimizer.objective.geometry_constraints = ConstraintPolicy::Diagnostic;

    let mut objective = DesignObjective::new(config);
    let cost = objective.evaluate(&DesignVector::default().to_array());

    assert!(cost.is_finite());
    assert_eq!(objective.history.valid, vec![true]);
    assert_eq!(objective.history.reject_reason, vec![String::new()]);
}

/// The shape priors of the frozen weight table are inert for the product
/// objective, whether or not they are enabled: the mission-sized search is
/// bounded by requirement residuals, not by penalty weights.
#[test]
fn subjective_shape_bounds_do_not_condition_the_product_search() {
    let mut relaxed = AlasConfig::default();
    relaxed.optimizer.weights.fuselage_floor_m = 0.0;
    let mut aggressive = relaxed.clone();
    aggressive.optimizer.weights.fuselage_floor_m = 1_000.0;
    aggressive.optimizer.weights.transport_shape_priors_enabled = true;

    let design = DesignVector::default().to_array();
    let relaxed_cost = DesignObjective::new(relaxed).evaluate(&design);
    let conditioned_cost = DesignObjective::new(aggressive).evaluate(&design);

    assert!(relaxed_cost.is_finite());
    assert_eq!(conditioned_cost, relaxed_cost);
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
fn a_real_preset_recomputes_capacity_during_candidate_preparation() {
    let mut config = AlasConfig::from_value(&serde_json::json!({"preset": "A220-300"}))
        .expect("the registered preset loads");
    let target = config.requirements.num_passengers;

    apply_candidate_payload_load_case(&mut config, &DesignVector::default())
        .expect("a fixed load case needs no geometry-derived capacity");

    assert_eq!(target, 130);
    assert_ne!(config.requirements.num_passengers, target);
    assert!(config.requirements.num_passengers > 0);
    assert_eq!(config.requirements.cabin_preset, "Custom");
}

#[test]
fn passenger_capacity_is_recomputed_without_an_opt_in_switch() {
    let mut config = AlasConfig::from_value(&serde_json::json!({"preset": "A220-300"}))
        .expect("the registered preset loads");
    let design = alas_config::presets::get("A220-300")
        .expect("the registered preset has a design")
        .design_vector;
    let target = config.requirements.num_passengers;
    config.requirements.cabin_preset = "Ryanair".to_owned();
    config.requirements.optimize_passenger_capacity = false;

    apply_candidate_payload_load_case(&mut config, &design)
        .expect("the capacity load case resolves");

    assert_ne!(config.requirements.num_passengers, target);
}
