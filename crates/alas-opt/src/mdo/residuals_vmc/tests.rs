// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A test asserts on values it built here, so a failed unwrap is the assertion
// failing rather than a library invariant being broken.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::mdo::sizing::run_candidate;
use alas_perf::performance::compute_v_speeds_at_masses;

fn candidate(preset: &str) -> (AlasConfig, SizingOutcome) {
    let config = AlasConfig::from_value(&serde_json::json!({
        "preset": preset,
        "optimizer": {"design_space": {"mode": "reference_adaptation"}}
    }))
    .unwrap();
    let design = alas_config::presets::get(preset).unwrap().design_vector;
    let outcome = run_candidate(&config, &design.to_array()).unwrap();
    (config, outcome)
}

#[test]
fn the_a320_minimum_control_speed_is_a_physical_fraction_of_the_reference_stall_speed() {
    let (config, outcome) = candidate("A320-200");
    let vmc = evaluate_candidate(&outcome, &config);
    let assessment = vmc.at_airport.clone().unwrap();
    // Certified transports sit well below the 1.13 VSR ceiling.
    assert!(
        (0.6..1.13).contains(&assessment.vmc_over_vsr),
        "{assessment:?}"
    );
    assert!(assessment.bank_deg <= 5.0 + 1e-9, "{assessment:?}");
    assert!(assessment.rudder_deg > 0.0 && assessment.rudder_deg <= 30.0 + 1e-6);
    // The ground estimate (rudder alone) is above the airborne value.
    assert!(assessment.vmcg_m_s.unwrap() >= assessment.vmca_m_s);
    assert!(!vmc.capped_to_declared && vmc.registered);
    let crosswind = vmc.crosswind.unwrap();
    assert!(crosswind.rudder_deg > 0.0 && crosswind.rudder_deg < crosswind.rudder_limit_deg);
}

#[test]
fn the_computed_ratio_enters_the_speed_rule_and_v2_respects_both_floors() {
    let (config, outcome) = candidate("A320-200");
    let vmc = evaluate_candidate(&outcome, &config);
    let resolved = performance_with_vmc(&config, &vmc);
    let ratio = vmc.at_airport.clone().unwrap().vmc_over_vsr;
    assert!((resolved.performance.vmc_vstall_factor - ratio).abs() < 1e-12);
    let departure = outcome.departure.unwrap();
    let speeds = compute_v_speeds_at_masses(
        outcome.sized.takeoff_mass_kg,
        outcome.sized.takeoff_mass_kg,
        outcome.plane.s_ref,
        departure,
        resolved.performance.cl_max_to,
        resolved.performance.cl_max_land,
        &resolved.performance,
    );
    // CS 25.107(b), (e): V2 >= 1.13 VSR and >= 1.10 VMC, VR >= 1.05 VMC.
    assert!(speeds.v2_ms >= 1.13 * speeds.v_stall_to_ms - 1e-9);
    assert!(speeds.v2_ms >= 1.10 * speeds.v_mc_ms - 1e-9);
    assert!(speeds.v_r_ms >= 1.05 * speeds.v_mc_ms - 1e-9);
    // The computed VMC, not the 1.13 ceiling, sets the speeds.
    assert!((speeds.v_mc_ms / speeds.v_stall_to_ms - ratio).abs() < 1e-9);
    // Unresolved: the original configuration is borrowed unchanged.
    let none = VmcOutcome {
        rule_ratio: None,
        ..vmc
    };
    assert!(matches!(
        performance_with_vmc(&config, &none),
        Cow::Borrowed(_)
    ));
}

#[test]
fn a_registered_aircraft_is_capped_at_the_declared_ratio_and_a_clean_sheet_one_is_not() {
    let (config, outcome) = candidate("A320-200");
    let mut strict = config.clone();
    strict.performance.vmc_vstall_factor = 0.5;
    let capped = evaluate_candidate(&outcome, &strict);
    assert!(capped.capped_to_declared);
    assert_eq!(capped.rule_ratio, Some(0.5));
    let mut clean_sheet = strict.clone();
    clean_sheet.optimizer.design_space.mode = DesignMode::CleanSheet;
    let uncapped = evaluate_candidate(&outcome, &clean_sheet);
    assert!(!uncapped.capped_to_declared && !uncapped.registered);
    assert!(uncapped.rule_ratio.unwrap() > 0.5);
}

