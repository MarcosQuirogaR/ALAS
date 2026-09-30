// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Tests for [`super`], the mid-cruise mass basis of the reported cruise point.

// Failed expectations and unwraps here are failed test assertions.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

const G: f64 = 9.80665;

fn polar_from(cl: Vec<f64>, cd: Vec<f64>) -> PolarSweep {
    let n = cl.len();
    PolarSweep {
        alpha_deg: (0..n).map(|i| i as f64).collect(),
        geometric_alpha_deg: vec![0.0; n],
        cd_induced: vec![0.0; n],
        cd_wave: vec![0.0; n],
        cd_parasite: vec![0.0; n],
        cm: vec![0.0; n],
        l_over_d: cl.iter().zip(&cd).map(|(c, d)| c / d).collect(),
        cl,
        cd,
    }
}

fn parabolic_polar(cd0: f64, k: f64) -> PolarSweep {
    let cl: Vec<f64> = (0..=40).map(|i| i as f64 * 0.025).collect();
    let cd = cl.iter().map(|c| cd0 + k * c * c).collect();
    polar_from(cl, cd)
}

fn parabolic_ld(cd0: f64, k: f64, cl: f64) -> f64 {
    cl / (cd0 + k * cl * cl)
}

#[test]
fn the_cruise_cl_scales_with_mass() {
    let heavy = cruise_cl_at_mass(500_000.0, G, 10_000.0, 800.0);
    let light = cruise_cl_at_mass(400_000.0, G, 10_000.0, 800.0);
    assert!((heavy / light - 1.25).abs() < 1e-12);
    assert!((heavy - 500_000.0 * G / 8.0e6).abs() < 1e-12);
}

#[test]
fn the_mid_cruise_mass_is_the_mean_of_the_breguet_endpoints() {
    let takeoff = 500_000.0;
    let fuel = 200_000.0;
    let mid = mid_cruise_mass_kg(takeoff, fuel);
    assert_eq!(mid, 0.5 * (takeoff + (takeoff - fuel)));
    assert!(mid < takeoff && mid > takeoff - fuel);
}

#[test]
fn no_or_negative_fuel_cruises_at_the_takeoff_mass() {
    let takeoff = 60_000.0;
    assert_eq!(mid_cruise_mass_kg(takeoff, 0.0), takeoff);
    assert_eq!(mid_cruise_mass_kg(takeoff, -1_500.0), takeoff);
    assert_eq!(mid_cruise_mass_kg(takeoff, f64::NAN), takeoff);
    // A zero-fuel mass above the takeoff mass gives negative fuel: clamped.
    let zero_fuel_mass = 61_000.0;
    assert_eq!(
        mid_cruise_mass_kg(takeoff, takeoff - zero_fuel_mass),
        takeoff
    );
    // More fuel than aircraft cannot take the end mass below zero.
    assert_eq!(mid_cruise_mass_kg(takeoff, 2.0 * takeoff), 0.5 * takeoff);
}

#[test]
fn interpolation_reproduces_the_polar_inside_its_range() {
    let polar = parabolic_polar(0.02, 0.04);
    let lookup = polar_l_over_d_at_cl(&polar, 0.51, 0.5).unwrap();
    // Linear between the 0.500 and 0.525 samples.
    let (a, b) = (
        parabolic_ld(0.02, 0.04, 0.5),
        parabolic_ld(0.02, 0.04, 0.525),
    );
    assert!((lookup.value - (a + (b - a) * 0.4)).abs() < 1e-12);
    assert!(!lookup.clamped);
    let cd = polar_cd_at_cl(&polar, 0.5, 0.5).unwrap();
    assert!((cd.value - (0.02 + 0.04 * 0.25)).abs() < 1e-15);
    assert!(polar_l_over_d_at_cl(&polar, f64::NAN, 0.5).is_none());
}

