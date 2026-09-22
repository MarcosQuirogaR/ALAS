// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-geom::asb::airfoil` against AeroSandbox's `Airfoil`, via
//! `golden/generators/gen_geom_asb_airfoil.py`.
//!
//! Every case here is checked at `Tier::Linalg`, including the purely
//! closed-form NACA generation: `docs/PORTING.md` assigns this whole row
//! `linalg` rather than `closed` because the crate it belongs to
//! (`alas-geom`) depends on `alas-math::spline` for `repanel`, and that is
//! the tier the row names for the module as a whole, not chosen per case.

// This file is itself a test binary, so an unwrap or expect that fails is
// the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::HashMap;

use alas_geom::asb::airfoil::{naca_coordinates, Airfoil};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct NacaCase {
    name: String,
    n_points_per_side: usize,
    coordinates: Vec<(f64, f64)>,
}

#[derive(Debug, Deserialize)]
struct SurfacePair {
    upper: Vec<(f64, f64)>,
    lower: Vec<(f64, f64)>,
}

#[derive(Debug, Deserialize)]
struct ThicknessCase {
    x_over_c: Vec<f64>,
    local_thickness: Vec<f64>,
    max_thickness: f64,
}

#[derive(Debug, Deserialize)]
struct RepanelCase {
    source: String,
    n_points_per_side: usize,
    input: Vec<(f64, f64)>,
    output: Vec<(f64, f64)>,
}

#[derive(Debug, Deserialize)]
struct BlendCase {
    airfoil_a: String,
    airfoil_b: String,
    blend_fraction: f64,
    name: String,
    coordinates: Vec<(f64, f64)>,
}

#[derive(Debug, Deserialize)]
struct NormalizeCase {
    input: Vec<(f64, f64)>,
    coordinates: Vec<(f64, f64)>,
    x_translation: f64,
    y_translation: f64,
    scale_factor: f64,
    rotation_angle: f64,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    naca: HashMap<String, NacaCase>,
    surfaces: HashMap<String, SurfacePair>,
    thickness: HashMap<String, ThicknessCase>,
    repanel: HashMap<String, RepanelCase>,
    blends: HashMap<String, BlendCase>,
    normalize: HashMap<String, NormalizeCase>,
}

fn compare_points(
    comparison: &mut Comparison,
    label: &str,
    actual: &[(f64, f64)],
    expected: &[(f64, f64)],
) {
    if actual.len() != expected.len() {
        comparison.exact(
            &format!("{label} (point count)"),
            &actual.len(),
            &expected.len(),
        );
        return;
    }
    for (index, (&(ax, ay), &(ex, ey))) in actual.iter().zip(expected).enumerate() {
        comparison.scalar(&format!("{label}[{index}].x"), ax, ex);
        comparison.scalar(&format!("{label}[{index}].y"), ay, ey);
    }
}

#[test]
fn naca_coordinates_match_aerosandbox_for_every_case() {
    let fixture: Fixture = alas_testkit::load("geom", "asb_airfoil");

    let mut comparison =
        Comparison::new("alas-geom::asb::airfoil (naca coordinates)", Tier::Linalg);
    for (key, case) in &fixture.naca {
        let actual = naca_coordinates(&case.name, case.n_points_per_side).unwrap_or_else(|| {
            panic!(
                "{key}: expected {:?} to parse as a 4-digit NACA name",
                case.name
            )
        });
        compare_points(&mut comparison, key, &actual, &case.coordinates);
    }
    comparison.finish();
}

#[test]
fn upper_and_lower_coordinates_match_aerosandbox() {
    let fixture: Fixture = alas_testkit::load("geom", "asb_airfoil");

    let mut comparison = Comparison::new(
        "alas-geom::asb::airfoil (upper/lower surfaces)",
        Tier::Linalg,
    );
    for (name, expected) in &fixture.surfaces {
        let case = fixture
            .naca
            .get(&format!("{name}_200"))
            .unwrap_or_else(|| panic!("fixture is missing the {name}_200 naca case"));
        let airfoil = Airfoil::from_coordinates(name.clone(), case.coordinates.clone());

        compare_points(
            &mut comparison,
            &format!("{name}.upper"),
            airfoil.upper_coordinates(),
            &expected.upper,
        );
        compare_points(
            &mut comparison,
            &format!("{name}.lower"),
            airfoil.lower_coordinates(),
            &expected.lower,
        );
    }
    comparison.finish();
}

#[test]
fn local_and_max_thickness_match_aerosandbox() {
    let fixture: Fixture = alas_testkit::load("geom", "asb_airfoil");

    let mut comparison = Comparison::new("alas-geom::asb::airfoil (thickness)", Tier::Linalg);
    for (name, expected) in &fixture.thickness {
        let case = fixture
            .naca
            .get(&format!("{name}_200"))
            .unwrap_or_else(|| panic!("fixture is missing the {name}_200 naca case"));
        let airfoil = Airfoil::from_coordinates(name.clone(), case.coordinates.clone());

        let local = airfoil.local_thickness(&expected.x_over_c);
        comparison.slice(
            &format!("{name}.local_thickness"),
            &local,
            &expected.local_thickness,
        );

        // `np.linspace(0, 1, 101)`, `max_thickness`'s upstream default
        // sample grid: built directly here rather than through this
        // crate's internal `spacing::linspace`, which is private to `asb`
        // and not meant to be reached from outside the crate.
        let default_sample: Vec<f64> = (0..=100).map(|i| f64::from(i) / 100.0).collect();
        comparison.scalar(
            &format!("{name}.max_thickness"),
            airfoil.max_thickness(&default_sample),
            expected.max_thickness,
        );
    }
    comparison.finish();
}

