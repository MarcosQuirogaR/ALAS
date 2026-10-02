// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The MTOW band and payload-adjusted sizing modes (`alas_config::MtowPlan`)
//! through the mission-sized objective.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::design_variables::DesignVector;
use alas_config::{AlasConfig, ConstraintPolicy, DesignMode, FuelScheme, MtowSizing};
use alas_opt::mdo::structural_feasibility::structural_design_mass_kg;
use alas_opt::objective::DesignObjective;
use alas_opt::{assess_product_candidate, CandidateAssessment};

/// The clean-sheet default with the balance family reported rather than
/// ranked, as in `mission_sized.rs`: these tests are about the takeoff-mass
/// closure, not a particular CG layout of the canonical vector.
fn clean_sheet(sizing: MtowSizing) -> AlasConfig {
    let mut config = AlasConfig::default();
    config
        .optimizer
        .weights
        .transport_planform_constraints_enabled = false;
    config.optimizer.objective.balance_constraints = ConstraintPolicy::Diagnostic;
    config.optimizer.objective.mtow_sizing = sizing;
    config
}

fn a320_reference(sizing: MtowSizing) -> (AlasConfig, DesignVector) {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" })).unwrap();
    config.optimizer.design_space.mode = DesignMode::ReferenceAdaptation;
    config.optimizer.objective.mtow_sizing = sizing;
    let design = alas_config::presets::get("A320-200").unwrap().design_vector;
    (config, design)
}

fn assess(config: &AlasConfig, design: &DesignVector) -> CandidateAssessment {
    assess_product_candidate(config, design).unwrap_or_else(|reason| panic!("{reason}"))
}

fn residual<'a>(
    assessment: &'a CandidateAssessment,
    id: &str,
) -> Option<&'a alas_opt::ConstraintResidual> {
    assessment
        .residuals
        .iter()
        .find(|residual| residual.id == id)
}

/// Every MTOW sizing mode produces a mission-sized result.
#[test]
fn every_mtow_mode_is_mission_sized() {
    let x = DesignVector::default().to_array();
    for sizing in MtowSizing::ALL {
        let config = clean_sheet(sizing);
        let mut product = DesignObjective::new(config);
        product.evaluate(&x);
        let product_mass = product.history.takeoff_mass_kg[0];
        assert!(
            product_mass.is_finite() && product_mass > 0.0,
            "{sizing:?}: product {:?}",
            product.history.reject_reason
        );
    }
}

/// The band closes on the design mission and flies the selected route
/// off-design: a short route is checked against the closed MTOW, the
/// derived design structural payload and the tank.
#[test]
fn the_band_flies_a_short_route_off_design_at_the_closed_mass() {
    let mut config = clean_sheet(MtowSizing::MtowBand);
    config.departure_airport = "Paris CDG (LFPG)".to_owned();
    config.arrival_airport = "Frankfurt (EDDF)".to_owned();
    let assessment = assess(&config, &DesignVector::default());
    let sized = &assessment.sized;
    let flight = sized
        .mtow
        .offdesign
        .as_ref()
        .expect("the route is flown off-design");
    assert!(
        flight.range_m < 0.1 * sized.design_range_m,
        "{} vs {}",
        flight.range_m,
        sized.design_range_m
    );
    assert_eq!(flight.mtow_kg, sized.takeoff_mass_kg);
    assert!(
        (flight.payload_limit_kg
            - (sized.mtow.derived_design_mzfw_kg - sized.operating_empty_mass_kg))
            .abs()
            < 1e-6
    );
    for id in [
        "offdesign_payload",
        "offdesign_tow",
        "offdesign_fuel_capacity",
    ] {
        let found = residual(&assessment, id).unwrap_or_else(|| panic!("{id} missing"));
        assert!(found.raw_residual.is_finite(), "{id}");
    }
    // A short route at the design payload needs far less than the MTOW.
    // With no planning seats the design payload is the laid-out load case,
    // so the route payload meets the derived design structural payload.
    let payload = residual(&assessment, "offdesign_payload").unwrap();
    assert!(payload.raw_residual.abs() < 1e-6, "{payload:?}");
    let tow = residual(&assessment, "offdesign_tow").unwrap();
    assert!(tow.raw_residual < 0.0, "{tow:?}");
    assert!(residual(&assessment, "mtow_band_upper").is_some());
    assert!(residual(&assessment, "mtow_band_lower").is_some());
    assert!(residual(&assessment, "mtow_ceiling").is_none());
    // The objective is normalised by the band target.
    assert!(sized.takeoff_mass_kg <= config.mtow_plan().upper_bound_kg.unwrap() + 1e-6);
}

