// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The takeoff mass every MTOW sizing mode (`alas_config::MtowSizing`)
//! closes on the product path, checked against the mode's own definition.
//!
//! Every mode runs the MDA closure (`mdo::mda::converge`): the masses are
//! evaluated at a takeoff mass, the mission is dispatched on the empty mass
//! they give, and a mission-closed mode repeats this until the takeoff mass
//! reproduces itself. The closure ledger is therefore
//! `TOM = ZFW + takeoff fuel` of the mission it was closed on, within the
//! configured `sizing_tolerance_kg`.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::design_variables::DesignVector;
use alas_config::{AlasConfig, MtowSizing};
use alas_opt::{assess_product_candidate, CandidateAssessment, SizedCandidate};

mod support;
use support::clean_sheet;

fn assess(config: &AlasConfig) -> CandidateAssessment {
    assess_product_candidate(config, &DesignVector::default())
        .unwrap_or_else(|reason| panic!("{reason}"))
}

/// `TOM - (ZFW + takeoff fuel)` of the mission the mass was closed on, kg.
fn ledger_gap_kg(sized: &SizedCandidate) -> f64 {
    sized.takeoff_mass_kg - sized.dispatch.zero_fuel_mass_kg - sized.design_mission_fuel_kg
}

fn violated(assessment: &CandidateAssessment, id: &str) -> Option<bool> {
    assessment
        .residuals
        .iter()
        .find(|residual| residual.id == id)
        .map(|residual| residual.normalized_violation > 0.0)
}

/// A mission-closed mode: the loop closed, and the takeoff mass is the empty
/// mass at that takeoff mass, plus the payload, plus the reserve-inclusive
/// fuel of the closure mission.
fn assert_mission_closed(config: &AlasConfig, sized: &SizedCandidate) {
    let tolerance_kg = config.optimizer.objective.sizing_tolerance_kg;
    assert!(
        sized.sizing_closed,
        "not closed in {}",
        sized.sizing_iterations
    );
    assert!(
        ledger_gap_kg(sized).abs() <= tolerance_kg,
        "ledger gap {} kg against {tolerance_kg} kg",
        ledger_gap_kg(sized)
    );
    assert!(
        (sized.dispatch.zero_fuel_mass_kg - sized.operating_empty_mass_kg) > 0.0,
        "no payload in the closure"
    );
    assert!(sized.dispatch.plan.reserve_fuel_kg() > 0.0);
}

#[test]
fn the_fixed_requirement_evaluates_the_declared_mass_and_checks_the_mission_against_it() {
    let config = clean_sheet(MtowSizing::FixedRequirement);
    let declared_kg = config.requirements.mtow_kg;
    let assessment = assess(&config);
    let sized = &assessment.sized;
    // One pass, at the declared mass: the aircraft is the requirement.
    assert_eq!(sized.sizing_iterations, 1);
    let load = sized.takeoff_loading.unwrap();
    assert_eq!(
        sized.takeoff_mass_kg,
        load.zero_fuel_mass_kg
            + (declared_kg - load.zero_fuel_mass_kg).min(sized.usable_capacity_kg)
    );
    assert_eq!(sized.design_gross_mass_kg, declared_kg);
    // The dispatch keeps the mass the mission needs, built on the empty mass
    // evaluated at the declared mass, and the ceiling check is its sign.
    let required_kg = sized.dispatch.takeoff_mass_kg;
    let mission_kg = sized.dispatch.zero_fuel_mass_kg + sized.dispatch.plan.takeoff_fuel_kg();
    assert!((required_kg - mission_kg).abs() <= 1e-6 * declared_kg);
    assert_eq!(
        violated(&assessment, "mtow_ceiling"),
        Some(required_kg > declared_kg)
    );
}

#[test]
fn the_mission_sized_closure_is_the_ledger_fixed_point_under_the_declared_ceiling() {
    let config = clean_sheet(MtowSizing::SizedByMission);
    let assessment = assess(&config);
    let sized = &assessment.sized;
    assert_mission_closed(&config, sized);
    assert!(
        sized.takeoff_mass_kg
            <= config.requirements.mtow_kg + config.optimizer.objective.sizing_tolerance_kg
    );
    assert_eq!(violated(&assessment, "mtow_ceiling"), Some(false));
}

#[test]
fn the_unconstrained_closure_crosses_a_declared_mass_it_only_seeds_from() {
    // The declared mass is set below what the mission needs: the mission-sized
    // mode is held at it, the unconstrained mode closes above it.
    let free = assess(&clean_sheet(MtowSizing::Unconstrained));
    let needed_kg = free.sized.takeoff_mass_kg;
    let mut capped = clean_sheet(MtowSizing::SizedByMission);
    let mut unconstrained = clean_sheet(MtowSizing::Unconstrained);
    for config in [&mut capped, &mut unconstrained] {
        config.requirements.mtow_kg = 0.95 * needed_kg;
    }
    let seeded = assess(&unconstrained);
    assert_mission_closed(&unconstrained, &seeded.sized);
    assert!(seeded.sized.takeoff_mass_kg > unconstrained.requirements.mtow_kg);
    assert_eq!(violated(&seeded, "mtow_ceiling"), None);
    let held = assess(&capped);
    assert!(
        held.sized.takeoff_mass_kg
            <= capped.requirements.mtow_kg + capped.optimizer.objective.sizing_tolerance_kg
    );
    assert!(
        !held.hard_feasible,
        "a ceiling below the mission must reject: {:?}",
        held.violated_hard_ids()
    );
}

#[test]
fn the_band_is_a_free_mass_under_the_same_ceiling_the_fixed_requirement_applies() {
    let band = clean_sheet(MtowSizing::MtowBand);
    let target_kg = band.mtow_band_target_kg();
    let (lower_kg, upper_kg) = band.mtow_band_limits_kg();
    assert!(
        (lower_kg - 0.9 * target_kg).abs() < 1e-6,
        "p = 10 % by default"
    );
    for chosen_kg in [lower_kg, target_kg, upper_kg] {
        let design = DesignVector {
            mtow_kg: Some(chosen_kg),
            ..Default::default()
        };
        let in_band = assess_product_candidate(&band, &design).unwrap();
        let mut fixed = clean_sheet(MtowSizing::FixedRequirement);
        fixed.requirements.mtow_kg = chosen_kg;
        let at_fixed = assess(&fixed);
        assert_eq!(
            in_band.sized.takeoff_mass_kg,
            at_fixed.sized.takeoff_mass_kg
        );
        assert_eq!(in_band.sized.design_gross_mass_kg, chosen_kg);
        assert_eq!(in_band.hard_feasible, at_fixed.hard_feasible, "{chosen_kg}");
        assert_eq!(
            violated(&in_band, "mtow_ceiling"),
            violated(&at_fixed, "mtow_ceiling")
        );
        // A takeoff-mass closure is not a band constraint any more.
        assert_eq!(violated(&in_band, "mtow_band_lower"), None);
        assert_eq!(violated(&in_band, "mtow_band_upper"), None);
    }
}

#[test]
fn the_payload_adjusted_closure_is_the_ledger_fixed_point_with_no_ceiling() {
    let config = clean_sheet(MtowSizing::PayloadAdjusted);
    let assessment = assess(&config);
    let sized = &assessment.sized;
    assert_mission_closed(&config, sized);
    assert_eq!(violated(&assessment, "mtow_ceiling"), None);
    // The structure is designed at the closed mass.
    assert!((sized.design_gross_mass_kg - sized.takeoff_mass_kg).abs() < 1e-6);
}