#[test]
fn a_lift_coefficient_outside_the_polar_clamps_to_its_end_values_and_is_flagged() {
    let polar = parabolic_polar(0.02, 0.04);
    let high = polar_l_over_d_at_cl(&polar, 5.0, 0.5).unwrap();
    assert!(high.clamped);
    assert_eq!(high.value, *polar.l_over_d.last().unwrap());
    let low = polar_cd_at_cl(&polar, -1.0, 0.5).unwrap();
    assert!(low.clamped);
    assert_eq!(low.value, polar.cd[0]);
    let empty = polar_from(vec![f64::NAN], vec![0.02]);
    assert!(polar_cd_at_cl(&empty, 0.5, 0.5).is_none());
}

#[test]
fn a_non_monotonic_polar_uses_the_bracket_nearest_the_anchor() {
    // CL rises to 1.0 and falls back past stall: CL 0.8 is crossed twice,
    // once on the attached branch (L/D 20) and once post-stall (L/D 5).
    let cl = vec![0.4, 0.8, 1.0, 0.8, 0.6];
    let cd = vec![0.4 / 20.0, 0.8 / 20.0, 1.0 / 10.0, 0.8 / 5.0, 0.6 / 4.0];
    let polar = polar_from(cl, cd);
    let attached = polar_l_over_d_at_cl(&polar, 0.7, 0.5).unwrap();
    assert!((attached.value - 20.0).abs() < 1e-12, "{attached:?}");
    let stalled = polar_l_over_d_at_cl(&polar, 0.7, 0.62).unwrap();
    assert!(
        (stalled.value - (4.0 + (5.0 - 4.0) * 0.5)).abs() < 1e-12,
        "{stalled:?}"
    );
    // Above the CL peak is out of range, clamped to the peak point.
    let above = polar_l_over_d_at_cl(&polar, 1.1, 0.5).unwrap();
    assert!(above.clamped && (above.value - 10.0).abs() < 1e-12);
}

#[test]
fn the_trim_drag_increment_is_carried_to_the_new_lift_coefficient() {
    let polar = parabolic_polar(0.02, 0.04);
    let trim_increment = 0.0012;
    let anchor_cl = 0.6;
    let anchor_cd = 0.02 + 0.04 * anchor_cl * anchor_cl + trim_increment;
    let point = cruise_point_at_cl(&polar, 0.5, anchor_cl, anchor_cd).unwrap();
    assert!((point.cd - (0.02 + 0.04 * 0.25 + trim_increment)).abs() < 1e-12);
    assert!((point.l_over_d - point.cl / point.cd).abs() < 1e-12);
    assert!(!point.clamped);
    // Re-anchoring on the moved point is idempotent.
    let again = cruise_point_at_cl(&polar, 0.5, point.cl, point.cd).unwrap();
    assert!((again.cd - point.cd).abs() < 1e-15);
    // Moving to the anchor itself returns the anchor.
    let same = cruise_point_at_cl(&polar, anchor_cl, anchor_cl, anchor_cd).unwrap();
    assert!((same.cd - anchor_cd).abs() < 1e-15);
}

#[test]
fn a_lighter_aircraft_gains_l_over_d_above_the_best_lift_coefficient_and_loses_below() {
    // Best L/D at CL = sqrt(cd0/k) = 0.707 on this synthetic polar.
    let polar = parabolic_polar(0.02, 0.04);
    for (anchor_cl, gains) in [(0.85, true), (0.45, false)] {
        let anchor_cd = 0.02 + 0.04 * anchor_cl * anchor_cl;
        let lighter = cruise_point_at_cl(&polar, 0.92 * anchor_cl, anchor_cl, anchor_cd).unwrap();
        let anchor_ld = anchor_cl / anchor_cd;
        assert_eq!(lighter.l_over_d > anchor_ld, gains, "{anchor_cl}");
    }
}

/// Dynamic pressure and area chosen so the takeoff mass flies at CL 0.6.
fn corner_inputs(takeoff_kg: f64) -> (f64, f64) {
    let area = 120.0;
    let q = takeoff_kg * G / (0.6 * area);
    (q, area)
}

