// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A test asserts on values it built here, so a failed unwrap is the assertion
// failing rather than a library invariant being broken.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::mdo::residuals_performance::oei_deck::YawAirframe;
use crate::mdo::sizing::run_candidate;

fn a320() -> (AlasConfig, crate::mdo::sizing::SizingOutcome) {
    let config = AlasConfig::from_value(&serde_json::json!({
        "preset": "A320-200",
        "optimizer": {"design_space": {"mode": "reference_adaptation"}}
    }))
    .unwrap();
    let design = alas_config::presets::get("A320-200").unwrap().design_vector;
    let outcome = run_candidate(&config, &design.to_array()).unwrap();
    (config, outcome)
}

fn evaluate(
    config: &AlasConfig,
    outcome: &crate::mdo::sizing::SizingOutcome,
) -> Vec<(ClimbSegment, ClimbOutcome)> {
    let model = EngineOutModel::new(
        config,
        Some(YawAirframe {
            plane: &outcome.plane,
            cg_x_m: outcome.cg_x,
        }),
        outcome.plane.s_ref,
        Some(outcome.static_thrust_kn * 1_000.0),
    )
    .unwrap();
    evaluate_climb_checks(&CertificationInputs {
        config,
        drag: &outcome.sized.fuel_artifacts.drag,
        engine_out: &model,
        s_ref_m2: outcome.plane.s_ref,
        takeoff_mass_kg: outcome.sized.takeoff_mass_kg,
        landing_mass_kg: outcome.sized.design_landing_mass_kg,
        departure: outcome.departure,
        arrival: outcome.arrival,
    })
}

