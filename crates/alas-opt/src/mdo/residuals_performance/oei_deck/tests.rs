// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A test asserts on values it built here, so a failed unwrap is the assertion
// failing rather than a library invariant being broken.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::mdo::sizing::run_candidate;
use alas_prop::system::PropulsionRating;

const G: f64 = 9.806_65;

fn a320_outcome() -> (AlasConfig, crate::mdo::sizing::SizingOutcome) {
    let config = AlasConfig::from_value(&serde_json::json!({
        "preset": "A320-200",
        "optimizer": {"design_space": {"mode": "reference_adaptation"}}
    }))
    .unwrap();
    let design = alas_config::presets::get("A320-200").unwrap().design_vector;
    let outcome = run_candidate(&config, &design.to_array()).unwrap();
    (config, outcome)
}

fn jet_model(config: &AlasConfig, s_ref: f64) -> EngineOutModel {
    EngineOutModel::new(config, None, s_ref, None).unwrap()
}

fn condition(altitude_m: f64, tas_m_s: f64) -> EngineOutCondition {
    EngineOutCondition {
        altitude_m,
        isa_deviation_c: 0.0,
        tas_m_s,
    }
}

#[test]
fn the_thrust_ratio_is_the_decks_own_ratio_and_lapses_with_speed_and_elevation() {
    let config = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"})).unwrap();
    let model = jet_model(&config, 122.6);
    let reference = model
        .thrust(condition(0.0, 75.0), EngineOutRating::TakeoffGoAround, true)
        .unwrap();
    // Independent evaluation through the deck: ratio of two rated points.
    let deck = PropulsionDeck::from_engine(
        &config.geometry.engine,
        config.requirements.cruise_mach,
        config.requirements.cruise_altitude_m,
        max_climb_rate_ft_min(config.mission.profile.initial_climb_rate_m_s),
    )
    .unwrap();
    let at = |altitude_m: f64, speed: f64| {
        let flight = deck.flight_condition(altitude_m, speed, G, 0.0).unwrap();
        deck.rated_point(flight, PropulsionRating::TakeoffGoAround)
            .unwrap()
            .thrust_n
    };
    assert!((reference.1 - at(0.0, 75.0) / at(0.0, 0.0)).abs() < 1e-12);
    assert!((reference.0 * 2.0 - at(0.0, 75.0)).abs() < 1e-6);
    // 0 < r < 1 at V2, monotone decreasing in speed and in elevation.
    assert!(reference.1 > 0.0 && reference.1 < 1.0);
    let faster = model
        .thrust(condition(0.0, 90.0), EngineOutRating::TakeoffGoAround, true)
        .unwrap();
    let higher = model
        .thrust(
            condition(2_000.0, 75.0),
            EngineOutRating::TakeoffGoAround,
            true,
        )
        .unwrap();
    assert!(faster.1 < reference.1);
    assert!(higher.1 < reference.1);
    // Static sea level reproduces unity.
    let static_point = model
        .thrust(condition(0.0, 0.0), EngineOutRating::TakeoffGoAround, true)
        .unwrap();
    assert!((static_point.1 - 1.0).abs() < 1e-12);
}

#[test]
fn the_windmilling_drag_follows_the_handbook_formula_for_the_fan_diameter() {
    let config = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"})).unwrap();
    let s_ref = 122.6;
    let model = jet_model(&config, s_ref);
    let point = model
        .point(
            condition(0.0, 75.0),
            EngineOutRating::TakeoffGoAround,
            0.022,
        )
        .unwrap();
    let d = config.geometry.engine.fan_diameter_m;
    let mach = 75.0 / us1976_compute_values(0.0, 0.0).speed_of_sound_m_s;
    let area = windmilling_jet_drag_area_m2(d, &high_bypass_streams(d), mach).unwrap();
    assert!((point.windmilling_cd - area / s_ref).abs() < 1e-15);
    // Order of magnitude of the handbook worked example (0.0043 for a
    // 1.735 m fan on 122.4 m^2).
    assert!(point.windmilling_cd > 0.002 && point.windmilling_cd < 0.01);
    // No airframe and no override: the trim term is unknown, not zero.
    assert_eq!(point.asymmetric_trim_cd, None);
    assert_eq!(point.trim_basis, TrimDragBasis::Unavailable);
}

#[test]
fn configured_values_replace_the_derived_drag_terms() {
    let mut config = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"})).unwrap();
    config.performance.oei_windmilling_cd = Some(0.0071);
    config.performance.oei_asymmetric_trim_cd = Some(0.0033);
    let model = jet_model(&config, 122.6);
    let point = model
        .point(
            condition(0.0, 75.0),
            EngineOutRating::TakeoffGoAround,
            0.022,
        )
        .unwrap();
    assert_eq!(point.windmilling_cd, 0.0071);
    assert_eq!(point.asymmetric_trim_cd, Some(0.0033));
    assert_eq!(point.trim_basis, TrimDragBasis::ConfiguredOverride);
}

#[test]
fn a_candidate_airframe_gives_a_plausible_handbook_trim_drag() {
    let (config, outcome) = a320_outcome();
    let s_ref = outcome.plane.s_ref;
    let model = EngineOutModel::new(
        &config,
        Some(YawAirframe {
            plane: &outcome.plane,
            cg_x_m: outcome.cg_x,
        }),
        s_ref,
        None,
    )
    .unwrap();
    let point = model
        .point(
            condition(0.0, 72.0),
            EngineOutRating::TakeoffGoAround,
            0.022,
        )
        .unwrap();
    let trim = point.asymmetric_trim_cd.unwrap();
    assert_eq!(point.trim_basis, TrimDragBasis::ZeroSideslipBalance);
    // Torenbeek G-84..89: positive and below a few hundredths on a twin of
    // this size.
    assert!(trim > 0.0005 && trim < 0.05, "trim CD {trim}");
    // Trim drag falls with the dynamic pressure at fixed thrust asymmetry:
    // a much faster point has less trim drag.
    let fast = model
        .point(
            condition(0.0, 110.0),
            EngineOutRating::TakeoffGoAround,
            0.022,
        )
        .unwrap();
    assert!(fast.asymmetric_trim_cd.unwrap() < trim);
}

