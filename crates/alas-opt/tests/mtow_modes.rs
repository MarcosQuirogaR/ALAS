// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The MTOW band and payload-adjusted sizing modes (`alas_config::MtowPlan`)
//! through the mission-sized objective.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::design_variables::DesignVector;
use alas_config::{AlasConfig, DesignMode, FuelScheme, MtowSizing};
use alas_opt::mdo::structural_feasibility::structural_design_mass_kg;
use alas_opt::objective::DesignObjective;
use alas_opt::{assess_product_candidate, CandidateAssessment};

mod support;
use support::clean_sheet;

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

/// The band is the fixed requirement at the chosen mass: the route is flown
/// once, with the structure at that mass, and only the fixed `mtow_ceiling`
/// limits the takeoff mass (the closure beside it is a diagnostic).
#[test]
fn the_band_is_the_fixed_requirement_at_the_chosen_mass() {
    let config = clean_sheet(MtowSizing::MtowBand);
    let chosen_kg = 0.97 * config.mtow_band_target_kg();
    let design = DesignVector {
        mtow_kg: Some(chosen_kg),
        ..Default::default()
    };
    let assessment = assess(&config, &design);
    let sized = &assessment.sized;
    assert!(sized.mtow.offdesign.is_none());
    assert_eq!(sized.mtow.structural_basis, "declared_cap");
    assert_eq!(sized.design_gross_mass_kg, chosen_kg);
    for id in ["mtow_band_upper", "mtow_band_lower"] {
        assert!(residual(&assessment, id).is_none(), "{id}");
    }
    let ceiling = residual(&assessment, "mtow_ceiling").expect("the fixed ceiling");
    assert_eq!(ceiling.limit, chosen_kg);
    let closure = residual(&assessment, "mtow_band_closure").expect("the closure diagnostic");
    assert_eq!(closure.role, alas_opt::mdo::ResidualRole::Diagnostic);
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
    for sizing in [MtowSizing::PayloadAdjusted] {
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

/// Hard MTOW keeps structural design weights at the declared cap while
/// the physical loading also observes the aircraft's usable tank capacity.
#[test]
fn the_fixed_requirement_keeps_the_declared_cap() {
    let (config, design) = a320_reference(MtowSizing::FixedRequirement);
    let assessment = assess(&config, &design);
    assert_eq!(
        assessment.sized.takeoff_mass_kg,
        assessment.sized.zero_fuel_mass_kg
            + (config.requirements.mtow_kg - assessment.sized.zero_fuel_mass_kg)
                .min(assessment.sized.usable_capacity_kg)
    );
    assert_eq!(
        assessment.sized.design_gross_mass_kg,
        config.requirements.mtow_kg
    );
    assert_eq!(assessment.sized.mtow.structural_basis, "declared_cap");
    assert_eq!(
        assessment.sized.takeoff_loading.unwrap().status,
        alas_mass::loading::MtowFuelLoadingStatus::VolumeLimited
    );
}

/// The landing mass of the sizing closure's design mission, kg: the reserve
/// floor. For a settled dispatch it is the plan's own destination landing mass,
/// which equals the zero-fuel mass plus every fuel past the destination
/// (contingency, alternate, final reserve, additional, extra and the taxi-in
/// budget) up to the dispatch's settling tolerance; the limit, the search's
/// residual and the report's arrival check read this one value.
fn landing_floor(sized: &alas_opt::SizedCandidate) -> f64 {
    let plan = &sized.dispatch.plan;
    let closed_form_kg = sized.dispatch.zero_fuel_mass_kg
        + plan.contingency.kg
        + plan.alternate.kg
        + plan.final_reserve.kg
        + plan.additional.kg
        + plan.extra.kg
        + plan.taxi_in_fuel_kg();
    let floor_kg = sized
        .design_mission_landing_mass_kg()
        .expect("a finite design-mission landing mass");
    assert!(
        (floor_kg - closed_form_kg).abs() < 1.0e-3 * sized.takeoff_mass_kg,
        "floor {floor_kg} kg vs closed form {closed_form_kg} kg"
    );
    floor_kg
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
    // The limit is designed at the reserve floor of the closure, which for a
    // settled dispatch is the plan's own destination landing mass, so the
    // residual sits exactly on its limit when the floor binds: the search
    // accepts no excess that the strict reporting check would reject.
    let closure_relative = config.optimizer.objective.sizing_tolerance_kg / sized.takeoff_mass_kg;
    assert!(
        landing.raw_residual < closure_relative * landing.limit,
        "{landing:?}"
    );
    if floor_kg >= fraction_kg
        && matches!(
            sized.dispatch.status,
            alas_mass::dispatch::DispatchStatus::Converged
        )
    {
        assert_eq!(landing.raw_residual, 0.0, "{landing:?}");
        assert_eq!(
            sized.design_mission_landing_mass_kg(),
            Some(sized.dispatch.destination_landing_mass_kg)
        );
    }
}

fn a220_reference(sizing: MtowSizing) -> (AlasConfig, DesignVector) {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": "A220-300" })).unwrap();
    config.optimizer.design_space.mode = DesignMode::ReferenceAdaptation;
    config.optimizer.objective.mtow_sizing = sizing;
    let design = alas_config::presets::get("A220-300").unwrap().design_vector;
    (config, design)
}

/// A registered aircraft in the band mode, evaluated at the target, is the
/// fixed requirement at the target: same residual values, same masses.
#[test]
fn the_a220_band_at_its_target_is_the_fixed_requirement() {
    let (config, mut design) = a220_reference(MtowSizing::MtowBand);
    design.mtow_kg = Some(config.mtow_band_target_kg());
    let band = assess(&config, &design);
    let (fixed_config, fixed_design) = a220_reference(MtowSizing::FixedRequirement);
    let fixed = assess(&fixed_config, &fixed_design);
    assert_eq!(band.sized.takeoff_mass_kg, fixed.sized.takeoff_mass_kg);
    assert_eq!(band.hard_feasible, fixed.hard_feasible);
    assert_eq!(band.violated_hard_ids(), fixed.violated_hard_ids());
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
    // No hard finding. The A220's cruise_thrust, 0.3350 installed against 0.3367 required in
    // a220-atr work (the PW1500G has no cruise-thrust datum, so the generic chart deck is used
    // unscaled), now passes by 0.7 %: 0.3371 installed against 0.3346 required, with the merges
    // since then (the cruise-drag change among them is kappa-cal: clean-polar kappa 0.43 and
    // viscous margin 0.97). The toc_rate_of_climb miss was a residual speed inconsistency, fixed
    // in a220-atr. A 0.5 % margin either way is inside the model's thrust uncertainty.
    // The departure is judged determined (it clears), so the `takeoff_obstacle_clearance_undetermined` warning row is
    // no longer produced; the determined `takeoff_obstacle_clearance` row stands in its place.
    assert!(
        assessment.violated_hard_ids().is_empty(),
        "{:?}",
        assessment.residuals
    );
    assert!(!assessment
        .residuals
        .iter()
        .any(|row| row.id == "takeoff_obstacle_clearance_undetermined"));
    assert!(assessment
        .residuals
        .iter()
        .any(|row| row.id == "takeoff_obstacle_clearance" && !row.violated()));
}

/// A redesigned A220-300 whose main gear the placement rule puts on the
/// minimum nose-load boundary is judged with its gear there. Its re-sized
/// closure left the design takeoff state 3.0e-8 below the nose-share limit
/// (0.0592143 against 0.0592143, 1.5e-6 of the limit, above the placement
/// slack); the residual move was inside the chart-read precision, so it was
/// not made and the candidate was rejected, as were 14 of the 15 of a
/// payload-adjusted search. The residual move is now applied to the closure
/// as a rigid translation of the main group.
#[test]
fn a_placed_main_gear_is_judged_where_the_rule_put_it() {
    let config = AlasConfig::from_value(&serde_json::json!({
        "preset": "A220-300",
        "mses": {"enabled": false},
        "downstream": {"openvsp": false, "vspaero": false, "avl": false, "flowunsteady": false},
        "structures": {"run_nastran": false, "run_patran_export": false},
        "optimizer": {"objective": {"mtow_sizing": "payload_adjusted"}}
    }))
    .unwrap();
    let design: DesignVector = serde_json::from_value(serde_json::json!({
        "span_m": 35.00082761183533, "root_chord_m": 6.921386870232416,
        "break_chord_m": 3.5703984623689147, "tip_chord_m": 1.2534096513095048,
        "sweep_deg": 30.228820068241447, "tip_twist_deg": -0.7604234468059277,
        "wing_x_shift_m": -0.15879836328833374, "tail_scale": 0.9961879138267175,
        "fuselage_length_m": 38.7, "tail_x_shift_m": 0.0,
        "airfoil_thickness_scale": 1.0374373185456731,
        "airfoil_camber_scale": 1.023756919262098,
        "bump_upper_front": -0.00017763538734254887,
        "bump_upper_rear": 9.827752567233381e-05,
        "bump_lower_mid": 0.0002233435020549011, "bump_lower_rear": 0.0
    }))
    .unwrap();
    let assessment = assess(&config, &design);
    for id in ["min_nose_gear_load", "max_nose_gear_load"] {
        let row = residual(&assessment, id).unwrap_or_else(|| panic!("no {id} row"));
        assert!(
            !row.violated(),
            "{id}: actual {:e} limit {:e} ({:?})",
            row.actual,
            row.limit,
            row.detail
        );
    }
}

/// A charted point quoted under a stated reserve rule is closed under that rule, and the route
/// is flown off-design under the configured policy: the E195-E2's 3,000 nm is quoted with a
/// 100 nm alternate (Embraer specification sheet), the configured policy carries 200 nmi.
#[test]
fn the_design_mission_is_closed_under_the_reserve_its_range_is_quoted_under() {
    use alas_mass::fuel_plan::FuelRule;
    let name = "E195-E2";
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
    config.optimizer.objective.mtow_sizing = MtowSizing::PayloadAdjusted;
    let design = alas_config::presets::get(name).unwrap().design_vector;
    let assessment = assess(&config, &design);
    let sized = &assessment.sized;
    let alternate_nmi = |rule: FuelRule| match rule {
        FuelRule::Diversion { distance_m } => distance_m / 1_852.0,
        other => panic!("{other:?}"),
    };
    assert!((alternate_nmi(sized.dispatch.plan.alternate.rule) - 100.0).abs() < 1e-9);
    let route = sized
        .mtow
        .offdesign
        .as_ref()
        .expect("the route is flown off-design");
    assert!(
        (alternate_nmi(route.dispatch.plan.alternate.rule)
            - config.fuel_policy.alternate_distance_nmi)
            .abs()
            < 1e-9
    );
}

/// The C295 design mission is payload-defined: 2,940 kg (Jane's All the
/// World's Aircraft 2008-09, C-295M, printed p.546, the normal-weights range
/// point) flown at the TCDS-certified MTOW (EASA.A.186 Issue 08, Section 5
/// item 13, printed p.79) with the reserve Jane's states (45 min hold). In
/// the payload-adjusted mode the closure carries the laid-out payload, the
/// 6,000 kg cargo requirement.
/// No range is declared: the closure solves the largest range whose full
/// reserve fuel plan fits the fuel the certified mass leaves, so the closed
/// takeoff mass is the certified one and the reserves are carried.
#[test]
fn the_c295_design_range_is_the_models_output_at_the_certified_mtow() {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": "C295" })).unwrap();
    config.optimizer.design_space.mode = DesignMode::ReferenceAdaptation;
    config.optimizer.objective.mtow_sizing = MtowSizing::PayloadAdjusted;
    let design = alas_config::presets::get("C295").unwrap().design_vector;
    assert_eq!(config.design_mission().range.declared_nmi(), None);
    let assessment = assess(&config, &design);
    let sized = &assessment.sized;
    let plan = &sized.dispatch.plan;
    let summary = format!(
        "C295 payload-defined design mission: range {:.1} nmi, OEW {:.1} kg, payload {:.1} kg, takeoff fuel {:.1} kg, trip {:.1} kg, reserve (contingency {:.1} + alternate {:.1} + final {:.1} + additional {:.1}) kg, taxi {:.1} kg, takeoff mass {:.2} kg, usable capacity {:.1} kg, status {:?}, closed {}",
        sized.design_range_m / 1_852.0,
        sized.operating_empty_mass_kg,
        sized.payload_kg,
        plan.takeoff_fuel_kg(),
        plan.trip.kg,
        plan.contingency.kg,
        plan.alternate.kg,
        plan.final_reserve.kg,
        plan.additional.kg,
        plan.taxi.kg,
        sized.dispatch.takeoff_mass_kg,
        sized.usable_capacity_kg,
        sized.dispatch.status,
        sized.sizing_closed,
    );
    assert!(sized.sizing_closed, "{summary}");
    assert!(
        sized.design_range_m > 0.0 && sized.mission_distance_known,
        "{summary}"
    );
    // The payload-adjusted closure flies the laid-out payload (the 6,000 kg cargo
    // requirement); the charted 2,940 kg point is priced in the design database
    // (characteristics.design_point_range), not by the closure.
    assert_eq!(sized.payload_kg, config.requirements.cargo_payload_kg);
    assert_eq!(sized.payload_kg, 6_000.0);
    // The certified mass is the takeoff mass: the range is the largest the
    // fuel it leaves flies with reserves (or the tanks bind below it).
    let tolerance_kg = config.optimizer.objective.sizing_tolerance_kg;
    let tank_limited = sized.usable_capacity_kg.is_finite()
        && sized.zero_fuel_mass_kg + sized.usable_capacity_kg - plan.taxi.kg
            < config.requirements.mtow_kg;
    if !tank_limited {
        assert!(
            (sized.dispatch.takeoff_mass_kg - config.requirements.mtow_kg).abs() < tolerance_kg,
            "takeoff mass {} kg against the certified {} kg",
            sized.dispatch.takeoff_mass_kg,
            config.requirements.mtow_kg
        );
    }
    assert!(plan.contingency.kg + plan.alternate.kg + plan.final_reserve.kg > 0.0);
}