#[test]
fn corner_l_over_d_moves_to_the_corner_mid_cruise_cl_on_the_caller_basis() {
    let polar = parabolic_polar(0.02, 0.04);
    let takeoff = 70_000.0;
    let fuel = 14_000.0;
    let (q, area) = corner_inputs(takeoff);
    let anchor_cd = 0.02 + 0.04 * 0.36 + 0.001;
    let anchor = (0.6, anchor_cd, 0.6 / anchor_cd);
    let corner = corner_l_over_d_on_polar(&polar, anchor, q, G, area, takeoff, fuel, 17.0);
    let expected_cl = 0.6 * (takeoff - 0.5 * fuel) / takeoff;
    assert!((corner.cruise_cl - expected_cl).abs() < 1e-12);
    let moved = cruise_point_at_cl(&polar, expected_cl, anchor.0, anchor.1).unwrap();
    assert!((corner.l_over_d - 17.0 * moved.l_over_d / anchor.2).abs() < 1e-12);
    assert!(!corner.clamped);
}

#[test]
fn a_zero_payload_corner_uses_its_own_takeoff_mass_and_fuel() {
    // Corner D: operating empty mass plus full tanks, no payload.
    let polar = parabolic_polar(0.02, 0.04);
    let oew = 40_000.0;
    let fuel = 20_000.0;
    let takeoff = oew + fuel;
    let (q, area) = corner_inputs(70_000.0);
    let anchor_cd = 0.02 + 0.04 * 0.36;
    let anchor = (0.6, anchor_cd, 0.6 / anchor_cd);
    let corner = corner_l_over_d_on_polar(&polar, anchor, q, G, area, takeoff, fuel, anchor.2);
    let expected_cl = cruise_cl_at_mass(oew + 0.5 * fuel, G, q, area);
    assert!((corner.cruise_cl - expected_cl).abs() < 1e-12);
    // Linear CD interpolation on a 0.025 CL grid errs by at most
    // k h^2 / 8 = 3.1e-6 in CD, about 1e-4 of the drag here.
    let exact = parabolic_ld(0.02, 0.04, expected_cl);
    assert!(
        (corner.l_over_d / exact - 1.0).abs() < 2e-4,
        "{} vs {exact}",
        corner.l_over_d
    );
}

#[test]
fn a_corner_without_fuel_or_with_negative_fuel_flies_at_its_takeoff_cl() {
    let polar = parabolic_polar(0.02, 0.04);
    let takeoff = 70_000.0;
    let (q, area) = corner_inputs(takeoff);
    let anchor_cd = 0.02 + 0.04 * 0.36;
    let anchor = (0.6, anchor_cd, 0.6 / anchor_cd);
    for fuel in [0.0, -500.0] {
        let corner = corner_l_over_d_on_polar(&polar, anchor, q, G, area, takeoff, fuel, anchor.2);
        assert!((corner.cruise_cl - 0.6).abs() < 1e-12, "{fuel}");
        assert!((corner.l_over_d - anchor.2).abs() < 1e-12, "{fuel}");
    }
}

#[test]
fn a_corner_outside_the_polar_is_clamped_and_flagged() {
    let polar = parabolic_polar(0.02, 0.04);
    let takeoff = 70_000.0;
    let (q, area) = corner_inputs(takeoff);
    let anchor_cd = 0.02 + 0.04 * 0.36;
    let anchor = (0.6, anchor_cd, 0.6 / anchor_cd);
    // A dynamic pressure a tenth as large puts the corner near CL 5.7.
    let corner =
        corner_l_over_d_on_polar(&polar, anchor, 0.1 * q, G, area, takeoff, 1_000.0, anchor.2);
    assert!(corner.clamped);
    assert!(corner.l_over_d.is_finite() && corner.l_over_d > 0.0);
}
