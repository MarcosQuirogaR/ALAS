// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-stab::dynamics` against `alas/physics/dynamics.py`, via
//! `golden/generators/gen_stab_dynamics.py`.
//!
//! # One tier, `linalg`, and a tightening deliberately not made
//!
//! `docs/PORTING.md` names a single `linalg` for this row, exactly as it does
//! for the sibling `alas-stab::trim`, and both halves are compared there.
//! `compute_dynamic_modes` earns it outright: every eigenvalue it reports is a
//! function of finite differences of dense VLM AIC solves, and the finite
//! differencing amplifies the sub-`linalg` LAPACK-vs-Gaussian residual besides:
//! the spiral eigenvalue, the smallest and most sensitive, lands about
//! 1.9e-11 from the reference, two orders inside `linalg` and just past
//! `closed`. `estimate_inertia` is closed-form geometry and agrees far tighter
//! than `linalg`; comparing it at the row's declared `linalg` is a less-tight
//! bound than that half could bear, never a loosening. Splitting the row into
//! a `closed` half and a `linalg` half is a tier decision that belongs in the
//! ledger rather than in a test, and the ledger keeps this row single-tier.
//! (`get_modes`'s own closed-form-ness is what `alas-stab::modes` checks at
//! `closed`, on fixed derivative sets.)
//!
//! The dutch roll is compared through its natural frequency and its stability
//! flag only: `alas-stab::modes` takes it as the root pair of the
//! sideslip/yaw-rate quadratic, which the reference's real and imaginary parts
//! are not (module doc there). The reference's imaginary part is the undamped
//! frequency, which equals the modulus of the complex root reported here.
//!
//! # The aircraft
//!
//! This runs on the frozen-reference aircraft and checks its reference
//! dimensions and names first: product geometry has a newer transport-planform
//! default, so using it here would compare different aeroplanes.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::HashMap;

