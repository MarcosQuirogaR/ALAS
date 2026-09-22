// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Numerical verification and contract tests of the wing-only entry.
//!
//! The analytic cases are numerical verification of the implementation
//! against closed-form lifting-line results for the same idealization
//! (inviscid, incompressible, flat-plate sections). They are not a physical
//! validation of the model against measured wing data.

// A fixture that cannot be built is a broken test, not a library failure, so
// these unwraps report where the fixture stopped being valid.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::f64::consts::PI;

use alas_config::{DesignVector, GeometryConfig};
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::wing::{Wing, WingXSec};

use super::*;

/// Semispan stations of the verification wings.
const STATIONS: usize = 40;

/// Never place a station exactly at the elliptic tip: the chord there is zero
/// and the lattice would see a degenerate panel.
const TIP_FRACTION: f64 = 0.995;

/// An elliptic planform with a straight quarter-chord line, symmetric
/// sections and no twist: the geometry classical lifting-line theory solves
/// in closed form.
fn elliptic_wing(root_chord_m: f64, span_m: f64) -> Wing {
    let semispan = 0.5 * span_m;
    let section = Airfoil::from_name("naca0012").expect("library NACA section");
    let xsecs = (0..=STATIONS)
        .map(|index| {
            let fraction = TIP_FRACTION * (PI * 0.5 * index as f64 / STATIONS as f64).sin();
            let y = semispan * fraction;
            let chord = root_chord_m * (1.0 - fraction * fraction).max(0.0).sqrt();
            WingXSec::new(
                [0.25 * (root_chord_m - chord), y, 0.0],
                chord,
                0.0,
                section.clone(),
            )
        })
        .collect();
    Wing::new("Main Wing", xsecs, true)
}

/// A constant-chord wing with the same station count and span.
fn rectangular_wing(chord_m: f64, span_m: f64) -> Wing {
    let semispan = 0.5 * span_m;
    let section = Airfoil::from_name("naca0012").expect("library NACA section");
    let xsecs = (0..=STATIONS)
        .map(|index| {
            let y = semispan * (PI * 0.5 * index as f64 / STATIONS as f64).sin();
            WingXSec::new([0.0, y, 0.0], chord_m, 0.0, section.clone())
        })
        .collect();
    Wing::new("Main Wing", xsecs, true)
}

fn never_cancelled() -> impl Fn() -> bool {
    || false
}

/// Sea level, incompressible speed, two-degree sweep around zero.
fn probe_inputs() -> WingAnalysisInputs {
    WingAnalysisInputs {
        surfaces: SurfaceSet::WingOnly,
        condition: FlightCondition {
            altitude_m: 0.0,
            speed: SpeedInput::TrueAirspeed(60.0),
            attitude: AttitudeInput::AngleOfAttack(4.0),
        },
        moment_reference_m: [0.0, 0.0, 0.0],
        spanwise_resolution: 1,
        chordwise_resolution: 8,
        sweep: AlphaSweep {
            min_deg: 0.0,
            max_deg: 4.0,
            points: 3,
        },
    }
}

/// Classical elliptic lifting-line slope for a flat-plate section lift slope
/// of `2 pi` per radian: `a = a0 / (1 + a0 / (pi AR))`.
fn lifting_line_slope_per_rad(aspect_ratio: f64) -> f64 {
    2.0 * PI / (1.0 + 2.0 / aspect_ratio)
}

/// The lift-curve slope the outcome's own sweep measures, per radian.
fn measured_slope_per_rad(outcome: &WingAnalysisOutcome) -> f64 {
    let first = outcome.sweep.first().expect("a swept point");
    let last = outcome.sweep.last().expect("a swept point");
    (last.cl - first.cl) / (last.alpha_deg - first.alpha_deg).to_radians()
}

#[test]
fn an_elliptic_wing_matches_the_lifting_line_slope_and_the_elliptic_induced_drag() {
    let model = WingModel::from_surfaces(
        "verification",
        vec![elliptic_wing(2.0, 20.0)],
        [0.5, 0.0, 0.0],
    )
    .expect("a well-formed elliptic wing");
    let aspect_ratio = model.reference().aspect_ratio();
    let outcome = analyse(&model, &probe_inputs(), &never_cancelled()).expect("a solved wing");

    let expected_slope = lifting_line_slope_per_rad(aspect_ratio);
    let slope_error = (measured_slope_per_rad(&outcome) - expected_slope).abs() / expected_slope;
    assert!(
        slope_error < 0.05,
        "AR {aspect_ratio:.4}: lattice slope {:.4}/rad against lifting line {expected_slope:.4}/rad ({:.2}% apart)",
        measured_slope_per_rad(&outcome),
        slope_error * 100.0
    );

    let efficiency = outcome.span_efficiency.expect("a lifting point has one");
    assert!(
        (efficiency - 1.0).abs() < 0.05,
        "elliptic span efficiency {efficiency:.4} should sit at the ideal 1.0"
    );
    let expected_cdi = outcome.cl * outcome.cl / (PI * aspect_ratio);
    let drag_error = (outcome.cd_induced - expected_cdi).abs() / expected_cdi;
    assert!(
        drag_error < 0.05,
        "CDi {:.6} against the elliptic {expected_cdi:.6} ({:.2}% apart)",
        outcome.cd_induced,
        drag_error * 100.0
    );
}

