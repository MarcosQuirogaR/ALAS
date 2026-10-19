// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A test asserts on values it built here, so a failed unwrap is the assertion
// failing rather than a library invariant being broken.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

/// Speed of sound large enough that the Prandtl-Glauert factor is one.
const A_INCOMPRESSIBLE: f64 = 1.0e9;
const RHO: f64 = 1.225;

/// A320-class twin: engine arm 5.75 m, fin 21.5 m^2 at 15 m, effective AR
/// 1.6, rudder 0.30 of the chord and 5.5 m^2, quarter-chord sweep 35 deg,
/// wing 122.4 m^2 and 34.1 m, rudder limit 30 deg, fuselage-only `Cn_beta`
/// of -0.10.
fn twin() -> YawControlGeometry {
    YawControlGeometry {
        balance: YawBalanceGeometry {
            engine_arm_m: 5.75,
            fin_arm_m: 15.0,
            fin_area_m2: 21.5,
            fin_effective_aspect_ratio: 1.6,
            rudder_area_m2: 5.5,
            fin_sweep_rad: 35.0_f64.to_radians(),
        },
        fin_taper_ratio: 0.33,
        rudder_chord_ratio: 0.30,
        rudder_max_deflection_rad: 30.0_f64.to_radians(),
        wing_area_m2: 122.4,
        wing_span_m: 34.1,
        cn_beta_without_fin_per_rad: -0.10,
    }
}

fn authority_of(geometry: &YawControlGeometry) -> (f64, f64) {
    let slope = fin_lift_curve_slope_per_rad(
        geometry.balance.fin_effective_aspect_ratio,
        geometry.balance.fin_sweep_rad,
        geometry.fin_taper_ratio,
        0.0,
    )
    .unwrap();
    (
        geometry.balance.fin_area_m2 * FIN_DYNAMIC_PRESSURE_RATIO * slope,
        rudder_effectiveness(geometry.rudder_chord_ratio).unwrap(),
    )
}

#[test]
fn the_fin_slope_matches_the_unswept_helmbold_closed_form_and_falls_with_sweep() {
    let unswept = fin_lift_curve_slope_per_rad(1.6, 0.0, 1.0, 0.0).unwrap();
    // 2 pi 1.6 / (2 + sqrt(4 + 1.6^2)) = 2.2038
    assert!((unswept - 2.2038).abs() < 1e-3, "{unswept}");
    let swept = fin_lift_curve_slope_per_rad(1.6, 35.0_f64.to_radians(), 0.33, 0.0).unwrap();
    assert!(swept < unswept);
    // Large aspect ratio tends to the 2D slope 2 pi / beta in the limit.
    let slender = fin_lift_curve_slope_per_rad(1.0e4, 0.0, 1.0, 0.0).unwrap();
    assert!((slender / (2.0 * PI) - 1.0).abs() < 1e-3);
    assert!(fin_lift_curve_slope_per_rad(0.0, 0.0, 1.0, 0.0).is_none());
    assert!(fin_lift_curve_slope_per_rad(1.6, 0.0, 1.0, 1.0).is_none());
}

#[test]
fn the_rudder_effectiveness_follows_the_published_curve_points() {
    // Sadraey Fig. 12.12 points printed in his worked examples.
    for (ratio, expected) in [(0.2, 0.41), (0.3, 0.52), (0.4, 0.60), (0.49, 0.664)] {
        let tau = rudder_effectiveness(ratio).unwrap();
        assert!((tau - expected).abs() < 1e-12, "{ratio}: {tau}");
    }
    let tau = rudder_effectiveness(0.30).unwrap();
    assert!((rudder_effectiveness(0.35).unwrap() - 0.56).abs() < 1e-12);
    // Beyond the points the Glauert shape continues monotonically.
    assert!(rudder_effectiveness(0.15).unwrap() < 0.41);
    assert!(rudder_effectiveness(0.6).unwrap() > 0.664);
    assert!(rudder_effectiveness(0.4).unwrap() > tau);
    assert!(rudder_effectiveness(0.0).is_none());
    assert!(rudder_effectiveness(1.0).is_none());
}

