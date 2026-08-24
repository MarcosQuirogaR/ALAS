// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-stab::modes` against AeroSandbox's
//! `dynamics.flight_dynamics.airplane.get_modes`, via
//! `golden/generators/gen_stab_modes.py`.
//!
//! # One tier, `closed`
//!
//! `get_modes` runs no solve of its own: it is closed-form arithmetic over a
//! stability-derivative set it takes as data. The fixture feeds *fixed*
//! derivative sets rather than ones from a vortex-lattice run, so nothing here
//! goes through a factorization and the two implementations evaluate the same
//! products in the same order. `docs/PORTING.md` names `closed` for this row,
//! and every eigenvalue and damping ratio is compared there. (The end-to-end
//! path that produces such derivatives from a real aircraft is
//! `alas-stab::dynamics`; its VLM-fed half carries the `alas-aero::asb_vlm`
//! solve and is a different question from this one.)
//!
//! # The two cases reach both branches of `get_mode_info`
//!
//! `b737` is a conventional aircraft whose short-period, phugoid and dutch
//! roll are oscillatory; `unstable` is statically and directionally unstable,
//! so its short-period and dutch roll degrade to the aperiodic branch (a zero
//! imaginary part, the characteristic root folded into the real part). The
//! generator refuses to write a fixture that reaches only one branch.

// This file is itself a test binary, so an unwrap or expect that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::HashMap;

use alas_aero::operating_point::OperatingPoint;
use alas_atmo::Atmosphere;
use alas_geom::asb::airplane::Airplane;
use alas_stab::modes::{self, MassProperties, Mode, StabilityAero};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct AirplaneFixture {
    s_ref: f64,
    c_ref: f64,
    b_ref: f64,
}

#[derive(Debug, Deserialize)]
struct OpFixture {
    altitude_m: f64,
    velocity: f64,
}

#[derive(Debug, Deserialize)]
struct MassFixture {
    mass: f64,
    ixx: f64,
    iyy: f64,
    izz: f64,
}

#[derive(Debug, Deserialize)]
struct AeroFixture {
    cl: f64,
    cd: f64,
    cma: f64,
    cmq: f64,
    clp: f64,
    cyb: f64,
    cnb: f64,
    cyr: f64,
    cnr: f64,
    clb: f64,
    clr: f64,
}

#[derive(Debug, Deserialize)]
struct ModeFixture {
    eigenvalue_real: f64,
    eigenvalue_imag: f64,
    damping_ratio: f64,
}

#[derive(Debug, Deserialize)]
struct ModesFixture {
    phugoid: ModeFixture,
    short_period: ModeFixture,
    roll_subsidence: ModeFixture,
    dutch_roll: ModeFixture,
    spiral: ModeFixture,
}

#[derive(Debug, Deserialize)]
struct Case {
    airplane: AirplaneFixture,
    op_point: OpFixture,
    mass: MassFixture,
    aero: AeroFixture,
    modes: ModesFixture,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    cases: HashMap<String, Case>,
}

/// An airplane carrying only the three reference dimensions `get_modes` reads;
/// the eigenmode formulas never touch its wings or fuselages.
fn airplane(fixture: &AirplaneFixture) -> Airplane {
    Airplane {
        name: "modes fixture".to_owned(),
        xyz_ref: [0.0, 0.0, 0.0],
        wings: Vec::new(),
        fuselages: Vec::new(),
        s_ref: fixture.s_ref,
        c_ref: fixture.c_ref,
        b_ref: fixture.b_ref,
    }
}

fn op_point(fixture: &OpFixture) -> OperatingPoint {
    // alpha/beta and the three rates are irrelevant here: get_modes reads only
    // the dynamic pressure, airspeed and density off the operating point.
    OperatingPoint::new(
        Atmosphere::new(fixture.altitude_m),
        fixture.velocity,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
    )
}

fn mass(fixture: &MassFixture) -> MassProperties {
    MassProperties {
        mass: fixture.mass,
        ixx: fixture.ixx,
        iyy: fixture.iyy,
        izz: fixture.izz,
    }
}

fn aero(fixture: &AeroFixture) -> StabilityAero {
    StabilityAero {
        cl: fixture.cl,
        cd: fixture.cd,
        cma: fixture.cma,
        cmq: fixture.cmq,
        clp: fixture.clp,
        cyb: fixture.cyb,
        cnb: fixture.cnb,
        cyr: fixture.cyr,
        cnr: fixture.cnr,
        clb: fixture.clb,
        clr: fixture.clr,
    }
}

fn compare_mode(comparison: &mut Comparison, name: &str, actual: &Mode, expected: &ModeFixture) {
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
}

#[test]
fn get_modes_matches_aerosandbox_for_every_case() {
    let fixture: Fixture = alas_testkit::load("stab", "modes");
    let mut names: Vec<&String> = fixture.cases.keys().collect();
    names.sort();

    let mut comparison = Comparison::new("alas-stab::modes", Tier::Closed);
    for name in names {
        let case = &fixture.cases[name];
        let result = modes::get_modes(
            &airplane(&case.airplane),
            &op_point(&case.op_point),
            &mass(&case.mass),
            &aero(&case.aero),
        );
        compare_mode(
            &mut comparison,
            &format!("{name}.phugoid"),
            &result.phugoid,
            &case.modes.phugoid,
        );
        compare_mode(
            &mut comparison,
            &format!("{name}.short_period"),
            &result.short_period,
            &case.modes.short_period,
        );
        compare_mode(
            &mut comparison,
            &format!("{name}.roll_subsidence"),
            &result.roll_subsidence,
            &case.modes.roll_subsidence,
        );
        compare_mode(
            &mut comparison,
            &format!("{name}.dutch_roll"),
            &result.dutch_roll,
            &case.modes.dutch_roll,
        );
        compare_mode(
            &mut comparison,
            &format!("{name}.spiral"),
            &result.spiral,
            &case.modes.spiral,
        );
    }
    comparison.finish();
}