#[test]
fn a_rectangular_wing_is_less_span_efficient_than_the_elliptic_ideal() {
    let elliptic = WingModel::from_surfaces("e", vec![elliptic_wing(2.0, 20.0)], [0.5, 0.0, 0.0])
        .expect("elliptic");
    let rectangular =
        WingModel::from_surfaces("r", vec![rectangular_wing(1.6, 20.0)], [0.4, 0.0, 0.0])
            .expect("rectangular");
    let inputs = probe_inputs();
    let elliptic = analyse(&elliptic, &inputs, &never_cancelled()).expect("elliptic solve");
    let rectangular =
        analyse(&rectangular, &inputs, &never_cancelled()).expect("rectangular solve");

    let elliptic_e = elliptic.span_efficiency.expect("elliptic efficiency");
    let rectangular_e = rectangular.span_efficiency.expect("rectangular efficiency");
    assert!(
        rectangular_e < elliptic_e,
        "rectangular {rectangular_e:.4} should trail elliptic {elliptic_e:.4}"
    );
    assert!(
        (0.85..1.0).contains(&rectangular_e),
        "rectangular span efficiency {rectangular_e:.4} left the lifting-line range"
    );
}

#[test]
fn the_span_load_covers_both_halves_and_integrates_to_the_solved_lift() {
    let model = WingModel::from_surfaces("e", vec![elliptic_wing(2.0, 20.0)], [0.5, 0.0, 0.0])
        .expect("elliptic");
    let outcome = analyse(&model, &probe_inputs(), &never_cancelled()).expect("a solved wing");

    assert_eq!(outcome.span_load.len(), 2 * STATIONS);
    let left = outcome.span_load.first().expect("a station");
    let right = outcome.span_load.last().expect("a station");
    assert!(left.y_m < 0.0 && right.y_m > 0.0);
    assert!((left.lift_per_span_n_m - right.lift_per_span_n_m).abs() < 1.0e-6);

    let mut integrated = 0.0;
    for window in outcome.span_load.windows(2) {
        let width = window[1].y_m - window[0].y_m;
        integrated += 0.5 * (window[0].lift_per_span_n_m + window[1].lift_per_span_n_m) * width;
    }
    let error = (integrated - outcome.lift_n).abs() / outcome.lift_n.abs();
    assert!(
        error < 0.02,
        "trapezoidal span integral {integrated:.1} N against the solved {:.1} N",
        outcome.lift_n
    );
}

#[test]
fn a_lift_target_reports_the_angle_that_reaches_it() {
    let model = WingModel::from_surfaces("e", vec![elliptic_wing(2.0, 20.0)], [0.5, 0.0, 0.0])
        .expect("elliptic");
    let mut inputs = probe_inputs();
    inputs.condition.attitude = AttitudeInput::LiftCoefficient(0.5);
    let outcome = analyse(&model, &inputs, &never_cancelled()).expect("a solved wing");

    assert!(outcome.alpha_from_lift_target);
    assert!(
        (outcome.cl - 0.5).abs() < 1.0e-5,
        "reported CL {:.6} missed the 0.5 target",
        outcome.cl
    );
    assert!(outcome.condition.alpha_deg.abs() <= ALPHA_LIMIT_DEG);
}

#[test]
fn an_unreachable_lift_target_is_refused_rather_than_extrapolated() {
    let model = WingModel::from_surfaces("e", vec![elliptic_wing(2.0, 20.0)], [0.5, 0.0, 0.0])
        .expect("elliptic");
    let mut inputs = probe_inputs();
    inputs.condition.attitude = AttitudeInput::LiftCoefficient(2.4);

    assert!(matches!(
        analyse(&model, &inputs, &never_cancelled()),
        Err(WingAnalysisError::UnreachableLift { .. })
    ));
}

#[test]
fn a_cancelled_run_returns_no_outcome() {
    let model = WingModel::from_surfaces("e", vec![elliptic_wing(2.0, 20.0)], [0.5, 0.0, 0.0])
        .expect("elliptic");

    assert!(matches!(
        analyse(&model, &probe_inputs(), &|| true),
        Err(WingAnalysisError::Cancelled)
    ));
}

#[test]
fn invalid_inputs_are_refused_before_any_solve() {
    let model = WingModel::from_surfaces("e", vec![elliptic_wing(2.0, 20.0)], [0.5, 0.0, 0.0])
        .expect("elliptic");
    let mut inputs = probe_inputs();
    inputs.condition.attitude = AttitudeInput::AngleOfAttack(45.0);
    inputs.chordwise_resolution = 0;

    let findings = inputs.validate();
    assert_eq!(findings.len(), 2);
    assert!(matches!(
        analyse(&model, &inputs, &never_cancelled()),
        Err(WingAnalysisError::InvalidInputs(_))
    ));
}

