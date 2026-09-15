// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-aero::lift_surrogate` against SUAVE's `Vortex_Lattice`
//! surrogate, via `golden/aero/lift_surrogate.json`.
//!
//! Three tiers, and the middle one is the point of the row. The training grid
//! and the fuselage correction are copied constants and are `exact`. The
//! sampled training tables come straight out of `alas-aero::vorlax` and carry
//! its `f32` tier, comparing them at anything tighter would be asserting
//! something about the single-precision kernel that its own row does not
//! claim. Everything the spline itself produces: the knot vectors, the
//! coefficients, and the evaluations, is `linalg`, the tier
//! `docs/PORTING.md` assigns this row and the tier
//! `alas-math::BicubicSpline`'s own row already carries, since the fit is a
//! pair of dense collocation solves.
//!
//! **The knots are compared before any value is.** `RectBivariateSpline` at
//! its defaults is Dierckx's `regrid` at zero smoothing, and infinitely many
//! bicubic surfaces pass through the same grid; a surface fitted with knots
//! somewhere else agrees at every training point and disagrees everywhere
//! between them. Four of the thirteen evaluations are outside the training
//! rectangle, on all four sides, because that is where FITPACK's clamp lives
//! and where a port that extrapolated instead would part company by an
//! unbounded amount.
//!
//! The vehicle is read out of `golden/aero/vorlax.json` rather than restated:
//! the two generators build the same one, and a second transcription is a
//! second thing that can drift.

// This file is itself a test binary, so an unwrap or expect that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod support;

use alas_aero::lift_surrogate::{
    aircraft_lift_coefficient, LiftSurrogate, TrainingGrid, FUSELAGE_LIFT_CORRECTION,
};
use alas_testkit::{Comparison, Tier};
use support::lift_surrogate::{Fixture, Spline};
use support::vorlax::Fixture as VorlaxFixture;

/// Train once. Eighty vortex-lattice solves on a 375-panel aircraft is not
/// expensive, but it is not free either, and four tests want the same object.
fn trained() -> (Fixture, LiftSurrogate) {
    let fixture: Fixture = alas_testkit::load("aero", "lift_surrogate");
    let vehicle: VorlaxFixture = alas_testkit::load("aero", "vorlax");
    let surrogate = LiftSurrogate::train(
        &vehicle.vlm_geometry(),
        &vehicle.vlm_settings(),
        &TrainingGrid::default(),
    )
    .expect("the fixture's vehicle trains");
    (fixture, surrogate)
}

/// The grid, and the two surrogates that have to be absent.
///
/// If either the supersonic or the transonic surface has become a surface,
/// `evaluate_surrogate` takes a three-way blended branch this row does not
/// translate, and every comparison below would be against a different model.
#[test]
fn the_training_grid_and_the_reached_branch_are_what_the_port_assumes() {
    let fixture: Fixture = alas_testkit::load("aero", "lift_surrogate");
    let grid = TrainingGrid::default();

    let mut c = Comparison::new("alas-aero::lift_surrogate grid", Tier::Exact);
    c.exact("training Mach numbers", &grid.mach, &fixture.training.mach);
    c.exact(
        "training angles of attack",
        &grid.angle_of_attack_rad,
        &fixture.training.angle_of_attack_rad,
    );
    c.exact(
        "fuselage_lift_correction",
        &FUSELAGE_LIFT_CORRECTION,
        &fixture.settings.fuselage_lift_correction,
    );
    c.exact(
        "the supersonic surrogate is absent",
        &fixture.settings.supersonic_surrogate_is_absent,
        &true,
    );
    c.exact(
        "the transonic surrogate is absent",
        &fixture.settings.transonic_surrogate_is_absent,
        &true,
    );
    c.exact(
        "the grid is subsonic throughout",
        &fixture.training.mach.iter().any(|&mach| mach >= 1.0),
        &false,
    );
    c.finish();
}

