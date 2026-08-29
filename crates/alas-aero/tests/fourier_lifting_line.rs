// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Independent checks of the geometry-resolving Fourier lifting-line model.
//!
//! The primary fixture is not a frozen output of another implementation. It
//! is the exact elliptic-wing solution of the same published lifting-line
//! equations: only `A_1` is nonzero, `e = 1`, and both lift and induced drag
//! have closed forms. The implementation still passes through a pivoted dense
//! solve, so comparisons use `Tier::Linalg`.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::f64::consts::PI;

use alas_aero::fourier_lifting_line::{
    thin_airfoil_zero_lift_angle_rad, AircraftFourierLiftingLine, FourierLiftingLineError,
    FourierLiftingLineSurface, LiftingLineSection,
};
use alas_geom::asb::airfoil::Airfoil;
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Expected {
    lift_curve_slope_per_rad: f64,
    lift_coefficient: f64,
    induced_drag_coefficient: f64,
    first_fourier_coefficient: f64,
    span_efficiency: f64,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    span_m: f64,
    area_m2: f64,
    section_lift_slope_per_rad: f64,
    alpha_deg: f64,
    harmonic_count: usize,
    expected: Expected,
    naca2412_zero_lift_angle_rad: f64,
}

fn elliptic_surface(fixture: &Fixture) -> FourierLiftingLineSurface {
    let center_chord_m = 4.0 * fixture.area_m2 / (PI * fixture.span_m);
    let mut sections = Vec::with_capacity(fixture.harmonic_count + 1);
    for row in (0..fixture.harmonic_count).rev() {
        let theta = (row + 1) as f64 * PI / (2.0 * fixture.harmonic_count as f64);
        let span_fraction = if row + 1 == fixture.harmonic_count {
            0.0
        } else {
            theta.cos()
        };
        sections.push(LiftingLineSection {
            span_fraction,
            chord_m: center_chord_m * theta.sin(),
            twist_rad: 0.0,
            zero_lift_angle_rad: 0.0,
        });
    }
    sections.push(LiftingLineSection {
        span_fraction: 1.0,
        chord_m: 0.0,
        twist_rad: 0.0,
        zero_lift_angle_rad: 0.0,
    });
    FourierLiftingLineSurface {
        name: "Exact elliptic wing".to_owned(),
        span_m: fixture.span_m,
        area_m2: fixture.area_m2,
        section_lift_slope_per_rad: fixture.section_lift_slope_per_rad,
        harmonic_count: fixture.harmonic_count,
        sections,
    }
}

fn trapezoidal_surface(harmonic_count: usize, tip_twist_deg: f64) -> FourierLiftingLineSurface {
    FourierLiftingLineSurface {
        name: "Tapered wing".to_owned(),
        span_m: 10.0,
        area_m2: 12.0,
        section_lift_slope_per_rad: 2.0 * PI,
        harmonic_count,
        sections: vec![
            LiftingLineSection {
                span_fraction: 0.0,
                chord_m: 1.6,
                twist_rad: 0.0,
                zero_lift_angle_rad: 0.0,
            },
            LiftingLineSection {
                span_fraction: 1.0,
                chord_m: 0.8,
                twist_rad: tip_twist_deg.to_radians(),
                zero_lift_angle_rad: 0.0,
            },
        ],
    }
}

#[test]
fn elliptic_wing_matches_the_exact_lift_and_induced_drag_solution() {
    let fixture: Fixture = alas_testkit::load("aero", "fourier_lifting_line");
    let surface = elliptic_surface(&fixture);
    let alpha_rad = fixture.alpha_deg.to_radians();
    let result = surface
        .solve(alpha_rad)
        .expect("elliptic system is nonsingular");
    let slope = result.lift_coefficient / alpha_rad;
    let efficiency = result
        .span_efficiency
        .expect("positive lift has efficiency");

    let mut comparison = Comparison::new("exact elliptic lifting-line solution", Tier::Linalg);
    comparison
        .scalar(
            "lift_curve_slope_per_rad",
            slope,
            fixture.expected.lift_curve_slope_per_rad,
        )
        .scalar(
            "lift_coefficient",
            result.lift_coefficient,
            fixture.expected.lift_coefficient,
        )
        .scalar(
            "induced_drag_coefficient",
            result.induced_drag_coefficient,
            fixture.expected.induced_drag_coefficient,
        )
        .scalar(
            "first_fourier_coefficient",
            result.fourier_coefficients[0],
            fixture.expected.first_fourier_coefficient,
        )
        .scalar(
            "span_efficiency",
            efficiency,
            fixture.expected.span_efficiency,
        );
    comparison.finish();

    for (index, coefficient) in result.fourier_coefficients.iter().enumerate().skip(1) {
        assert!(
            coefficient.abs() < 1.0e-12,
            "elliptic loading must not excite A_{}: {coefficient:e}",
            2 * index + 1
        );
    }
}

