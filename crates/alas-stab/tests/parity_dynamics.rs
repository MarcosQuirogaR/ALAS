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
//! differencing amplifies the sub-`linalg` LAPACK-vs-Gaussian residual besides
//! -- the spiral eigenvalue, the smallest and most sensitive, lands about
//! 1.9e-11 from the reference, two orders inside `linalg` and just past
//! `closed`. `estimate_inertia` is closed-form geometry and agrees far tighter
//! than `linalg`; comparing it at the row's declared `linalg` is a less-tight
//! bound than that half could bear, never a loosening. Splitting the row into
//! a `closed` half and a `linalg` half is a tier decision that belongs in the
//! ledger rather than in a test, and the ledger keeps this row single-tier.
//! (`get_modes`'s own closed-form-ness is what `alas-stab::modes` checks at
//! `closed`, on fixed derivative sets.)
//!
//! # The aircraft
//!
//! Like `parity_trim.rs`, this runs on the nominal aircraft
//! `alas-geom::builder` builds -- the same object `golden/geom/builder.json`
//! pins -- and checks its reference dimensions and names first: if the two
//! sides have stopped meaning the same aeroplane, every comparison below is
//! answering a different question.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::HashMap;

use alas_aero::operating_point::OperatingPoint;
use alas_atmo::Atmosphere;
use alas_config::geometry::GeometryConfig;
use alas_geom::asb::airplane::Airplane;
use alas_geom::builder::AircraftBuilder;
use alas_stab::dynamics::{self, DynamicMode};
use alas_stab::modes::MassProperties;
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
    AircraftBuilder::new(Some(GeometryConfig::default()))
        .build(None, true)
        .expect("the default aircraft builds")
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
    // so it is compared there -- a less-tight bound than it could bear, never
    // a loosening. See the module doc.
    let mut comparison = Comparison::new("dynamics.estimate_inertia", Tier::Linalg);
    for name in names(&fixture.estimate_inertia) {
        let case = &fixture.estimate_inertia[name];
        let (ixx, iyy, izz) = dynamics::estimate_inertia(&plane, case.inputs.mass_kg);
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

#[test]
fn compute_dynamic_modes_matches_python() {
    let fixture: Fixture = alas_testkit::load("stab", "dynamics");
    let plane = build();
    let case = &fixture.compute_dynamic_modes;

    let op_point = OperatingPoint::new(
        Atmosphere::new(case.inputs.altitude_m),
        case.inputs.velocity,
        case.inputs.alpha,
        0.0,
        0.0,
        0.0,
        0.0,
    );
    // Reproduce the production chain: the inertia fed to the mode solve comes
    // from estimate_inertia on the same aircraft.
    let (ixx, iyy, izz) = dynamics::estimate_inertia(&plane, case.inputs.mass_kg);
    let mass = MassProperties {
        mass: case.inputs.mass_kg,
        ixx,
        iyy,
        izz,
    };

    let result = dynamics::compute_dynamic_modes(&plane, &op_point, &mass)
        .expect("the nominal aircraft meshes and solves");

    let mut comparison = Comparison::new("dynamics.compute_dynamic_modes", Tier::Linalg);
    compare_mode(
        &mut comparison,
        "phugoid",
        &result.phugoid,
        &case.modes["phugoid"],
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
    compare_mode(
        &mut comparison,
        "dutch_roll",
        &result.dutch_roll,
        &case.modes["dutch_roll"],
    );
    compare_mode(
        &mut comparison,
        "spiral",
        &result.spiral,
        &case.modes["spiral"],
    );
    comparison.finish();
}
