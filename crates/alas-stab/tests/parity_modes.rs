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
//!
//! # The dutch roll is compared through its natural frequency only
//!
//! `alas-stab::modes` takes the dutch roll as the root pair of the
//! sideslip/yaw-rate quadratic, which upstream's real part (yaw-damping term
//! not halved) and imaginary part (the undamped frequency) are not: see the
//! module doc there. What the two still share is the quadratic's constant
//! term: upstream's imaginary part is `omega_n`, and the modulus of this
//! implementation's complex root is the same `omega_n`. That is compared when
//! the reference mode is oscillatory; the roots themselves are pinned by the
//! hand cases in the module's unit tests.

// This file is itself a test binary, so an unwrap or expect that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::HashMap;

use alas_aero::operating_point::OperatingPoint;
use alas_atmo::{Atmosphere, GAS_CONSTANT_AIR, STANDARD_GRAVITY};
use alas_geom::asb::airplane::Airplane;
use alas_stab::modes::{self, MassProperties, Mode, StabilityAero};
use alas_testkit::reference_ambient::try_reference_ambient;
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

/// The reference's gravity in the phugoid term, m/s^2: the two-decimal
/// default of its `get_modes`.
const REFERENCE_GRAVITY: f64 = 9.81;

/// The density the reference's fitted atmosphere returned for this case, and
/// whether it was read from a recorded state.
///
/// The fixture was generated with that fitted atmosphere, which differs from
/// the U.S. Standard Atmosphere, 1976 the product evaluates by 1.5 % in
/// temperature at 11 km and up to 1.7 % in density. `get_modes` reads the
/// ambient state only through the density (the airspeed is an input), so the
/// density is the quantity to restore.
///
/// - At an altitude the atmosphere fixture records (`golden/atmo/
///   differentiable.json`: the `unstable` case at 8 000 m) the recorded
///   density is used, and every mode is compared with the reference.
/// - The `b737` case sits at 8 000 ft = 2 438.4 m, between the recorded rows.
///   Its density follows from the reference's own roll-subsidence root,
///   `sigma_roll = rho u0 S b^2 C_lp / (4 I_xx)` (Caughey's roll-subsidence
///   approximation, the relation `get_modes` evaluates). That root then
///   defines the density instead of being checked against it, so the case
///   compares the other four modes with the reference and the roll root only
///   by consistency; the density it yields is required to lie within 2 % of
///   the 1976 value as a sanity check.
fn reference_density(case: &Case) -> (f64, bool) {
    if let Some(ambient) = try_reference_ambient(case.op_point.altitude_m) {
        return (ambient.density_kg_m3, true);
    }
    let b = case.airplane.b_ref;
    let rho = 4.0 * case.mass.ixx * case.modes.roll_subsidence.eigenvalue_real
        / (case.op_point.velocity * case.airplane.s_ref * b * b * case.aero.clp);
    let standard = Atmosphere::new(case.op_point.altitude_m).density();
    assert!(
        (rho / standard - 1.0).abs() < 0.02,
        "the density implied by the reference roll root, {rho}, is not within 2 % of {standard}"
    );
    (rho, false)
}

/// The operating point of a case at the reference's density: the standard
/// atmosphere at the altitude, with the temperature deviation that gives that
/// density at the pressure of the level.
fn op_point(case: &Case, density: f64) -> OperatingPoint {
    // alpha/beta and the three rates are irrelevant here: get_modes reads only
    // the dynamic pressure, airspeed and density off the operating point.
    let level = Atmosphere::new(case.op_point.altitude_m);
    let temperature = level.pressure() / (GAS_CONSTANT_AIR * density);
    OperatingPoint::new(
        level.with_temperature_deviation(temperature - level.temperature()),
        case.op_point.velocity,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
    )
}

/// The reference's phugoid, restated at the product's gravity.
///
/// The phugoid is `sigma +- i omega` with `sigma = x_u / 2` and
/// `omega^2 = -sigma^2 - g z_u / u0`; the product reads the standard gravity
/// 9.80665 m/s^2 where the reference's `get_modes` reads 9.81. Gravity enters
/// only through the squared natural frequency `sigma^2 + omega^2`, so the
/// reference mode at the product's gravity follows exactly from the recorded
/// real part and natural frequency: `wn' = wn sqrt(g / 9.81)`,
/// `omega' = sqrt(wn'^2 - sigma^2)`, `zeta' = -sigma / wn'`. The factor is
/// 0.99983. An aperiodic phugoid (zero imaginary part) is not restated.
fn phugoid_at_product_gravity(mode: &ModeFixture) -> ModeFixture {
    if mode.eigenvalue_imag <= 0.0 {
        return ModeFixture {
            eigenvalue_real: mode.eigenvalue_real,
            eigenvalue_imag: mode.eigenvalue_imag,
            damping_ratio: mode.damping_ratio,
        };
    }
    let sigma = mode.eigenvalue_real;
    let wn_squared =
        (sigma.powi(2) + mode.eigenvalue_imag.powi(2)) * (STANDARD_GRAVITY / REFERENCE_GRAVITY);
    ModeFixture {
        eigenvalue_real: sigma,
        eigenvalue_imag: (wn_squared - sigma.powi(2)).sqrt(),
        damping_ratio: -sigma / wn_squared.sqrt(),
    }
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
        let (density, recorded) = reference_density(case);
        let result = modes::get_modes(
            &airplane(&case.airplane),
            &op_point(case, density),
            &mass(&case.mass),
            &aero(&case.aero),
        );
        compare_mode(
            &mut comparison,
            &format!("{name}.phugoid"),
            &result.phugoid,
            &phugoid_at_product_gravity(&case.modes.phugoid),
        );
        compare_mode(
            &mut comparison,
            &format!("{name}.short_period"),
            &result.short_period,
            &case.modes.short_period,
        );
        if recorded {
            compare_mode(
                &mut comparison,
                &format!("{name}.roll_subsidence"),
                &result.roll_subsidence,
                &case.modes.roll_subsidence,
            );
        }
        if case.modes.dutch_roll.eigenvalue_imag > 0.0 {
            comparison.scalar(
                &format!("{name}.dutch_roll.natural_frequency"),
                result
                    .dutch_roll
                    .eigenvalue_real
                    .hypot(result.dutch_roll.eigenvalue_imag),
                case.modes.dutch_roll.eigenvalue_imag,
            );
        }
        compare_mode(
            &mut comparison,
            &format!("{name}.spiral"),
            &result.spiral,
            &case.modes.spiral,
        );
    }
    comparison.finish();
}
