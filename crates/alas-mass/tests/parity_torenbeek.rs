// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-mass::torenbeek` against AeroSandbox's
//! `torenbeek_weights.py`, via `golden/generators/gen_mass_torenbeek.py`.
//!
//! Every quantity here is closed-form `f64` arithmetic, no factorization,
//! spline fit or iteration, so the whole row is checked at `Tier::Closed`,
//! matching `docs/PORTING.md`.

// This file is itself a test binary, so an unwrap or expect that fails is
// the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_geom::asb::airfoil::Airfoil;
use alas_geom::asb::fuselage::{Fuselage, FuselageXSec};
use alas_geom::asb::wing::{Wing, WingXSec};
use alas_mass::torenbeek::{mass_fuselage_simple, mass_wing};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

/// The three wings the generator builds (shaped like `aircraft_builder.py`'s
/// main wing, hstab and vstab) rebuilt from the same literal values the
/// generator's own docstring records. Identical to the geometry
/// `parity_asb_wing.rs` already reconstructs; duplicated here rather than
/// shared, since a parity test's whole point is to stand on its own.
fn build_main_wing() -> Wing {
    let root_z_m = -2.1;
    let break_z_m = -0.3;
    let tip_z_m = 2.5;
    let root_twist_deg = 4.0;
    let break_twist_deg = 2.0;
    let break_span_fraction = 0.35;
    let outboard_sweep_decrement_deg: f64 = 2.0;

    let span_m: f64 = 71.75;
    let root_chord_m = 16.50;
    let break_chord_m = 7.80;
    let tip_chord_m = 1.60;
    let sweep_deg: f64 = 34.00;
    let tip_twist_deg = 0.00;

    let semi_span = span_m / 2.0;
    let y_break = break_span_fraction * semi_span;
    let sweep_in = sweep_deg.to_radians();
    let sweep_out = (sweep_deg - outboard_sweep_decrement_deg).to_radians();
    let dx_break = y_break * sweep_in.tan();
    let dx_tip = dx_break + (semi_span - y_break) * sweep_out.tan();

    let root_section = Airfoil::from_name("naca4412").expect("naca4412 parses");
    let tip_airfoil = Airfoil::from_name("naca2410").expect("naca2410 parses");

    Wing::new(
        "Main Wing",
        vec![
            WingXSec::new(
                [0.0, 0.0, root_z_m],
                root_chord_m,
                root_twist_deg,
                root_section.clone(),
            ),
            WingXSec::new(
                [dx_break, y_break, break_z_m],
                break_chord_m,
                break_twist_deg,
                root_section,
            ),
            WingXSec::new(
                [dx_tip, semi_span, tip_z_m],
                tip_chord_m,
                tip_twist_deg,
                tip_airfoil,
            ),
        ],
        true,
    )
}

fn build_hstab() -> Wing {
    let tail_airfoil = Airfoil::from_name("naca0012").expect("naca0012 parses");
    Wing::new(
        "Horizontal Stabilizer",
        vec![
            WingXSec::new([0.0, 0.0, 0.0], 8.0, -2.0, tail_airfoil.clone()),
            WingXSec::new([7.5, 11.0, 1.0], 2.2, -2.0, tail_airfoil),
        ],
        true,
    )
}

fn build_vstab() -> Wing {
    let tail_airfoil = Airfoil::from_name("naca0012").expect("naca0012 parses");
    Wing::new(
        "Vertical Stabilizer",
        vec![
            WingXSec::new([0.0, 0.0, 0.0], 9.5, 0.0, tail_airfoil.clone()),
            WingXSec::new([9.0, 0.0, 9.8], 3.2, 0.0, tail_airfoil),
        ],
        false,
    )
}

fn wing_by_name(name: &str) -> Wing {
    match name {
        "main_wing" => build_main_wing(),
        "hstab" => build_hstab(),
        "vstab" => build_vstab(),
        other => panic!("fixture named an unexpected wing: {other}"),
    }
}

/// Evenly spaced points from `start` to `stop`, inclusive: NumPy's
/// `linspace(start, stop, num, endpoint=True)`. Duplicated from
/// `alas_geom::asb::spacing::linspace` for the reason
/// `alas-mass::torenbeek` itself already duplicates it: that module is
/// private to `alas-geom`'s `asb` module.
fn linspace(start: f64, stop: f64, num: usize) -> Vec<f64> {
    if num == 0 {
        return Vec::new();
    }
    if num == 1 {
        return vec![start];
    }
    let step = (stop - start) / (num - 1) as f64;
    let mut values: Vec<f64> = (0..num).map(|i| start + i as f64 * step).collect();
    let last = values.len() - 1;
    values[last] = stop;
    values
}

/// `aerosandbox.numpy.sinspace(0, 1, num)`: `1 - cos(linspace(0, pi/2,
/// num))`, bunching points near the start, with both endpoints pinned to
/// exactly `0.0`/`1.0` (upstream's own floating-point fixup).
fn sinspace01(num: usize) -> Vec<f64> {
    let mut values: Vec<f64> = linspace(0.0, std::f64::consts::FRAC_PI_2, num)
        .into_iter()
        .map(|angle| 1.0 - angle.cos())
        .collect();
    values[0] = 0.0;
    let last = values.len() - 1;
    values[last] = 1.0;
    values
}