/// The payload-adjusted mode carries fuel-policy reserves and closes the
/// ledger: closed mass = OEW + payload + takeoff fuel.
#[test]
fn the_payload_adjusted_closure_carries_reserves_and_closes_the_ledger() {
    let config = clean_sheet(MtowSizing::PayloadAdjusted);
    assert_eq!(config.fuel_policy.scheme, FuelScheme::EasaBasic);
    let assessment = assess(&config, &DesignVector::default());
    let sized = &assessment.sized;
    assert!(sized.dispatch.plan.reserve_fuel_kg() > 0.0);
    let ledger_kg = sized.operating_empty_mass_kg + sized.payload_kg + sized.takeoff_fuel_kg;
    let tolerance_kg = (2.0e-4 * sized.takeoff_mass_kg).max(5.0);
    assert!(
        (sized.takeoff_mass_kg - ledger_kg).abs() <= tolerance_kg,
        "closure {} kg vs OEW + payload + fuel {} kg",
        sized.takeoff_mass_kg,
        ledger_kg
    );
    assert!(residual(&assessment, "mtow_ceiling").is_none());
    assert!(sized.mtow.offdesign.is_none());
}

/// The structure of both design modes is designed at the converged mass,
/// for a clean sheet and for a registered aircraft in reference adaptation.
#[test]
fn the_design_modes_design_the_structure_at_the_converged_mass() {
    for sizing in [MtowSizing::MtowBand, MtowSizing::PayloadAdjusted] {
        let clean = clean_sheet(sizing);
        let (reference, a320) = a320_reference(sizing);
        for (config, design, what) in [
            (clean, DesignVector::default(), "clean sheet"),
            (reference, a320, "A320 reference adaptation"),
        ] {
            let assessment = assess(&config, &design);
            let sized = &assessment.sized;
            let closure_kg = sized.takeoff_mass_kg;
            assert!(
                closure_kg.is_finite() && closure_kg > 0.0,
                "{what} {sizing:?}"
            );
            assert_eq!(
                sized.mtow.structural_basis, "closure_mass",
                "{what} {sizing:?}"
            );
            assert!(
                (sized.design_gross_mass_kg - closure_kg).abs() < 1e-6,
                "{what} {sizing:?}"
            );
            let structural_kg =
                structural_design_mass_kg(&config.at_sized_closure_mass(closure_kg));
            assert!(
                (structural_kg - closure_kg).abs() < 1e-6,
                "{what} {sizing:?}: {structural_kg}"
            );
            let expected_landing_kg = config
                .design_landing_mass_with_reserve_floor(closure_kg, Some(landing_floor(sized)));
            assert!(
                (sized.design_landing_mass_kg - expected_landing_kg).abs() < 1e-6,
                "{what} {sizing:?}"
            );
        }
    }
}

/// The fixed requirement keeps the structure and the analysis mass at the
/// declared cap on a registered aircraft.
#[test]
fn the_fixed_requirement_keeps_the_declared_cap() {
    let (config, design) = a320_reference(MtowSizing::FixedRequirement);
    let assessment = assess(&config, &design);
    assert_eq!(
        assessment.sized.takeoff_mass_kg,
        config.requirements.mtow_kg
    );
    assert_eq!(
        assessment.sized.design_gross_mass_kg,
        config.requirements.mtow_kg
    );
    assert_eq!(assessment.sized.mtow.structural_basis, "declared_cap");
}

/// Zero-fuel mass plus every fuel past the destination (contingency,
/// alternate, final reserve, additional, extra and the taxi-in budget) of
/// the sizing closure, kg.
fn landing_floor(sized: &alas_opt::SizedCandidate) -> f64 {
    let plan = &sized.dispatch.plan;
    sized.dispatch.zero_fuel_mass_kg
        + plan.contingency.kg
        + plan.alternate.kg
        + plan.final_reserve.kg
        + plan.additional.kg
        + plan.extra.kg
        + plan.taxi_in_fuel_kg()
}