#[test]
fn the_rudder_only_balance_and_vmcg_follow_the_analytic_expressions() {
    let geometry = twin();
    let (area_slope, tau) = authority_of(&geometry);
    let force = 100_000.0;
    let v = 70.0;
    let q = 0.5 * RHO * v * v;
    let trim = geometry
        .trim(
            force,
            q,
            v / A_INCOMPRESSIBLE,
            None,
            YawTrimMode::RudderOnly,
        )
        .unwrap();
    let expected = force * 5.75 / (15.0 * q * area_slope * tau);
    assert!((trim.rudder_rad - expected).abs() < 1e-12);
    assert_eq!(trim.sideslip_rad, 0.0);
    assert_eq!(trim.bank_rad, 0.0);
    // VMCG with the rudder at its stop: V^2 = 2 N / (rho S_v eta a tau l_v dr).
    let vmcg = geometry
        .minimum_control_speed(force, RHO, A_INCOMPRESSIBLE, None, YawTrimMode::RudderOnly)
        .unwrap();
    let closed_form = (2.0 * force * 5.75
        / (RHO * area_slope * tau * 15.0 * geometry.rudder_max_deflection_rad))
        .sqrt();
    assert!(
        (vmcg.tas_m_s - closed_form).abs() < 1e-6,
        "{} {closed_form}",
        vmcg.tas_m_s
    );
    assert_eq!(vmcg.limit, ControlLimit::Rudder);
}

#[test]
fn the_banked_balance_satisfies_the_moment_and_side_force_equations() {
    let geometry = twin();
    let (area_slope, tau) = authority_of(&geometry);
    let (force, weight, v) = (100_000.0, 700_000.0, 62.0);
    let q = 0.5 * RHO * v * v;
    let mach = v / A_INCOMPRESSIBLE;
    // Limit bank: the side force is the weight component.
    let trim = geometry
        .trim(force, q, mach, Some(weight), YawTrimMode::LeastRudder)
        .unwrap();
    let fin_force = q * area_slope * (trim.sideslip_rad + tau * trim.rudder_rad);
    assert!((fin_force - trim.fin_force_n).abs() < 1e-6 * trim.fin_force_n.abs().max(1.0));
    let moment = 15.0 * trim.fin_force_n
        + q * 122.4 * 34.1 * geometry.cn_beta_without_fin_per_rad * trim.sideslip_rad;
    assert!(
        (moment - force * 5.75).abs() < 1e-6 * force * 5.75,
        "{moment}"
    );
    assert!((trim.fin_force_n - weight * trim.bank_rad.sin()).abs() < 1e-6 * weight);
    assert!(trim.bank_rad <= BANK_LIMIT_RAD + 1e-12);
}

#[test]
fn the_minimum_drag_trim_is_zero_sideslip_while_the_bank_suffices() {
    let geometry = twin();
    let (force, weight, v) = (95_000.0, 700_000.0, 75.0);
    let q = 0.5 * RHO * v * v;
    let trim = geometry
        .trim(force, q, 0.2, Some(weight), YawTrimMode::MinimumDrag)
        .unwrap();
    assert_eq!(trim.sideslip_rad, 0.0);
    let fin_force = force * 5.75 / 15.0;
    assert!((trim.fin_force_n - fin_force).abs() < 1e-9);
    assert!((trim.bank_rad - (fin_force / weight).asin()).abs() < 1e-12);
    assert!(trim.bank_rad < BANK_LIMIT_RAD);
    // A light aircraft cannot balance the force with 5 deg of bank: the
    // limit bank and a sideslip toward the failed engine complete it.
    let light = geometry
        .trim(force, q, 0.2, Some(300_000.0), YawTrimMode::MinimumDrag)
        .unwrap();
    assert!(light.bank_limited);
    assert!((light.bank_rad - BANK_LIMIT_RAD).abs() < 1e-12);
    assert!(light.sideslip_rad < 0.0);
    assert!(light.fin_force_n < fin_force);
}

#[test]
fn bank_and_sideslip_lower_the_minimum_control_speed_of_a_stable_airframe() {
    let geometry = twin();
    let (force, weight) = (110_000.0, 650_000.0);
    let ground = geometry
        .minimum_control_speed(force, RHO, 340.0, None, YawTrimMode::RudderOnly)
        .unwrap();
    let air = geometry
        .minimum_control_speed(force, RHO, 340.0, Some(weight), YawTrimMode::LeastRudder)
        .unwrap();
    assert!(
        air.tas_m_s < ground.tas_m_s - 0.5,
        "{} {}",
        air.tas_m_s,
        ground.tas_m_s
    );
    assert!(air.trim.bank_rad <= BANK_LIMIT_RAD + 1e-12);
    // The state at VMC is at the limit of one control.
    let rudder_fraction = air.trim.rudder_rad / geometry.rudder_max_deflection_rad;
    let sideslip_fraction = air.trim.sideslip_rad.abs() / SIDESLIP_LIMIT_RAD;
    assert!((rudder_fraction.max(sideslip_fraction) - 1.0).abs() < 1e-6);
}