fn evaluated(checks: &[(ClimbSegment, ClimbOutcome)], segment: ClimbSegment) -> ClimbEvaluation {
    checks
        .iter()
        .find_map(|(candidate, outcome)| match outcome {
            ClimbOutcome::Evaluated(evaluation) if *candidate == segment => Some(*evaluation),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{segment:?} was not evaluated"))
}

#[test]
fn the_three_checks_are_evaluated_for_the_a320_with_the_clause_speeds_and_gradients() {
    let (config, outcome) = a320();
    let checks = evaluate(&config, &outcome);
    assert_eq!(checks.len(), 3);
    let perf = &config.performance;
    let final_segment = evaluated(&checks, ClimbSegment::FinalSegment);
    let approach = evaluated(&checks, ClimbSegment::ApproachClimb);
    let landing = evaluated(&checks, ClimbSegment::LandingClimb);
    assert!((final_segment.cl - perf.cl_max_clean / 1.18_f64.powi(2)).abs() < 1e-12);
    // CS 25.121(d): the approach configuration is the take-off flap when its
    // stall speed is within 10 % of the landing flap (CLmax_TO x 1.1^2 >=
    // CLmax_land). The declared-high-lift CLmax_TO of the A320 qualifies; the
    // 2.10 class default did not.
    let (cl_max_approach, _) = approach_configuration(perf);
    assert_eq!(cl_max_approach, perf.cl_max_to);
    assert!((approach.cl - cl_max_approach / 1.4_f64.powi(2)).abs() < 1e-12);
    assert!((landing.cl - perf.cl_max_land / 1.23_f64.powi(2)).abs() < 1e-12);
    assert_eq!(final_segment.gradient, 0.012);
    assert_eq!(approach.gradient, 0.021);
    assert_eq!(landing.gradient, 0.032);
    // All-engines landing climb carries neither failed-engine nor trim drag.
    assert_eq!(landing.windmilling_cd, 0.0);
    assert_eq!(landing.asymmetric_trim_cd, 0.0);
    assert!(final_segment.windmilling_cd > 0.0 && approach.windmilling_cd > 0.0);
    // Faster flight (approach 1.4 VSR0 vs VREF 1.23 VSR0) at the same mass.
    assert!(approach.speed_m_s > landing.speed_m_s);
    // Every requirement is finite and positive.
    for evaluation in [final_segment, approach, landing] {
        assert!(evaluation.required_sls_tw.is_finite() && evaluation.required_sls_tw > 0.0);
    }
}

#[test]
fn each_requirement_equals_its_closed_form_from_the_reported_terms() {
    let (config, outcome) = a320();
    let checks = evaluate(&config, &outcome);
    let n = outcome.n_engines as f64;
    let weight_ratio = outcome.sized.design_landing_mass_kg / outcome.sized.takeoff_mass_kg;
    for (segment, out_factor, weight) in [
        (ClimbSegment::FinalSegment, n / (n - 1.0), 1.0),
        (ClimbSegment::ApproachClimb, n / (n - 1.0), weight_ratio),
        (ClimbSegment::LandingClimb, 1.0, weight_ratio),
    ] {
        let evaluation = evaluated(&checks, segment);
        let by_hand = out_factor * (evaluation.gradient + evaluation.cd / evaluation.cl) * weight
            / evaluation.thrust_ratio;
        assert!(
            (evaluation.required_sls_tw / by_hand - 1.0).abs() < 1e-12,
            "{segment:?}"
        );
    }
}

#[test]
fn the_landing_climb_grows_with_the_landing_drag_and_the_spool_up_loss() {
    let (mut config, outcome) = a320();
    let base = evaluated(&evaluate(&config, &outcome), ClimbSegment::LandingClimb);
    config.performance.landing_flap_delta_cd += 0.02;
    let draggier = evaluated(&evaluate(&config, &outcome), ClimbSegment::LandingClimb);
    assert!(draggier.required_sls_tw > base.required_sls_tw);
    assert!((draggier.cd - base.cd - 0.02).abs() < 1e-12);

    config.performance.landing_flap_delta_cd -= 0.02;
    config.performance.go_around_spool_thrust_fraction = 0.95;
    let slow = evaluated(&evaluate(&config, &outcome), ClimbSegment::LandingClimb);
    assert!((slow.required_sls_tw / base.required_sls_tw - 1.0 / 0.95).abs() < 1e-12);
}

#[test]
fn the_approach_climb_includes_the_landing_flap_drag_but_not_the_gear() {
    let (mut config, outcome) = a320();
    // Pin the landing configuration: the declared take-off CLmax of the A320
    // would otherwise qualify the take-off flap as the approach flap
    // (CS 25.121(d), see `approach_configuration`).
    config.performance.cl_max_to = 2.10;
    let base = evaluated(&evaluate(&config, &outcome), ClimbSegment::ApproachClimb);
    config.performance.landing_gear_delta_cd += 0.05;
    let unchanged = evaluated(&evaluate(&config, &outcome), ClimbSegment::ApproachClimb);
    assert_eq!(unchanged.cd.to_bits(), base.cd.to_bits());
    config.performance.landing_flap_delta_cd += 0.05;
    let draggier = evaluated(&evaluate(&config, &outcome), ClimbSegment::ApproachClimb);
    assert!((draggier.cd - base.cd - 0.05).abs() < 1e-12);
}

#[test]
fn an_unresolved_aerodrome_is_an_evidence_gap_not_a_zero() {
    let (config, mut outcome) = a320();
    outcome.arrival = None;
    outcome.departure = None;
    let checks = evaluate(&config, &outcome);
    assert_eq!(checks.len(), 3);
    for (_, result) in &checks {
        assert!(matches!(result, ClimbOutcome::EvidenceGap(_)));
    }
}

#[test]
fn the_net_path_margin_is_the_gross_margin_minus_the_cs25_115_reduction() {
    // Installed thrust exactly meets the 2.4 % second-segment requirement:
    // gross gradient 2.4 %, net 2.4 - 0.8 = 1.6 % for a twin.
    let margin = net_path_gradient_margin(2, 0.024, 0.30, 0.30 / 0.8, 0.8).unwrap();
    assert!((margin - 0.016).abs() < 1e-12);
    // Quad: reduction 1.0 %.
    let quad = net_path_gradient_margin(4, 0.030, 0.30, 0.30 / 0.8, 0.8).unwrap();
    assert!((quad - 0.020).abs() < 1e-12);
    // 25.115 is defined for 2/3/4 engines only.
    assert!(net_path_gradient_margin(1, 0.024, 0.3, 0.3, 0.8).is_none());
    assert!(net_path_gradient_margin(2, 0.024, 0.3, 0.3, 0.0).is_none());
}

#[test]
fn the_approach_configuration_is_the_lowest_drag_flap_whose_vsr_is_within_110_percent_of_landing() {
    // Take-off CLmax below CLmax_land / 1.21 = 2.149: VSR would exceed 1.10
    // VSR0, so the landing flaps are the approach configuration.
    let mut perf = alas_config::PerformanceConfig {
        cl_max_land: 2.60,
        landing_flap_delta_cd: 0.040,
        oei_climb_delta_cd: 0.015,
        cl_max_to: 2.10,
        ..Default::default()
    };
    assert_eq!(approach_configuration(&perf), (2.60, 0.040));
    // At and above the boundary the take-off flaps qualify and carry less drag.
    perf.cl_max_to = 2.60 / 1.21;
    assert_eq!(approach_configuration(&perf), (2.60 / 1.21, 0.015));
    perf.cl_max_to = 2.60;
    assert_eq!(approach_configuration(&perf), (2.60, 0.015));
    // A take-off increment above the landing one never selects the take-off flaps.
    perf.oei_climb_delta_cd = 0.050;
    assert_eq!(approach_configuration(&perf), (2.60, 0.040));
    // A non-finite take-off CLmax falls back to the landing configuration.
    perf.oei_climb_delta_cd = 0.015;
    perf.cl_max_to = f64::NAN;
    assert_eq!(approach_configuration(&perf), (2.60, 0.040));
}

#[test]
fn the_atr_approach_climb_flies_take_off_flaps_and_meets_the_clause_at_the_registered_design() {
    let config = AlasConfig::from_value(&serde_json::json!({
        "preset": "ATR72-600",
        "optimizer": {"design_space": {"mode": "reference_adaptation"}}
    }))
    .unwrap();
    let design = alas_config::presets::get("ATR72-600")
        .unwrap()
        .design_vector;
    let outcome = run_candidate(&config, &design.to_array()).unwrap();
    let checks = evaluate(&config, &outcome);
    let approach = evaluated(&checks, ClimbSegment::ApproachClimb);
    let perf = &config.performance;
    // CLmax_to is within 1.10^2 of CLmax_land for the ATR: take-off flaps.
    assert!(perf.cl_max_to * 1.21 >= perf.cl_max_land);
    assert!((approach.cl - perf.cl_max_to / 1.4_f64.powi(2)).abs() < 1e-12);
    let installed_tw = outcome.n_engines as f64 * outcome.static_thrust_kn * 1_000.0
        / (outcome.sized.takeoff_mass_kg * 9.806_65);
    assert!(
        approach.required_sls_tw < installed_tw,
        "required {} against installed {installed_tw}",
        approach.required_sls_tw
    );
}