#[test]
fn second_segment_evidence_uses_a_configured_ratio_over_the_deck() {
    let mut config = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"})).unwrap();
    let model = jet_model(&config, 122.6);
    let airport = Airport::custom("Test", 0.0, 3000.0, 3000.0, 0.0, 0.0, 0.0);
    let speeds = alas_perf::performance::compute_v_speeds_at_masses(
        70_000.0,
        70_000.0,
        122.6,
        &airport,
        config.performance.cl_max_to,
        config.performance.cl_max_land,
        &config.performance,
    );
    let cd0 = |_: f64, _: f64, _: f64| 0.022;
    let derived =
        second_segment_evidence(Some(&model), &config.performance, &airport, &speeds, &cd0);
    let ratio = derived.condition.condition_to_sls_thrust_ratio.unwrap();
    assert!(ratio > 0.5 && ratio < 1.0);
    assert_eq!(
        derived.condition.windmilling_cd,
        derived.point.map(|p| p.windmilling_cd)
    );
    config.performance.oei_condition_to_sls_thrust_ratio = Some(0.9);
    let manual =
        second_segment_evidence(Some(&model), &config.performance, &airport, &speeds, &cd0);
    assert_eq!(manual.condition.condition_to_sls_thrust_ratio, Some(0.9));
    // Without a model nothing is invented.
    let none =
        second_segment_evidence(None, &PerformanceConfig::default(), &airport, &speeds, &cd0);
    assert_eq!(none.condition.condition_to_sls_thrust_ratio, None);
    assert_eq!(none.condition.windmilling_cd, None);
    assert_eq!(none.condition.asymmetric_trim_cd, None);
}

#[test]
fn a_turboprop_needs_its_installed_static_thrust_and_uses_the_reserve_rating_out() {
    let config = AlasConfig::from_value(&serde_json::json!({"preset": "ATR72-600"})).unwrap();
    assert!(EngineOutModel::new(&config, None, 61.0, None).is_err());
    let static_per_engine_n = 40_000.0;
    let model = EngineOutModel::new(&config, None, 61.0, Some(static_per_engine_n)).unwrap();
    assert!(model.is_turboprop());
    let one_out = model
        .thrust(condition(0.0, 60.0), EngineOutRating::TakeoffGoAround, true)
        .unwrap();
    let all_on = model
        .thrust(
            condition(0.0, 60.0),
            EngineOutRating::TakeoffGoAround,
            false,
        )
        .unwrap();
    // Maximum take-off reserve (2750 shp) exceeds the normal rating (2475 shp).
    assert!(one_out.0 > all_on.0);
    assert!((one_out.1 - one_out.0 / static_per_engine_n).abs() < 1e-12);
    // Feathered propeller: 4 percent of the clean CD0.
    let point = model
        .point(condition(0.0, 60.0), EngineOutRating::TakeoffGoAround, 0.03)
        .unwrap();
    assert!((point.windmilling_cd - 0.0012).abs() < 1e-15);
}

#[test]
fn the_weighted_trim_is_the_yaw_state_and_sideslip_does_not_lower_the_trim_drag() {
    let (config, outcome) = a320_outcome();
    let build = |weight_kg: Option<f64>| {
        let model = EngineOutModel::new(
            &config,
            Some(YawAirframe {
                plane: &outcome.plane,
                cg_x_m: outcome.cg_x,
            }),
            outcome.plane.s_ref,
            None,
        )
        .unwrap();
        match weight_kg {
            Some(kg) => model.with_weight_kg(kg),
            None => model,
        }
    };
    let at = |model: &EngineOutModel| {
        model
            .point(
                condition(0.0, 72.0),
                EngineOutRating::TakeoffGoAround,
                0.022,
            )
            .unwrap()
    };
    let unweighted = at(&build(None));
    let weighted = at(&build(Some(outcome.sized.takeoff_mass_kg)));
    assert_eq!(unweighted.trim_basis, TrimDragBasis::ZeroSideslipBalance);
    assert_eq!(weighted.trim_basis, TrimDragBasis::YawTrim);
    // The bank limit allows the zero-sideslip balance at this weight, and
    // sideslip toward the live engine relieves the rudder, not the fin side
    // force: the trim drag is the same.
    let (a, b) = (
        unweighted.asymmetric_trim_cd.unwrap(),
        weighted.asymmetric_trim_cd.unwrap(),
    );
    assert!((a - b).abs() < 1e-12 * a, "{a} {b}");
    // A very light aircraft cannot balance the fin force with 5 deg of bank:
    // the limit bank and a sideslip toward the failed engine complete the
    // balance with less fin side force, hence less trim drag.
    let light = at(&build(Some(0.3 * outcome.sized.takeoff_mass_kg)));
    assert!(light.asymmetric_trim_cd.unwrap() < a);
    // The yaw-control data exposes the rudder limit of the configuration.
    let control = build(None).yaw_control().unwrap();
    assert!((control.rudder_max_deflection_rad - 30.0_f64.to_radians()).abs() < 1e-12);
}