#[test]
fn repanel_matches_aerosandbox() {
    let fixture: Fixture = alas_testkit::load("geom", "asb_airfoil");

    let mut comparison = Comparison::new("alas-geom::asb::airfoil (repanel)", Tier::Linalg);
    for (key, case) in &fixture.repanel {
        let source = Airfoil::from_coordinates(case.source.clone(), case.input.clone());
        let repaneled = source
            .repanel(case.n_points_per_side)
            .unwrap_or_else(|error| panic!("{key}: repanel failed: {error}"));

        compare_points(&mut comparison, key, &repaneled.coordinates, &case.output);
    }
    comparison.finish();
}

#[test]
fn blend_with_another_airfoil_matches_aerosandbox() {
    let fixture: Fixture = alas_testkit::load("geom", "asb_airfoil");

    let mut comparison = Comparison::new(
        "alas-geom::asb::airfoil (blend_with_another_airfoil)",
        Tier::Linalg,
    );
    for (key, case) in &fixture.blends {
        let airfoil_a = fixture
            .naca
            .get(&format!("{}_200", case.airfoil_a))
            .unwrap_or_else(|| panic!("fixture is missing the {}_200 naca case", case.airfoil_a));
        let airfoil_b = fixture
            .naca
            .get(&format!("{}_200", case.airfoil_b))
            .unwrap_or_else(|| panic!("fixture is missing the {}_200 naca case", case.airfoil_b));

        let a = Airfoil::from_coordinates(case.airfoil_a.clone(), airfoil_a.coordinates.clone());
        let b = Airfoil::from_coordinates(case.airfoil_b.clone(), airfoil_b.coordinates.clone());

        let blended = a
            .blend_with_another_airfoil(&b, case.blend_fraction, 100)
            .unwrap_or_else(|error| panic!("{key}: blend failed: {error}"));

        comparison.exact(&format!("{key}.name"), &blended.name, &case.name);
        compare_points(
            &mut comparison,
            key,
            &blended.coordinates,
            &case.coordinates,
        );
    }
    comparison.finish();
}

#[test]
fn normalize_matches_aerosandbox_in_both_the_section_and_the_transform() {
    // The four reported numbers matter as much as the moved section:
    // `alas-aero::neuralfoil` corrects its moment coefficient with the
    // translation, divides its Reynolds number by the scale and offsets its
    // angle of attack by the rotation. A port that moved the section
    // correctly and reported the rotation with the wrong sign would produce a
    // polar shifted by up to three degrees on the sections in this fixture.
    let fixture: Fixture = alas_testkit::load("geom", "asb_airfoil");
    let mut comparison = Comparison::new("alas-geom::asb::airfoil (normalize)", Tier::Linalg);

    for (name, case) in &fixture.normalize {
        let source = Airfoil::from_coordinates(name.clone(), case.input.clone());
        let normalized = source.normalize();
        comparison.scalar(
            &format!("{name}.x_translation"),
            normalized.x_translation,
            case.x_translation,
        );
        comparison.scalar(
            &format!("{name}.y_translation"),
            normalized.y_translation,
            case.y_translation,
        );
        comparison.scalar(
            &format!("{name}.scale_factor"),
            normalized.scale_factor,
            case.scale_factor,
        );
        comparison.scalar(
            &format!("{name}.rotation_angle_deg"),
            normalized.rotation_angle_deg,
            case.rotation_angle,
        );
        compare_points(
            &mut comparison,
            &format!("{name}.coordinates"),
            &normalized.airfoil.coordinates,
            &case.coordinates,
        );
    }
    comparison.finish();
}

#[test]
fn every_normalize_case_is_a_section_that_was_not_already_in_the_standard_frame() {
    // `gen_geom_asb_airfoil.py` refuses to write a fixture in which no case
    // moves each of the four numbers; this states the same property from the
    // side that would otherwise silently pass.
    let fixture: Fixture = alas_testkit::load("geom", "asb_airfoil");
    assert!(fixture
        .normalize
        .values()
        .any(|case| case.x_translation != 0.0));
    assert!(fixture
        .normalize
        .values()
        .any(|case| case.y_translation != 0.0));
    assert!(fixture
        .normalize
        .values()
        .any(|case| case.scale_factor != 1.0));
    assert!(fixture
        .normalize
        .values()
        .any(|case| case.rotation_angle != 0.0));
}

#[test]
fn the_fixture_names_the_sections_this_program_needs() {
    let fixture: Fixture = alas_testkit::load("geom", "asb_airfoil");
    // naca0012 is the one name this program's presets actually resolve
    // through the NACA fallback (docs/PORTING.md, Geometry); naca2412
    // exercises the cambered branch that a symmetric section cannot.
    assert!(fixture.naca.contains_key("naca0012_200"));
    assert!(fixture.naca.contains_key("naca2412_200"));
}