#[test]
fn vmc_rises_with_the_engine_arm_and_thrust_and_falls_with_a_larger_fin() {
    let base = twin();
    let weight = Some(650_000.0);
    let vmc = |geometry: &YawControlGeometry, force: f64| {
        geometry
            .minimum_control_speed(force, RHO, 340.0, weight, YawTrimMode::LeastRudder)
            .unwrap()
            .tas_m_s
    };
    let reference = vmc(&base, 100_000.0);
    assert!(vmc(&base, 140_000.0) > reference);
    let wide = YawControlGeometry {
        balance: YawBalanceGeometry {
            engine_arm_m: 8.0,
            ..base.balance
        },
        ..base
    };
    assert!(vmc(&wide, 100_000.0) > reference);
    let big_fin = YawControlGeometry {
        balance: YawBalanceGeometry {
            fin_area_m2: 30.0,
            rudder_area_m2: 7.7,
            ..base.balance
        },
        ..base
    };
    assert!(vmc(&big_fin, 100_000.0) < reference);
    // A tiny fin controls the asymmetry only far above any take-off speed.
    let tiny = YawControlGeometry {
        balance: YawBalanceGeometry {
            fin_area_m2: 0.05,
            rudder_area_m2: 0.01,
            ..base.balance
        },
        ..base
    };
    let tiny_vmc = tiny.minimum_control_speed(1.0e5, RHO, 340.0, weight, YawTrimMode::LeastRudder);
    assert!(tiny_vmc.is_none_or(|speed| speed.tas_m_s > 150.0));
}

#[test]
fn the_crosswind_rudder_is_the_net_yaw_stiffness_over_the_rudder_slope() {
    let geometry = twin();
    let (area_slope, tau) = authority_of(&geometry);
    let fin_slope = area_slope * 15.0 / (122.4 * 34.1);
    let beta = 12.0_f64.to_radians();
    let rudder = geometry.crosswind_rudder_rad(beta, 0.0).unwrap();
    let net = (-0.10 + fin_slope).max(NET_DIRECTIONAL_STABILITY_FLOOR);
    let expected = net * beta / (fin_slope * tau);
    assert!((rudder - expected).abs() < 1e-12);
    // No airframe-without-fin term: the rudder cancels the weathercock
    // moment of the fin, delta = beta / tau.
    let ideal = YawControlGeometry {
        cn_beta_without_fin_per_rad: 0.0,
        ..geometry
    };
    assert!((ideal.crosswind_rudder_rad(beta, 0.0).unwrap() - beta / tau).abs() < 1e-12);
    assert!(geometry.crosswind_rudder_rad(f64::NAN, 0.0).is_none());
}

#[test]
fn the_fuselage_yaw_slope_follows_torenbeek_9_64() {
    let fuselage = FuselageYawGeometry {
        length_m: 37.6,
        cg_from_nose_m: 17.0,
        side_area_m2: 110.0,
        max_height_m: 4.14,
        height_quarter_m: 3.9,
        width_quarter_m: 3.6,
        height_three_quarter_m: 3.0,
        width_three_quarter_m: 2.4,
    };
    let cn = fuselage_cn_beta_per_rad(fuselage, 122.4, 34.1).unwrap();
    let k_beta = 0.3 * 17.0 / 37.6 + 0.75 * 4.14 / 37.6 - 0.105;
    let expected =
        -k_beta * 110.0 * 37.6 / (122.4 * 34.1) * (3.9_f64 / 3.0).sqrt() * (2.4_f64 / 3.6).cbrt();
    assert!((cn - expected).abs() < 1e-12);
    assert!(cn < 0.0 && cn > -0.5, "{cn}");
    // Eq. (9-65) needs a slender body.
    let stubby = FuselageYawGeometry {
        length_m: 10.0,
        ..fuselage
    };
    assert!(fuselage_cn_beta_per_rad(stubby, 122.4, 34.1).is_none());
}

#[test]
fn unusable_inputs_give_no_state() {
    let geometry = twin();
    assert!(geometry
        .trim(-1.0, 3000.0, 0.2, None, YawTrimMode::RudderOnly)
        .is_none());
    assert!(geometry
        .trim(1.0e5, 0.0, 0.2, None, YawTrimMode::RudderOnly)
        .is_none());
    assert!(geometry
        .minimum_control_speed(1.0e5, 0.0, 340.0, None, YawTrimMode::RudderOnly)
        .is_none());
    let bad = YawControlGeometry {
        wing_span_m: f64::NAN,
        ..geometry
    };
    assert!(bad
        .trim(1.0e5, 3000.0, 0.2, None, YawTrimMode::RudderOnly)
        .is_none());
}