#[test]
fn the_ceiling_residual_is_a_diagnostic_when_registered_and_a_constraint_when_clean_sheet() {
    let (config, outcome) = candidate("A320-200");
    let registered = vmc_residuals(
        &evaluate_candidate(&outcome, &config),
        ResidualRole::Constraint,
    );
    let ceiling = registered.iter().find(|r| r.id == "vmc_over_vsr").unwrap();
    assert_eq!(ceiling.role, ResidualRole::Diagnostic);
    assert!(!ceiling.violated());
    let mut clean_sheet = config.clone();
    clean_sheet.optimizer.design_space.mode = DesignMode::CleanSheet;
    let residuals = vmc_residuals(
        &evaluate_candidate(&outcome, &clean_sheet),
        ResidualRole::Constraint,
    );
    let ceiling = residuals.iter().find(|r| r.id == "vmc_over_vsr").unwrap();
    assert_eq!(ceiling.role, ResidualRole::Constraint);
    assert!(residuals.iter().any(|r| r.id == "crosswind_rudder"));
    let margin = residuals
        .iter()
        .find(|r| r.id == "fin_area_ratio_for_vmc_ceiling")
        .unwrap();
    // An A320 fin is not oversized for the ceiling: the required ratio is
    // below one but not negligible.
    assert!((0.05..1.0).contains(&margin.actual), "{}", margin.actual);
}

#[test]
fn an_unavailable_yaw_model_is_reported_without_ranking_and_a_tiny_fin_is_a_violation() {
    let (config, outcome) = candidate("A320-200");
    let mut plane = outcome.plane.clone();
    plane.wings.truncate(2); // no fin
    let drag = &outcome.sized.fuel_artifacts.drag;
    let no_fin = evaluate_vmc(&VmcInputs {
        config: &config,
        plane: &plane,
        cg_x_m: outcome.cg_x,
        takeoff_mass_kg: outcome.sized.takeoff_mass_kg,
        landing_mass_kg: outcome.sized.design_landing_mass_kg,
        s_ref_m2: outcome.plane.s_ref,
        static_per_engine_n: None,
        departure: outcome.departure,
        clean_cd0: &|mach, alt, isa| drag.cd_at_atmosphere(0.0, mach, alt, isa),
    });
    assert!(matches!(no_fin.at_airport, Err(VmcFailure::Unavailable(_))));
    assert_eq!(no_fin.rule_ratio, None);
    let rows = vmc_residuals(&no_fin, ResidualRole::Constraint);
    assert_eq!(rows[0].role, ResidualRole::Diagnostic);
    // Not controllable anywhere: a violation in a clean-sheet design.
    let failure = VmcOutcome {
        at_airport: Err(VmcFailure::NotControllable),
        at_sea_level: Err(VmcFailure::NotControllable),
        crosswind: None,
        rule_ratio: None,
        capped_to_declared: false,
        registered: false,
    };
    let rows = vmc_residuals(&failure, ResidualRole::Constraint);
    assert!(rows[0].violated());
    assert_eq!(rows[0].role, ResidualRole::Constraint);
}

