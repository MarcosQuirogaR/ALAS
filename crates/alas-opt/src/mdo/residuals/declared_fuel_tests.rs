// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A test asserts on values it built here, so a failed unwrap is the assertion
// failing rather than a library invariant being broken.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::mdo::ResidualRole;

use super::*;

fn config(preset: &str, mode: &str) -> AlasConfig {
    AlasConfig::from_value(&serde_json::json!({
        "preset": preset,
        "optimizer": {"design_space": {"mode": mode}}
    }))
    .unwrap()
}

fn sized(config: &AlasConfig) -> SizingOutcome {
    let design = alas_config::presets::get(&config.preset)
        .unwrap()
        .design_vector;
    super::super::sizing::run_candidate(config, &design.to_array()).unwrap()
}

fn find(outcome: &SizingOutcome, config: &AlasConfig, id: &str) -> Option<ConstraintResidual> {
    mass_residuals(outcome, config)
        .into_iter()
        .find(|r| r.id == id)
}

#[test]
fn incomplete_structural_inventory_gates_every_design_mode() {
    let mut config = config("B787-9", "reference_adaptation");
    let mut outcome = sized(&config);
    outcome.structural_inventory_complete = false;
    for mode in [
        alas_config::optimizer::DesignMode::ReferenceAdaptation,
        alas_config::optimizer::DesignMode::BaselineSandbox,
        alas_config::optimizer::DesignMode::CleanSheet,
    ] {
        config.optimizer.design_space.mode = mode;
        let incomplete = find(&outcome, &config, "structural_inventory_unverified").unwrap();
        assert_eq!(incomplete.role, ResidualRole::Constraint);
        assert!(incomplete.normalized_violation > 0.0, "{mode:?}");
    }
}

#[test]
fn the_nominal_passes_on_every_preset_with_the_published_figures_as_context() {
    for preset in alas_config::presets::registry() {
        let config = config(preset.name, "reference_adaptation");
        let outcome = sized(&config);
        let declared = find(&outcome, &config, "fuel_capacity_declared").unwrap();
        assert_eq!(declared.normalized_violation, 0.0, "{}", preset.name);
        let published = find(&outcome, &config, "fuel_capacity_published");
        assert_eq!(
            published.is_some(),
            preset.reference.usable_fuel_mass_kg.is_some(),
            "{}",
            preset.name
        );
        if let Some(context) = published {
            assert_eq!(context.role, ResidualRole::Diagnostic);
        }
    }
}

#[test]
fn a_candidate_that_shrinks_the_tanks_below_the_nominal_fails() {
    let config = config("A320-200", "reference_adaptation");
    let mut outcome = sized(&config);
    let nominal = find(&outcome, &config, "fuel_capacity_declared")
        .unwrap()
        .limit;
    // The short route needs a small fraction of the tanks, so the route fuel
    // check alone cannot notice a wing that shrinks them.
    assert!(outcome.sized.ramp_fuel_kg < 0.25 * nominal);
    outcome.sized.usable_capacity_kg = 0.95 * nominal;
    let shrunk = find(&outcome, &config, "fuel_capacity_declared").unwrap();
    assert!(shrunk.normalized_violation > 0.0);
    outcome.sized.usable_capacity_kg = 1.05 * nominal;
    let grown = find(&outcome, &config, "fuel_capacity_declared").unwrap();
    assert_eq!(grown.normalized_violation, 0.0);
}

#[test]
fn a_design_mission_sets_the_requirement_and_the_nominal_capacity_is_the_fallback() {
    use alas_config::MtowSizing;
    let mut nominal_capacity = None;
    for sizing in [MtowSizing::FixedRequirement, MtowSizing::MtowBand] {
        let mut config = config("A320-200", "reference_adaptation");
        config.optimizer.objective.mtow_sizing = sizing;
        let outcome = sized(&config);
        let declared = find(&outcome, &config, "fuel_capacity_declared").unwrap();
        let sized = &outcome.sized;
        assert_eq!(
            outcome.plan.design_mission.is_some(),
            sizing == MtowSizing::MtowBand,
            "{sizing:?}"
        );
        if outcome.plan.design_mission.is_some() {
            // The tanks must hold the design mission's reserve-inclusive
            // takeoff fuel, and the route check adds the taxi-out on top.
            assert_eq!(declared.limit, sized.design_mission_fuel_kg, "{sizing:?}");
            assert!(sized.design_mission_fuel_kg > sized.design_mission_trip_fuel_kg);
            assert!(sized.ramp_fuel_kg >= sized.design_mission_fuel_kg);
        } else {
            nominal_capacity = Some(declared.limit);
        }
        // Both requirements are held against the dispatch capacity.
        let capacity_kg = sized.usable_capacity_kg;
        assert_eq!(
            declared.normalized_violation > 0.0,
            capacity_kg < declared.limit,
            "{sizing:?}"
        );
    }
    // The fixed requirement closes on the route, so it keeps the fallback,
    // which is the preset's own modelled capacity.
    let fallback = nominal_capacity.expect("the fixed requirement has no design mission");
    let preset = sized(&config("A320-200", "reference_adaptation"));
    assert!((fallback - preset.sized.usable_capacity_kg).abs() < 1e-6);
}

#[test]
fn only_a_reference_adaptation_carries_the_requirement() {
    let reference = config("A320-200", "reference_adaptation");
    let outcome = sized(&reference);
    let mut clean_sheet = reference.clone();
    clean_sheet.optimizer.design_space.mode = alas_config::DesignMode::CleanSheet;
    assert!(find(&outcome, &clean_sheet, "fuel_capacity_declared").is_none());
    assert!(find(&outcome, &clean_sheet, "fuel_capacity").is_some());
}