/// The sampled tables, which is `sample_training`'s whole output.
///
/// These come out of `alas-aero::vorlax` and are checked at its tier. A
/// transposed reshape would produce a table that is smooth and wrong, so the
/// comparison is per entry rather than on a summary.
#[test]
fn the_training_tables_match_sample_by_sample() {
    let (fixture, surrogate) = trained();
    let tables = surrogate.training();

    let mut c = Comparison::new("alas-aero::lift_surrogate training", Tier::F32);
    for (i, row) in fixture.training.lift_coefficient.iter().enumerate() {
        c.slice(&format!("CL[alpha={i}]"), &tables.lift_coefficient[i], row);
    }
    for (i, row) in fixture.training.drag_coefficient.iter().enumerate() {
        c.slice(&format!("CDi[alpha={i}]"), &tables.drag_coefficient[i], row);
    }
    for tag in &fixture.wing_tags {
        for (i, row) in fixture.training.wing_lift_coefficient[tag]
            .iter()
            .enumerate()
        {
            c.slice(
                &format!("{tag}/CL[alpha={i}]"),
                &tables.wing_lift_coefficient[tag][i],
                row,
            );
        }
        for (i, row) in fixture.training.wing_drag_coefficient[tag]
            .iter()
            .enumerate()
        {
            c.slice(
                &format!("{tag}/CDi[alpha={i}]"),
                &tables.wing_drag_coefficient[tag][i],
                row,
            );
        }
    }
    c.finish();
}

/// The eight fitted surfaces, as knots and coefficients.
#[test]
fn every_fitted_surface_has_the_knots_and_coefficients_the_reference_fitted() {
    let (fixture, surrogate) = trained();
    let mut c = Comparison::new("alas-aero::lift_surrogate surfaces", Tier::Linalg);

    let mut check = |name: &str, actual: &alas_math::BicubicSpline, want: &Spline| {
        let (knots_x, knots_y) = actual.knots();
        // The knot vectors are copied data points, not computed values, so a
        // difference in them is a different placement rule rather than a
        // different arithmetic, which is why they are checked exactly even
        // inside a `linalg` comparison.
        c.exact(&format!("{name}/knots_x"), &knots_x.to_vec(), &want.knots_x);
        c.exact(&format!("{name}/knots_y"), &knots_y.to_vec(), &want.knots_y);
        c.slice(
            &format!("{name}/coefficients"),
            actual.coefficients(),
            &want.coefficients,
        );
    };

    check(
        "lift_coefficient",
        surrogate.lift_surface(),
        &fixture.surrogates.lift_coefficient,
    );
    check(
        "drag_coefficient",
        surrogate.drag_surface(),
        &fixture.surrogates.drag_coefficient,
    );
    for (index, tag) in fixture.wing_tags.iter().enumerate() {
        check(
            &format!("{tag}/lift_coefficient"),
            surrogate.wing_lift_surface(index),
            &fixture.surrogates.wing_lift_coefficient[tag],
        );
        check(
            &format!("{tag}/drag_coefficient"),
            surrogate.wing_drag_surface(index),
            &fixture.surrogates.wing_drag_coefficient[tag],
        );
    }
    c.finish();
}

/// The evaluations, including the four outside the training rectangle.
#[test]
fn every_evaluation_matches_including_the_ones_that_clamp() {
    let (fixture, surrogate) = trained();
    let mut c = Comparison::new("alas-aero::lift_surrogate evaluations", Tier::Linalg);

    for case in &fixture.cases {
        let solution = surrogate.evaluate(case.angle_of_attack_deg.to_radians(), case.mach);
        let tag = &case.tag;
        c.scalar(
            &format!("{tag}/CL"),
            solution.inviscid_lift_coefficient,
            case.inviscid_lift_coefficient,
        );
        c.scalar(
            &format!("{tag}/CDi"),
            solution.inviscid_induced_drag_coefficient,
            case.inviscid_induced_drag_coefficient,
        );
        c.scalar(
            &format!("{tag}/aircraft CL"),
            aircraft_lift_coefficient(solution.inviscid_lift_coefficient, FUSELAGE_LIFT_CORRECTION),
            case.aircraft_lift_coefficient,
        );
        for (index, wing) in fixture.wing_tags.iter().enumerate() {
            c.scalar(
                &format!("{tag}/{wing}/CL"),
                solution.wing_lift_coefficient[index],
                case.wing_lift_coefficient[wing],
            );
            c.scalar(
                &format!("{tag}/{wing}/CDi"),
                solution.wing_induced_drag_coefficient[index],
                case.wing_induced_drag_coefficient[wing],
            );
        }
    }
    c.finish();
}