/// The VMC table of every registered preset, at the preset's nominal take-off
/// mass: `cargo test -p alas-opt --release vmc_table -- --ignored --nocapture`.
#[test]
#[ignore = "diagnostic table of all registered presets; slow"]
// The table is the output of this diagnostic, so it prints.
#[allow(clippy::print_stderr)]
fn vmc_table() {
    for registered in alas_config::presets::registry() {
        let (config, outcome) = candidate(registered.name);
        let vmc = evaluate_candidate(&outcome, &config);
        match (&vmc.at_airport, &vmc.at_sea_level) {
            (Ok(a), Ok(s)) => eprintln!(
                "VMC {:14} m {:8.0} vmca {:6.1} vsr {:6.1} ratio {:5.3} (sl {:5.3}) vmcg {:6.1} rud {:5.1} ss {:5.1} bank {:4.1} {:?} fin_ratio {:?} xw {:?}",
                registered.name,
                outcome.sized.takeoff_mass_kg,
                a.vmca_m_s,
                a.vsr_m_s,
                a.vmc_over_vsr,
                s.vmc_over_vsr,
                a.vmcg_m_s.unwrap_or(f64::NAN),
                a.rudder_deg,
                a.sideslip_deg,
                a.bank_deg,
                a.limit,
                a.fin_area_ratio_for_ceiling,
                vmc.crosswind.map(|c| (c.sideslip_deg, c.rudder_deg)),
            ),
            (a, _) => eprintln!("VMC {:14} {a:?}", registered.name),
        }
    }
}

/// The trim state and second-segment margin of every preset at V2:
/// `cargo test -p alas-opt --release trim_table -- --ignored --nocapture`.
#[test]
#[ignore = "diagnostic table of all registered presets; slow"]
// The table is the output of this diagnostic, so it prints.
#[allow(clippy::print_stderr)]
fn trim_table() {
    use crate::mdo::residuals_performance::performance_residuals;
    for registered in alas_config::presets::registry() {
        let (config, outcome) = candidate(registered.name);
        let vmc = evaluate_candidate(&outcome, &config);
        let resolved = performance_with_vmc(&config, &vmc);
        let Some(departure) = outcome.departure else {
            continue;
        };
        let perf = &resolved.performance;
        let speeds = compute_v_speeds_at_masses(
            outcome.sized.takeoff_mass_kg,
            outcome.sized.takeoff_mass_kg,
            outcome.plane.s_ref,
            departure,
            perf.cl_max_to,
            perf.cl_max_land,
            perf,
        );
        let drag = &outcome.sized.fuel_artifacts.drag;
        let atmosphere =
            alas_atmo::us1976_compute_values(departure.elevation_m, departure.isa_deviation_c);
        let mach = speeds.v2_ms / atmosphere.speed_of_sound_m_s;
        let cd0 =
            drag.cd_at_atmosphere(0.0, mach, departure.elevation_m, departure.isa_deviation_c);
        let build = |weighted: bool| {
            let model = EngineOutModel::new(
                &resolved,
                Some(YawAirframe {
                    plane: &outcome.plane,
                    cg_x_m: outcome.cg_x,
                }),
                outcome.plane.s_ref,
                Some(outcome.static_thrust_kn * 1_000.0),
            )
            .unwrap();
            let model = if weighted {
                model.with_weight_kg(outcome.sized.takeoff_mass_kg)
            } else {
                model
            };
            model
                .point(
                    EngineOutCondition {
                        altitude_m: departure.elevation_m,
                        isa_deviation_c: departure.isa_deviation_c,
                        tas_m_s: speeds.v2_ms,
                    },
                    EngineOutRating::TakeoffGoAround,
                    cd0,
                )
                .unwrap()
        };
        let (zero, banked) = (build(false), build(true));
        let rows = performance_residuals(&outcome, &config, ResidualRole::Constraint);
        let seg = rows
            .iter()
            .find(|r| r.id == "oei_second_segment")
            .map(|r| (r.actual, r.limit));
        eprintln!(
            "TRIM {:12} v2/vsr {:5.3} trimCD0 {:?}/{:?} basis {:?}/{:?} second_segment {:?}",
            registered.name,
            speeds.v2_ms / speeds.v_stall_to_ms,
            zero.asymmetric_trim_cd,
            banked.asymmetric_trim_cd,
            zero.trim_basis,
            banked.trim_basis,
            seg.map(|(a, l)| (a, l, 100.0 * (l / a - 1.0))),
        );
    }
}

