// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Unit tests for the scalar aircraft-design objective.

use alas_config::{AlasConfig, DesignVector};
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
        vec!["design_space".to_owned()]
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

/// Invalid mandatory structural inputs reject the candidate.
#[test]
fn invalid_structure_rejects_the_candidate() {
    let mut config = AlasConfig::default();
    config.requirements.max_wing_area_m2 = 1.0;
    config.structures.max_linear_curvature_relative_error = f64::NAN;

    let mut objective = DesignObjective::new(config);
    let cost = objective.evaluate(&DesignVector::default().to_array());

    assert!(cost.is_finite());
    assert_eq!(objective.history.valid, vec![false]);
    assert!(objective.history.reject_reason[0].contains("structural_"));
}

#[test]
fn a_real_preset_recomputes_capacity_during_candidate_preparation() {
    let mut config = AlasConfig::from_value(&serde_json::json!({"preset": "A220-300"}))
        .expect("the registered preset loads");
    assert_eq!(config.requirements.num_passengers, 140);
    let target = 100;
    config.requirements.num_passengers = target;

    apply_candidate_payload_load_case(&mut config, &DesignVector::default())
        .expect("a fixed load case needs no geometry-derived capacity");

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
    let target = 100;
    config.requirements.num_passengers = target;
    config.requirements.cabin_preset = "Ryanair".to_owned();
    config.requirements.optimize_passenger_capacity = false;

    apply_candidate_payload_load_case(&mut config, &design)
        .expect("the capacity load case resolves");

    assert_ne!(config.requirements.num_passengers, target);
}

#[test]
fn explicit_count_cabin_survives_optimizer_candidate_preparation() {
    let mut config = AlasConfig::from_value(&serde_json::json!({"preset": "AVE"}))
        .expect("the registered preset loads");
    config.requirements.cabin_preset = "Emirates".to_owned();
    config.cabin.passenger.class_mix_mode = "count".to_owned();
    config.cabin.passenger.first.count = 30;
    config.cabin.passenger.business.count = 60;
    config.cabin.passenger.premium.count = 10;
    config.cabin.passenger.economy.count = 300;

    apply_candidate_payload_load_case(&mut config, &DesignVector::default())
        .expect("the explicit count load case resolves");

    assert_eq!(config.cabin.passenger.first.count, 30);
    assert_eq!(config.cabin.passenger.business.count, 60);
    assert_eq!(config.cabin.passenger.premium.count, 0);
    assert_eq!(config.cabin.passenger.economy.count, 310);
    assert_eq!(config.requirements.num_passengers, 400);
}