#[test]
fn moving_the_moment_reference_aft_shifts_the_pitching_moment_by_the_lift_arm() {
    let wing = elliptic_wing(2.0, 20.0);
    let forward =
        WingModel::from_surfaces("f", vec![wing.clone()], [0.5, 0.0, 0.0]).expect("model");
    let aft = WingModel::from_surfaces("a", vec![wing], [1.5, 0.0, 0.0]).expect("model");
    let mut inputs = probe_inputs();
    inputs.moment_reference_m = [0.5, 0.0, 0.0];
    let forward = analyse(&forward, &inputs, &never_cancelled()).expect("forward reference");
    inputs.moment_reference_m = [1.5, 0.0, 0.0];
    let aft = analyse(&aft, &inputs, &never_cancelled()).expect("aft reference");

    // Geometry x is positive aft and the pitching moment is positive nose-up,
    // so moving the reference one metre aft adds `+L * dx` to the moment.
    let arm = 1.0;
    let expected = forward.pitch_moment_n_m + forward.lift_n * arm;
    let error = (aft.pitch_moment_n_m - expected).abs() / forward.lift_n.abs().max(1.0);
    assert!(
        error < 0.02,
        "aft-reference moment {:.1} N m against the transported {expected:.1} N m",
        aft.pitch_moment_n_m
    );
}

#[test]
fn the_wing_only_model_carries_one_surface_and_no_fuselage() {
    let model = build_wing_model(
        &GeometryConfig::default(),
        &DesignVector::default(),
        SurfaceSet::WingOnly,
        None,
    )
    .expect("the default geometry lofts");

    assert_eq!(model.surface_names(), vec!["Main Wing".to_owned()]);
    assert!(model.airplane().fuselages.is_empty());
    assert!(model.reference().area_m2 > 0.0);
    assert!(model.reference().span_m > 0.0);
    assert!(model.reference().chord_m > 0.0);
}

#[test]
fn the_empennage_option_adds_the_two_tail_surfaces_without_changing_the_reference() {
    let geometry = GeometryConfig::default();
    let design = DesignVector::default();
    let wing = build_wing_model(&geometry, &design, SurfaceSet::WingOnly, None).expect("wing");
    let tailed = build_wing_model(&geometry, &design, SurfaceSet::WingAndEmpennage, None)
        .expect("wing and empennage");

    assert_eq!(
        tailed.surface_names(),
        vec![
            "Main Wing".to_owned(),
            "Horizontal Stabilizer".to_owned(),
            "Vertical Stabilizer".to_owned()
        ]
    );
    assert!(tailed.airplane().fuselages.is_empty());
    assert_eq!(wing.reference().area_m2, tailed.reference().area_m2);
    assert_eq!(wing.reference().span_m, tailed.reference().span_m);
    assert_eq!(wing.reference().chord_m, tailed.reference().chord_m);
}

#[test]
fn a_product_wing_runs_from_geometry_alone_and_the_empennage_adds_static_stability() {
    let geometry = GeometryConfig::default();
    let design = DesignVector::default();
    let wing = build_wing_model(&geometry, &design, SurfaceSet::WingOnly, None).expect("wing");
    let mut inputs = WingAnalysisInputs {
        moment_reference_m: wing.reference().moment_reference_m,
        sweep: AlphaSweep {
            min_deg: 0.0,
            max_deg: 2.0,
            points: 2,
        },
        ..WingAnalysisInputs::default()
    };
    let wing_only = analyse(&wing, &inputs, &never_cancelled()).expect("wing-only solve");
    assert!(wing_only.cl > 0.0, "a positive angle must lift");
    assert!(wing_only.cd_induced > 0.0, "lift implies induced drag");
    assert!(wing_only.stability.is_none(), "no empennage, no stability");
    assert!(wing_only.diagnostics.panel_count > 0);

    inputs.surfaces = SurfaceSet::WingAndEmpennage;
    let tailed = build_wing_model(&geometry, &design, SurfaceSet::WingAndEmpennage, None)
        .expect("wing and empennage");
    let with_tail = analyse(&tailed, &inputs, &never_cancelled()).expect("tailed solve");
    let stability = with_tail
        .stability
        .expect("the empennage enables stability");

    assert_eq!(with_tail.modelled_surfaces.len(), 3);
    assert!(stability.cl_alpha_per_rad > 0.0);
    assert!(
        (stability.static_margin + stability.cm_alpha_per_rad / stability.cl_alpha_per_rad).abs()
            < 1.0e-9,
        "the static margin must be -Cma/CLa"
    );
    assert!(
        stability.cm_alpha_per_rad < wing_only.sweep_moment_slope(),
        "adding a tail must make the moment slope more nose-down"
    );
}

impl WingAnalysisOutcome {
    /// The pitching-moment slope the outcome's own sweep measures, per
    /// radian; used only by the tests above.
    fn sweep_moment_slope(&self) -> f64 {
        let first = self.sweep.first().expect("a swept point");
        let last = self.sweep.last().expect("a swept point");
        (last.cm_pitch - first.cm_pitch) / (last.alpha_deg - first.alpha_deg).to_radians()
    }
}
