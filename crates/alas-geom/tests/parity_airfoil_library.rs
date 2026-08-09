// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-geom::airfoil_library` against `alas.geometry.airfoils`,
//! via `golden/generators/gen_geom_airfoil_library.py`.
//!
//! Every case here is checked at `Tier::Linalg`: `docs/PORTING.md` assigns
//! this row `linalg` because it depends on `asb::airfoil::repanel`, which
//! goes through `alas-math::spline` -- the tier applies to the module as a
//! whole, not chosen per case.

// This file is itself a test binary, so an unwrap or expect that fails is
// the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::HashMap;

use alas_config::DesignVector;
use alas_geom::airfoil_library::{
    apply_bumps, build_section, morph_airfoil, normalize_coordinates, AirfoilLibrary,
};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct GetCase {
    requested_name: String,
    resolved_name: String,
    coordinates: Vec<(f64, f64)>,
}

#[derive(Debug, Deserialize)]
struct NormalizeCase {
    input: Vec<(f64, f64)>,
    output: Vec<(f64, f64)>,
}

#[derive(Debug, Deserialize)]
struct ApplyBumpsCase {
    base: String,
    bumps_upper: [f64; 2],
    bumps_lower: [f64; 2],
    n_points_per_side: usize,
    coordinates: Vec<(f64, f64)>,
}

#[derive(Debug, Deserialize)]
struct MorphCase {
    base: String,
    thickness_scale: f64,
    camber_scale: f64,
    n_points: usize,
    coordinates: Vec<(f64, f64)>,
}

#[derive(Debug, Deserialize)]
struct BuildSectionDesignVector {
    bump_upper_front: f64,
    bump_upper_rear: f64,
    bump_lower_mid: f64,
    bump_lower_rear: f64,
    airfoil_thickness_scale: f64,
    airfoil_camber_scale: f64,
}

#[derive(Debug, Deserialize)]
struct BuildSectionCase {
    base: String,
    design_vector: BuildSectionDesignVector,
    coordinates: Vec<(f64, f64)>,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    get: HashMap<String, GetCase>,
    normalize_coordinates: HashMap<String, NormalizeCase>,
    apply_bumps: HashMap<String, ApplyBumpsCase>,
    morph_airfoil: HashMap<String, MorphCase>,
    build_section: BuildSectionCase,
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

/// Resolve `name` through `AirfoilLibrary::get`, failing loudly (not
/// silently skipping the case) if it does not resolve -- every base name the
/// fixture names is one this crate's own scope guarantees resolves.
fn resolve(name: &str) -> alas_geom::asb::airfoil::Airfoil {
    AirfoilLibrary::get(name).unwrap_or_else(|| panic!("{name} did not resolve"))
}

#[test]
fn airfoil_library_get_resolves_every_branch_matching_the_reference() {
    let fixture: Fixture = alas_testkit::load("geom", "airfoil_library");

    let mut comparison = Comparison::new(
        "alas-geom::airfoil_library (AirfoilLibrary::get)",
        Tier::Linalg,
    );
    for (key, case) in &fixture.get {
        let resolved = AirfoilLibrary::get(&case.requested_name)
            .unwrap_or_else(|| panic!("{key}: expected {:?} to resolve", case.requested_name));
        comparison.exact(
            &format!("{key}.resolved_name"),
            &resolved.name,
            &case.resolved_name,
        );
        compare_points(
            &mut comparison,
            &format!("{key}.coordinates"),
            &resolved.coordinates,
            &case.coordinates,
        );
    }
    comparison.finish();
}

#[test]
fn normalize_coordinates_matches_the_reference_for_every_case() {
    let fixture: Fixture = alas_testkit::load("geom", "airfoil_library");

    let mut comparison = Comparison::new(
        "alas-geom::airfoil_library (normalize_coordinates)",
        Tier::Linalg,
    );
    for (key, case) in &fixture.normalize_coordinates {
        let actual = normalize_coordinates(&case.input);
        compare_points(&mut comparison, key, &actual, &case.output);
    }
    comparison.finish();
}

#[test]
fn apply_bumps_matches_the_reference_for_every_case() {
    let fixture: Fixture = alas_testkit::load("geom", "airfoil_library");

    let mut comparison = Comparison::new("alas-geom::airfoil_library (apply_bumps)", Tier::Linalg);
    for (key, case) in &fixture.apply_bumps {
        let base = resolve(&case.base);
        let bumped = apply_bumps(
            &base.coordinates,
            case.bumps_upper,
            case.bumps_lower,
            case.n_points_per_side,
        )
        .unwrap_or_else(|error| panic!("{key}: apply_bumps failed: {error}"));
        compare_points(&mut comparison, key, &bumped.coordinates, &case.coordinates);
    }
    comparison.finish();
}

#[test]
fn morph_airfoil_matches_the_reference_for_every_case() {
    let fixture: Fixture = alas_testkit::load("geom", "airfoil_library");

    let mut comparison =
        Comparison::new("alas-geom::airfoil_library (morph_airfoil)", Tier::Linalg);
    for (key, case) in &fixture.morph_airfoil {
        let base = resolve(&case.base);
        let morphed = morph_airfoil(
            &base.coordinates,
            case.thickness_scale,
            case.camber_scale,
            case.n_points,
        );
        compare_points(
            &mut comparison,
            key,
            &morphed.coordinates,
            &case.coordinates,
        );
    }
    comparison.finish();
}

#[test]
fn build_section_matches_the_reference_end_to_end() {
    let fixture: Fixture = alas_testkit::load("geom", "airfoil_library");
    let case = &fixture.build_section;

    let base = resolve(&case.base);
    let dv = DesignVector {
        bump_upper_front: case.design_vector.bump_upper_front,
        bump_upper_rear: case.design_vector.bump_upper_rear,
        bump_lower_mid: case.design_vector.bump_lower_mid,
        bump_lower_rear: case.design_vector.bump_lower_rear,
        airfoil_thickness_scale: case.design_vector.airfoil_thickness_scale,
        airfoil_camber_scale: case.design_vector.airfoil_camber_scale,
        ..DesignVector::default()
    };
    let section = build_section(&dv, &base.coordinates)
        .unwrap_or_else(|error| panic!("build_section failed: {error}"));

    let mut comparison =
        Comparison::new("alas-geom::airfoil_library (build_section)", Tier::Linalg);
    compare_points(
        &mut comparison,
        "build_section",
        &section.coordinates,
        &case.coordinates,
    );
    comparison.finish();
}

#[test]
fn the_fixture_names_the_three_branches_the_default_aircraft_needs() {
    let fixture: Fixture = alas_testkit::load("geom", "airfoil_library");
    assert!(fixture.get.contains_key("naca2410"));
    assert!(fixture.get.contains_key("SC2-0714"));
    assert!(fixture.get.contains_key("naca0012"));
}