/// `_fuselage_stations` from `gen_geom_asb_fuselage.py`/
/// `gen_mass_torenbeek.py`: 20 (x, z, r) triples, independent of the
/// circular/ovoid choice.
fn fuselage_stations() -> Vec<(f64, f64, f64)> {
    const DIAMETER_M: f64 = 6.2;
    const NOSE_Z_M: f64 = -0.5;
    const CABIN_START_X_M: f64 = 6.0;
    const CABIN_Z_M: f64 = 0.2;
    const TAILCONE_LENGTH_M: f64 = 14.0;
    const TAIL_Z_M: f64 = 1.8;
    const FUSELAGE_LENGTH_M: f64 = 76.72;

    let radius = DIAMETER_M / 2.0;
    let cabin_end = FUSELAGE_LENGTH_M - TAILCONE_LENGTH_M;

    let mut stations = Vec::new();

    let x_nose = sinspace01(10);
    for &xi in &x_nose[..x_nose.len() - 1] {
        let x_val = xi * CABIN_START_X_M;
        let z_val = CABIN_Z_M + (NOSE_Z_M - CABIN_Z_M) * (1.0 - xi).powi(2);
        let r_val = radius * (1.0 - (1.0 - xi).powi(2)).sqrt();
        stations.push((x_val, z_val, r_val));
    }

    stations.push((CABIN_START_X_M, CABIN_Z_M, radius));
    stations.push((cabin_end, CABIN_Z_M, radius));

    let x_tail = linspace(0.0, 1.0, 10);
    for &xi in &x_tail[1..] {
        let x_val = cabin_end + xi * TAILCONE_LENGTH_M;
        let z_val = CABIN_Z_M + (TAIL_Z_M - CABIN_Z_M) * xi.powf(1.5);
        let r_val = radius * (1.0 - xi.powf(1.5));
        stations.push((x_val, z_val, r_val));
    }

    stations
}

fn build_main_fuselage() -> Fuselage {
    let xsecs = fuselage_stations()
        .into_iter()
        .map(|(x, z, r)| {
            FuselageXSec::new([x, 0.0, z], Some(r), None, None, 2.0).expect("radius alone is valid")
        })
        .collect();
    Fuselage::new("Fuselage", xsecs)
}

fn build_ovoid_fuselage(height_m: f64) -> Fuselage {
    const DIAMETER_M: f64 = 6.2;
    let xsecs = fuselage_stations()
        .into_iter()
        .map(|(x, z, r)| {
            let local_width = r * 2.0;
            let local_height = r * 2.0 * (height_m / DIAMETER_M);
            FuselageXSec::new(
                [x, 0.0, z],
                None,
                Some(local_width),
                Some(local_height),
                2.0,
            )
            .expect("width and height together are valid")
        })
        .collect();
    Fuselage::new("Fuselage", xsecs)
}

fn fuselage_by_name(name: &str) -> Fuselage {
    match name {
        "main_fuselage" => build_main_fuselage(),
        "ovoid_fuselage" => build_ovoid_fuselage(7.5),
        other => panic!("fixture named an unexpected fuselage: {other}"),
    }
}

#[derive(Debug, Deserialize)]
struct MassWingCase {
    wing: String,
    #[serde(rename = "design_mass_TOGW")]
    design_mass_togw: f64,
    ultimate_load_factor: f64,
    suspended_mass: f64,
    never_exceed_airspeed: f64,
    max_airspeed_for_flaps: f64,
    main_gear_mounted_to_wing: bool,
    flap_deflection_angle: f64,
    strut_y_location: Option<f64>,
    mass_wing_total: f64,
}

#[derive(Debug, Deserialize)]
struct FuselageCase {
    fuselage: String,
    never_exceed_airspeed: f64,
    wing_to_tail_distance: f64,
    mass_fuselage: f64,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    mass_wing: Vec<MassWingCase>,
    mass_fuselage_simple: Vec<FuselageCase>,
}

#[test]
fn mass_wing_matches_aerosandbox_across_wings_and_parameters() {
    let fixture: Fixture = alas_testkit::load("mass", "torenbeek");

    let mut comparison = Comparison::new("alas-mass::torenbeek::mass_wing", Tier::Closed);
    for (index, case) in fixture.mass_wing.iter().enumerate() {
        let wing = wing_by_name(&case.wing);
        let actual = mass_wing(
            &wing,
            case.design_mass_togw,
            case.ultimate_load_factor,
            case.suspended_mass,
            case.never_exceed_airspeed,
            case.max_airspeed_for_flaps,
            case.main_gear_mounted_to_wing,
            case.flap_deflection_angle,
            case.strut_y_location,
        );
        comparison.scalar(
            &format!("mass_wing[{index}] ({})", case.wing),
            actual,
            case.mass_wing_total,
        );
    }
    comparison.finish();
}

#[test]
fn mass_fuselage_simple_matches_aerosandbox() {
    let fixture: Fixture = alas_testkit::load("mass", "torenbeek");

    let mut comparison =
        Comparison::new("alas-mass::torenbeek::mass_fuselage_simple", Tier::Closed);
    for (index, case) in fixture.mass_fuselage_simple.iter().enumerate() {
        let fuselage = fuselage_by_name(&case.fuselage);
        let actual = mass_fuselage_simple(
            &fuselage,
            case.never_exceed_airspeed,
            case.wing_to_tail_distance,
        );
        comparison.scalar(
            &format!("mass_fuselage_simple[{index}] ({})", case.fuselage),
            actual,
            case.mass_fuselage,
        );
    }
    comparison.finish();
}