#[test]
fn airfoil_camber_recovers_the_analytic_naca2412_zero_lift_angle() {
    let fixture: Fixture = alas_testkit::load("aero", "fourier_lifting_line");
    let airfoil = Airfoil::from_name("naca2412").expect("NACA 2412 is analytical");
    let actual =
        thin_airfoil_zero_lift_angle_rad(&airfoil).expect("NACA 2412 has a finite camber line");
    assert!(
        (actual - fixture.naca2412_zero_lift_angle_rad).abs() < 2.0e-6,
        "computed {actual:.12e}, analytic {:.12e}",
        fixture.naca2412_zero_lift_angle_rad
    );

    let symmetric = Airfoil::from_name("naca0012").expect("NACA 0012 is analytical");
    let symmetric_zero =
        thin_airfoil_zero_lift_angle_rad(&symmetric).expect("NACA 0012 has a finite camber line");
    assert!(symmetric_zero.abs() < 1.0e-14);
}

#[test]
fn fourier_refinement_converges_on_a_tapered_wing() {
    let alpha = 5.0_f64.to_radians();
    let coarse = trapezoidal_surface(4, -2.0)
        .solve(alpha)
        .expect("coarse solve");
    let medium = trapezoidal_surface(8, -2.0)
        .solve(alpha)
        .expect("medium solve");
    let fine = trapezoidal_surface(16, -2.0)
        .solve(alpha)
        .expect("fine solve");
    let coarse_error = (coarse.lift_coefficient - fine.lift_coefficient).abs();
    let medium_error = (medium.lift_coefficient - fine.lift_coefficient).abs();
    assert!(medium_error < coarse_error * 0.4);
}

#[test]
fn washout_reduces_outboard_loading_and_total_lift() {
    let alpha = 5.0_f64.to_radians();
    let untwisted = trapezoidal_surface(12, 0.0)
        .solve(alpha)
        .expect("untwisted solve");
    let washed_out = trapezoidal_surface(12, -3.0)
        .solve(alpha)
        .expect("washout solve");
    assert!(washed_out.lift_coefficient < untwisted.lift_coefficient);
    assert!(
        washed_out.stations[0].section_lift_coefficient
            < untwisted.stations[0].section_lift_coefficient
    );
}

#[test]
fn reference_area_conversion_is_explicit_and_force_conserving() {
    let surface = trapezoidal_surface(8, 0.0);
    let model = AircraftFourierLiftingLine {
        reference_area_m2: 24.0,
        surfaces: vec![surface.clone()],
    };
    let alpha = 4.0_f64.to_radians();
    let surface_result = surface.solve(alpha).expect("surface solve");
    let aircraft_result = model.solve(alpha).expect("aircraft solve");
    assert!(
        (aircraft_result.lift_coefficient - 0.5 * surface_result.lift_coefficient).abs() < 1e-13
    );
    assert!(
        (aircraft_result.induced_drag_coefficient - 0.5 * surface_result.induced_drag_coefficient)
            .abs()
            < 1e-13
    );
}

#[test]
fn invalid_geometry_is_rejected_instead_of_coerced() {
    let mut surface = trapezoidal_surface(8, 0.0);
    surface.sections[1].span_fraction = 0.5;
    assert_eq!(
        surface.solve(0.0),
        Err(FourierLiftingLineError::InvalidSectionOrder)
    );
    surface = trapezoidal_surface(0, 0.0);
    assert_eq!(
        surface.solve(0.0),
        Err(FourierLiftingLineError::EmptyHarmonicSet)
    );
}