/// The payload-adjusted design landing mass covers the zero-fuel mass and
/// the reserves on a short sizing mission, where the preset MLW/MTOW ratio
/// alone does not; the landing residual then compares the planned landing
/// mass with that WLDG. The fixed requirement keeps the declared MLW.
#[test]
fn the_payload_adjusted_landing_mass_covers_zero_fuel_mass_and_reserves() {
    let (mut config, design) = a320_reference(MtowSizing::PayloadAdjusted);
    // A short design range (the LEMD-LEPA distance) with extra fuel carried
    // past the destination: the floor has to hold that fuel as well.
    config.optimizer.objective.design_range_nmi = 295.0;
    config.fuel_policy.extra_fuel_kg = 400.0;
    let assessment = assess(&config, &design);
    let sized = &assessment.sized;
    assert!((sized.dispatch.plan.extra.kg - 400.0).abs() < 1e-9);
    let closure_kg = sized.takeoff_mass_kg;
    let ratio_kg = config.design_landing_mass_at_closure(closure_kg);
    let floor_kg = landing_floor(sized);
    // The short sizing mission: the ratio alone is below the floor.
    assert!(
        ratio_kg < floor_kg,
        "ratio {ratio_kg} kg, floor {floor_kg} kg"
    );
    assert!((sized.design_landing_mass_kg - floor_kg).abs() < 1e-6);
    let landing = residual(&assessment, "landing_mass").expect("landing residual");
    assert_eq!(landing.limit, sized.design_landing_mass_kg);
    let planned_kg = sized.dispatch.destination_landing_mass_kg;
    assert!((landing.raw_residual - (planned_kg - sized.design_landing_mass_kg)).abs() < 1e-6);
    // The planned landing mass is the zero-fuel mass plus everything carried
    // past the destination, extra fuel included, so the check sits on its
    // limit on one fuel basis.
    assert_eq!(landing.normalized_violation, 0.0, "{landing:?}");

    let (fixed, design) = a320_reference(MtowSizing::FixedRequirement);
    assert_eq!(
        assess(&fixed, &design).sized.design_landing_mass_kg,
        66_000.0
    );
}

/// The clean-sheet payload-adjusted landing mass is the larger of the
/// landing fraction of the closure and the zero-fuel mass plus reserves.
#[test]
fn the_clean_sheet_payload_adjusted_landing_mass_is_the_larger_bound() {
    let config = clean_sheet(MtowSizing::PayloadAdjusted);
    let assessment = assess(&config, &DesignVector::default());
    let sized = &assessment.sized;
    let fraction_kg = config.design_landing_mass_at_closure(sized.takeoff_mass_kg);
    let floor_kg = landing_floor(sized);
    assert!((sized.design_landing_mass_kg - fraction_kg.max(floor_kg)).abs() < 1e-6);
    let landing = residual(&assessment, "landing_mass").expect("landing residual");
    // The limit is designed at the reserve floor of the last closure pass;
    // the closure stops once the takeoff mass moves by less than
    // `sizing_tolerance_kg`, so the planned landing mass can sit above the
    // limit by at most that fraction of it.
    let closure_relative = config.optimizer.objective.sizing_tolerance_kg / sized.takeoff_mass_kg;
    assert!(
        landing.raw_residual < closure_relative * landing.limit,
        "{landing:?}"
    );
}

fn a220_reference(sizing: MtowSizing) -> (AlasConfig, DesignVector) {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": "A220-300" })).unwrap();
    config.optimizer.design_space.mode = DesignMode::ReferenceAdaptation;
    config.optimizer.objective.mtow_sizing = sizing;
    let design = alas_config::presets::get("A220-300").unwrap().design_vector;
    (config, design)
}

/// A registered aircraft closes the MTOW band on its charted design point
/// (ACP Issue 013: 2,150 nmi at the MZFW corner, 18,643 kg payload): the
/// closure lies inside the band about the declared MTOW, the route landing
/// mass stays below the design landing mass, and the candidate is feasible.
#[test]
fn the_a220_band_closes_on_its_charted_point_inside_the_declared_band() {
    let (config, design) = a220_reference(MtowSizing::MtowBand);
    let assessment = assess(&config, &design);
    let sized = &assessment.sized;
    assert!((sized.design_range_m - 2_150.0 * 1_852.0).abs() < 1e-6);
    assert!((sized.mtow.design_payload_kg - 18_643.0).abs() < 1e-6);
    let plan = config.mtow_plan();
    assert!(sized.takeoff_mass_kg >= plan.lower_bound_kg.unwrap());
    assert!(sized.takeoff_mass_kg <= plan.upper_bound_kg.unwrap());
    let flight = sized
        .mtow
        .offdesign
        .as_ref()
        .expect("route flown off-design");
    assert!(flight.dispatch.destination_landing_mass_kg <= sized.design_landing_mass_kg);
    assert!(sized.dispatch.destination_landing_mass_kg <= sized.design_landing_mass_kg + 1e-6);
    assert!(assessment.hard_feasible, "{:?}", assessment.residuals);
}

/// The payload-adjusted mode closes on the same charted range by default,
/// at the laid-out payload, so the closure is a design-range mass and not
/// the mass of the short selected route.
#[test]
fn the_payload_adjusted_mode_closes_on_the_charted_range_by_default() {
    let (config, design) = a220_reference(MtowSizing::PayloadAdjusted);
    let assessment = assess(&config, &design);
    let sized = &assessment.sized;
    assert!((sized.design_range_m - 2_150.0 * 1_852.0).abs() < 1e-6);
    assert!(sized.takeoff_mass_kg <= config.requirements.mtow_kg);
    assert!(sized.takeoff_mass_kg > 0.85 * config.requirements.mtow_kg);
    assert!(assessment.hard_feasible, "{:?}", assessment.residuals);
}
