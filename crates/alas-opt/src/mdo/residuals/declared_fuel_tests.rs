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
fn a_candidate_whose_tanks_cannot_hold_the_mission_fuel_fails_and_the_registered_tanks_are_context()
{
    let config = config("A320-200", "reference_adaptation");
    let mut outcome = sized(&config);
    let declared = find(&outcome, &config, "fuel_capacity_declared").unwrap();
    // Under the fixed requirement the route is the only required mission:
    // the limit is its ramp fuel (taxi + trip + reserves), not the
    // registered tank volume.
    assert_eq!(declared.limit, outcome.sized.ramp_fuel_kg);
    let nominal = find(&outcome, &config, "fuel_capacity_nominal").unwrap();
    assert_eq!(nominal.role, ResidualRole::Diagnostic);
    assert!(nominal.limit > declared.limit);
    let required_kg = declared.limit;
    outcome.sized.usable_capacity_kg = 0.95 * required_kg;
    let short = find(&outcome, &config, "fuel_capacity_declared").unwrap();
    assert!(short.normalized_violation > 0.0);
    outcome.sized.usable_capacity_kg = 1.05 * required_kg;
    let enough = find(&outcome, &config, "fuel_capacity_declared").unwrap();
    assert_eq!(enough.normalized_violation, 0.0);
}

#[test]
fn a_design_mission_and_its_off_design_route_both_bound_the_required_fuel() {
    use alas_config::MtowSizing;
    for sizing in [MtowSizing::FixedRequirement, MtowSizing::PayloadAdjusted] {
        let mut config = config("A320-200", "reference_adaptation");
        config.optimizer.objective.mtow_sizing = sizing;
        let outcome = sized(&config);
        let declared = find(&outcome, &config, "fuel_capacity_declared").unwrap();
        let sized = &outcome.sized;
        assert_eq!(
            outcome.plan.design_mission.is_some(),
            sizing == MtowSizing::PayloadAdjusted,
            "{sizing:?}"
        );
        let route_kg = sized
            .mtow
            .offdesign
            .as_ref()
            .filter(|flight| {
                matches!(
                    flight.dispatch.status,
                    alas_mass::dispatch::DispatchStatus::Converged
                )
            })
            .map(|flight| flight.dispatch.plan.ramp_fuel_kg());
        assert_eq!(route_kg.is_some(), outcome.plan.design_mission.is_some());
        let expected = route_kg.map_or(sized.ramp_fuel_kg, |kg| kg.max(sized.ramp_fuel_kg));
        assert_eq!(declared.limit, expected, "{sizing:?}");
        assert!(declared.limit >= sized.ramp_fuel_kg);
        assert_eq!(
            declared.normalized_violation > 0.0,
            sized.usable_capacity_kg < declared.limit,
            "{sizing:?}"
        );
    }
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