#[test]
fn every_registered_preset_has_a_computed_vmc_below_the_certification_ceiling() {
    // Certified aircraft cannot exceed 1.13 VSR; a computed value above it
    // would be a calibration gap of the fin model, not of the aircraft. The
    // lower bound guards against a model that stopped limiting control.
    for registered in alas_config::presets::registry() {
        if registered.name == "DC-10" {
            // Known open finding, covered by the expected-failure test below.
            continue;
        }
        let (config, outcome) = candidate(registered.name);
        let vmc = evaluate_candidate(&outcome, &config);
        let assessment = vmc
            .at_airport
            .clone()
            .unwrap_or_else(|error| panic!("{}: {error:?}", registered.name));
        assert!(
            (0.6..VMC_CEILING_OVER_VSR).contains(&assessment.vmc_over_vsr),
            "{}: VMC/VSR {}",
            registered.name,
            assessment.vmc_over_vsr
        );
        assert!(assessment.bank_deg <= 5.0 + 1e-9, "{}", registered.name);
        assert!(!vmc.capped_to_declared, "{}", registered.name);
        let resolved = performance_with_vmc(&config, &vmc);
        let perf = &resolved.performance;
        let speeds = compute_v_speeds_at_masses(
            outcome.sized.takeoff_mass_kg,
            outcome.sized.takeoff_mass_kg,
            outcome.plane.s_ref,
            outcome.departure.unwrap(),
            perf.cl_max_to,
            perf.cl_max_land,
            perf,
        );
        assert!(
            speeds.v2_ms >= 1.10 * speeds.v_mc_ms - 1e-9,
            "{}",
            registered.name
        );
    }
}

/// Expected failure, open finding (final pass 2026-10-16). CS 25.149(c): VMC
/// may not exceed 1.13 VSR, and the DC-10 is certificated, so a computed
/// 1.189 is a model artefact, not a property of the aircraft. VMC (fin and
/// rudder authority) does not depend on CLmax, VSR scales as 1/sqrt(CLmax_TO);
/// the class default 2.10 gave about 1.10, the handbook double-slotted value
/// 2.469 (Raymer Eq. 12.21, flap type of the DC-10 assumed, not documented)
/// gives 1.189. The 1.13 ceiling bounds the DC-10 CLmax_TO near 2.23. The
/// test is strict and must fail until the DC-10 take-off lift is resolved;
/// remove the marker then.
#[test]
#[should_panic(expected = "DC-10: VMC/VSR")]
fn dc10_computed_vmc_is_below_the_certification_ceiling_known_failure() {
    let (config, outcome) = candidate("DC-10");
    let vmc = evaluate_candidate(&outcome, &config);
    let assessment = vmc
        // CS 25.149(c) holds at sea level, whatever the departure aerodrome.
        .at_sea_level
        .clone()
        .unwrap_or_else(|error| panic!("DC-10: {error:?}"));
    assert!(
        (0.6..VMC_CEILING_OVER_VSR).contains(&assessment.vmc_over_vsr),
        "DC-10: VMC/VSR {}",
        assessment.vmc_over_vsr
    );
}

#[test]
fn the_weathercock_stability_is_positive_on_a_transport_and_negative_stability_is_a_violation() {
    let (config, outcome) = candidate("A320-200");
    let mut clean_sheet = config.clone();
    clean_sheet.optimizer.design_space.mode = DesignMode::CleanSheet;
    let vmc = evaluate_candidate(&outcome, &clean_sheet);
    let crosswind = vmc.crosswind.unwrap();
    // Fuselage plus fin only: 0.053 per rad on the A320 (probe 2026-10-19).
    assert!(
        (0.02..0.10).contains(&crosswind.cn_beta_per_rad),
        "{}",
        crosswind.cn_beta_per_rad
    );
    let rows = vmc_residuals(&vmc, ResidualRole::Constraint);
    let row = rows
        .iter()
        .find(|r| r.id == "directional_stability")
        .unwrap();
    assert_eq!(row.role, ResidualRole::Constraint);
    assert!(!row.violated());
    let unstable = VmcOutcome {
        crosswind: Some(CrosswindAssessment {
            cn_beta_per_rad: -0.02,
            ..crosswind
        }),
        ..vmc
    };
    let rows = vmc_residuals(&unstable, ResidualRole::Constraint);
    let row = rows
        .iter()
        .find(|r| r.id == "directional_stability")
        .unwrap();
    assert!(row.violated());
}