use alas_aero::operating_point::OperatingPoint;
use alas_atmo::{Atmosphere, GAS_CONSTANT_AIR, STANDARD_GRAVITY};
use alas_config::geometry::GeometryConfig;
use alas_geom::asb::airplane::Airplane;
use alas_geom::builder::AircraftBuilder;
use alas_stab::dynamics::{self, DynamicMode};
use alas_stab::modes::MassProperties;
use alas_testkit::reference_ambient::reference_ambient;
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct AirplaneFixture {
    s_ref: f64,
    c_ref: f64,
    b_ref: f64,
    wing_names: Vec<String>,
    fuselage_names: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct InertiaInputs {
    mass_kg: f64,
}

#[derive(Debug, Deserialize)]
struct InertiaCase {
    inputs: InertiaInputs,
    ixx: f64,
    iyy: f64,
    izz: f64,
}

#[derive(Debug, Deserialize)]
struct ModeFixture {
    eigenvalue_real: f64,
    eigenvalue_imag: f64,
    damping_ratio: f64,
    period_s: f64,
    stable: bool,
}

#[derive(Debug, Deserialize)]
struct DynamicInputs {
    altitude_m: f64,
    velocity: f64,
    alpha: f64,
    mass_kg: f64,
}

#[derive(Debug, Deserialize)]
struct DynamicFixture {
    inputs: DynamicInputs,
    modes: HashMap<String, ModeFixture>,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    airplane: AirplaneFixture,
    estimate_inertia: HashMap<String, InertiaCase>,
    compute_dynamic_modes: DynamicFixture,
}

fn build() -> Airplane {
    let mut plane = AircraftBuilder::new_reference_compatibility(Some(GeometryConfig::default()))
        .build(None, true)
        .expect("the default aircraft builds");
    // Preserve the historical area scale for this frozen translation fixture;
    // its lateral/Y b_ref was already the builder's projected value.
    if let Some(wing) = plane.wings.first() {
        let s_ref = wing.unfolded_area();
        plane.s_ref = s_ref;
    }
    plane
}

fn names<T>(cases: &HashMap<String, T>) -> Vec<&String> {
    let mut names: Vec<&String> = cases.keys().collect();
    names.sort();
    names
}

#[test]
fn the_two_implementations_are_analysing_the_same_aeroplane() {
    let fixture: Fixture = alas_testkit::load("stab", "dynamics");
    let plane = build();

    let mut discrete = Comparison::new("dynamics.airplane (discrete)", Tier::Exact);
    discrete.exact(
        "wing_names",
        &plane
            .wings
            .iter()
            .map(|w| w.name.clone())
            .collect::<Vec<_>>(),
        &fixture.airplane.wing_names,
    );
    discrete.exact(
        "fuselage_names",
        &plane
            .fuselages
            .iter()
            .map(|f| f.name.clone())
            .collect::<Vec<_>>(),
        &fixture.airplane.fuselage_names,
    );
    discrete.finish();

    let mut numeric = Comparison::new("dynamics.airplane", Tier::Closed);
    numeric.scalar("s_ref", plane.s_ref, fixture.airplane.s_ref);
    numeric.scalar("c_ref", plane.c_ref, fixture.airplane.c_ref);
    numeric.scalar("b_ref", plane.b_ref, fixture.airplane.b_ref);
    numeric.finish();
}

#[test]
fn estimate_inertia_matches_python() {
    let fixture: Fixture = alas_testkit::load("stab", "dynamics");
    let plane = build();

    // Closed-form geometry (radii of gyration times mass, no solve): it
    // agrees far tighter than the row's `linalg`, but the row is single-tier,
    // so it is compared there: a less-tight bound than it could bear, never
    // a loosening. See the module doc.
    let mut comparison = Comparison::new("dynamics.estimate_inertia", Tier::Linalg);
    for name in names(&fixture.estimate_inertia) {
        let case = &fixture.estimate_inertia[name];
        let (ixx, iyy, izz) =
            dynamics::estimate_inertia_reference_compatibility(&plane, case.inputs.mass_kg);
        comparison.scalar(&format!("{name}.ixx"), ixx, case.ixx);
        comparison.scalar(&format!("{name}.iyy"), iyy, case.iyy);
        comparison.scalar(&format!("{name}.izz"), izz, case.izz);
    }
    comparison.finish();
}

fn compare_mode(
    comparison: &mut Comparison,
    name: &str,
    actual: &DynamicMode,
    expected: &ModeFixture,
) {
    comparison.scalar(
        &format!("{name}.eigenvalue_real"),
        actual.eigenvalue_real,
        expected.eigenvalue_real,
    );
    comparison.scalar(
        &format!("{name}.eigenvalue_imag"),
        actual.eigenvalue_imag,
        expected.eigenvalue_imag,
    );
    comparison.scalar(
        &format!("{name}.damping_ratio"),
        actual.damping_ratio,
        expected.damping_ratio,
    );
    comparison.scalar(
        &format!("{name}.period_s"),
        actual.period_s,
        expected.period_s,
    );

    let mut discrete = Comparison::new(format!("{name} (discrete)"), Tier::Exact);
    discrete.exact("stable", &actual.stable, &expected.stable);
    discrete.finish();
}

/// The reference's gravity in the phugoid term, m/s^2: the two-decimal
/// default of its `get_modes`.
const REFERENCE_GRAVITY: f64 = 9.81;

/// The reference's phugoid, restated at the product's gravity.
///
/// The phugoid is the root pair of `s^2 - 2 sigma s + (sigma^2 + omega^2)`
/// with `sigma = x_u / 2` and `omega^2 = -sigma^2 - g z_u / u0`; the product
/// reads the standard gravity 9.80665 m/s^2 where the reference's `get_modes`
/// reads 9.81. Gravity enters only through `omega^2 + sigma^2 = -g z_u / u0`,
/// the squared natural frequency, and `sigma` does not depend on it, so the
/// reference mode at the product's gravity follows exactly from the recorded
/// real part and the recorded natural frequency, with no other input:
/// `wn' = wn sqrt(g / 9.81)`, `omega' = sqrt(wn'^2 - sigma^2)`,
/// `zeta' = -sigma / wn'` and `T' = 2 pi / wn'`. The factor is 0.99983.
fn phugoid_at_product_gravity(mode: &ModeFixture) -> ModeFixture {
    assert!(
        mode.eigenvalue_imag > 0.0,
        "an aperiodic phugoid has no natural frequency to restate"
    );
    let sigma = mode.eigenvalue_real;
    let wn_squared =
        (sigma.powi(2) + mode.eigenvalue_imag.powi(2)) * (STANDARD_GRAVITY / REFERENCE_GRAVITY);
    let wn = wn_squared.sqrt();
    ModeFixture {
        eigenvalue_real: sigma,
        eigenvalue_imag: (wn_squared - sigma.powi(2)).sqrt(),
        damping_ratio: -sigma / wn,
        period_s: 2.0 * std::f64::consts::PI / wn,
        stable: mode.stable,
    }
}

#[test]
fn compute_dynamic_modes_matches_python() {
    let fixture: Fixture = alas_testkit::load("stab", "dynamics");
    let plane = build();
    let case = &fixture.compute_dynamic_modes;

    // The fixture was generated with the reference's fitted atmosphere, which
    // differs from the U.S. Standard Atmosphere, 1976 the product evaluates
    // by 1.5 % in temperature at 11 km. The mode solve reads the ambient
    // state only through the density (the velocity is an input and the
    // derivatives are coefficients), so the model is driven with the
    // reference's recorded density at the altitude
    // (`golden/atmo/differentiable.json`), through the pressure of the level
    // and a temperature deviation.
    let level = Atmosphere::new(case.inputs.altitude_m);
    let reference = reference_ambient(case.inputs.altitude_m);
    let temperature = level.pressure() / (GAS_CONSTANT_AIR * reference.density_kg_m3);
    let op_point = OperatingPoint::new(
        level.with_temperature_deviation(temperature - level.temperature()),
        case.inputs.velocity,
        case.inputs.alpha,
        0.0,
        0.0,
        0.0,
        0.0,
    );
    // Reproduce the production chain: the inertia fed to the mode solve comes
    // from estimate_inertia on the same aircraft.
    let (ixx, iyy, izz) =
        dynamics::estimate_inertia_reference_compatibility(&plane, case.inputs.mass_kg);
    let mass = MassProperties {
        mass: case.inputs.mass_kg,
        ixx,
        iyy,
        izz,
    };

    let result = dynamics::compute_dynamic_modes_reference_compatibility(&plane, &op_point, &mass)
        .expect("the nominal aircraft meshes and solves");

    let mut comparison = Comparison::new("dynamics.compute_dynamic_modes", Tier::Linalg);
    compare_mode(
        &mut comparison,
        "phugoid",
        &result.phugoid,
        &phugoid_at_product_gravity(&case.modes["phugoid"]),
    );
    compare_mode(
        &mut comparison,
        "short_period",
        &result.short_period,
        &case.modes["short_period"],
    );
    compare_mode(
        &mut comparison,
        "roll_subsidence",
        &result.roll_subsidence,
        &case.modes["roll_subsidence"],
    );
    comparison.scalar(
        "dutch_roll.natural_frequency",
        result
            .dutch_roll
            .eigenvalue_real
            .hypot(result.dutch_roll.eigenvalue_imag),
        case.modes["dutch_roll"].eigenvalue_imag,
    );
    assert!(result.dutch_roll.eigenvalue_imag > 0.0);
    assert_eq!(result.dutch_roll.stable, case.modes["dutch_roll"].stable);
    compare_mode(
        &mut comparison,
        "spiral",
        &result.spiral,
        &case.modes["spiral"],
    );
    comparison.finish();
}
