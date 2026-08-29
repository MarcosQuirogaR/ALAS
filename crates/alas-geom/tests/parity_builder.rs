// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-geom::builder` against `alas.geometry.aircraft_builder`,
//! via `golden/generators/gen_geom_builder.py`.
//!
//! This is Phase 3's centerpiece check: the fixture is the actual nominal
//! aircraft (`AircraftBuilder(GeometryConfig()).build(dv=None, ...)`), so
//! comparing against it exercises every other module in this crate together
//! -- all three of `AirfoilLibrary::get`'s name-resolution branches, the
//! root section's `build_section` shaping, and both `asb::wing`/
//! `asb::fuselage` lofting -- on real input rather than a synthetic probe.
//!
//! Two tiers are in play, the same split `wing_structure`'s and
//! `airfoil_library`'s own parity tests use. Every quantity native to
//! `Wing`/`WingXSec`/`Fuselage`/`FuselageXSec`/`Airplane` themselves --
//! positions, chords, twists, areas, spans, the mean aerodynamic chord, the
//! aerodynamic center, the taper ratio, the fuselage stations, the returned
//! `Airplane`'s reference quantities -- is closed-form arithmetic and is
//! checked at `Tier::Closed`, the tier `docs/PORTING.md` names for this row.
//! The wing sections' airfoil coordinate arrays are the one exception: the
//! root and break sections pass through `build_section`'s cubic-spline
//! `repanel` step, and every section subdivided by `Wing::subdivide_sections`
//! is re-blended through the same spline, so those are checked at
//! `Tier::Linalg` instead of pulling the whole row to a looser tier it does
//! not otherwise need.

// This file is itself a test binary, so an unwrap or expect that fails is
// the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_config::GeometryConfig;
use alas_geom::asb::airplane::Airplane;
use alas_geom::asb::fuselage::Fuselage;
use alas_geom::asb::wing::Wing;
use alas_geom::builder::AircraftBuilder;
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct XsecRecord {
    xyz_le: [f64; 3],
    chord: f64,
    twist: f64,
    airfoil_name: String,
    airfoil_coordinates: Vec<(f64, f64)>,
}

#[derive(Debug, Deserialize)]
struct WingRecord {
    name: String,
    symmetric: bool,
    xsecs: Vec<XsecRecord>,
    area: f64,
    span: f64,
    mean_aerodynamic_chord: f64,
    aerodynamic_center: [f64; 3],
    taper_ratio: f64,
}

#[derive(Debug, Deserialize)]
struct FuselageXsecRecord {
    xyz_c: [f64; 3],
    width: f64,
    height: f64,
}

#[derive(Debug, Deserialize)]
struct FuselageRecord {
    name: String,
    xsecs: Vec<FuselageXsecRecord>,
}

#[derive(Debug, Deserialize)]
struct AirplaneRecord {
    name: String,
    xyz_ref: [f64; 3],
    s_ref: f64,
    c_ref: f64,
    b_ref: f64,
    wings: Vec<WingRecord>,
    fuselages: Vec<FuselageRecord>,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    with_engines: AirplaneRecord,
    without_engines: AirplaneRecord,
}

fn compare_wing(
    closed: &mut Comparison,
    linalg: &mut Comparison,
    label: &str,
    wing: &Wing,
    expected: &WingRecord,
) {
    closed.exact(&format!("{label}.name"), &wing.name, &expected.name);
    closed.exact(
        &format!("{label}.symmetric"),
        &wing.symmetric,
        &expected.symmetric,
    );
    closed.scalar(&format!("{label}.area"), wing.area(), expected.area);
    closed.scalar(&format!("{label}.span"), wing.span(), expected.span);
    closed.scalar(
        &format!("{label}.mean_aerodynamic_chord"),
        wing.mean_aerodynamic_chord(),
        expected.mean_aerodynamic_chord,
    );
    let ac = wing.aerodynamic_center(0.25);
    closed.slice(
        &format!("{label}.aerodynamic_center"),
        &ac,
        &expected.aerodynamic_center,
    );
    closed.scalar(
        &format!("{label}.taper_ratio"),
        wing.taper_ratio(),
        expected.taper_ratio,
    );

    if wing.xsecs.len() != expected.xsecs.len() {
        closed.exact(
            &format!("{label}.xsecs (count)"),
            &wing.xsecs.len(),
            &expected.xsecs.len(),
        );
        return;
    }
    for (index, (xsec, record)) in wing.xsecs.iter().zip(&expected.xsecs).enumerate() {
        let xlabel = format!("{label}.xsecs[{index}]");
        closed.slice(&format!("{xlabel}.xyz_le"), &xsec.xyz_le, &record.xyz_le);
        closed.scalar(&format!("{xlabel}.chord"), xsec.chord, record.chord);
        closed.scalar(&format!("{xlabel}.twist"), xsec.twist, record.twist);
        closed.exact(
            &format!("{xlabel}.airfoil_name"),
            &xsec.airfoil.name,
            &record.airfoil_name,
        );

        if xsec.airfoil.coordinates.len() != record.airfoil_coordinates.len() {
            linalg.exact(
                &format!("{xlabel}.airfoil_coordinates (point count)"),
                &xsec.airfoil.coordinates.len(),
                &record.airfoil_coordinates.len(),
            );
            continue;
        }
        for (point_index, (&(ax, ay), &(ex, ey))) in xsec
            .airfoil
            .coordinates
            .iter()
            .zip(&record.airfoil_coordinates)
            .enumerate()
        {
            linalg.scalar(
                &format!("{xlabel}.airfoil_coordinates[{point_index}].x"),
                ax,
                ex,
            );
            linalg.scalar(
                &format!("{xlabel}.airfoil_coordinates[{point_index}].y"),
                ay,
                ey,
            );
        }
    }
}

fn compare_fuselage(
    comparison: &mut Comparison,
    label: &str,
    fuselage: &Fuselage,
    expected: &FuselageRecord,
) {
    comparison.exact(&format!("{label}.name"), &fuselage.name, &expected.name);
    if fuselage.xsecs.len() != expected.xsecs.len() {
        comparison.exact(
            &format!("{label}.xsecs (count)"),
            &fuselage.xsecs.len(),
            &expected.xsecs.len(),
        );
        return;
    }
    for (index, (xsec, record)) in fuselage.xsecs.iter().zip(&expected.xsecs).enumerate() {
        let xlabel = format!("{label}.xsecs[{index}]");
        comparison.slice(&format!("{xlabel}.xyz_c"), &xsec.xyz_c, &record.xyz_c);
        comparison.scalar(&format!("{xlabel}.width"), xsec.width, record.width);
        comparison.scalar(&format!("{xlabel}.height"), xsec.height, record.height);
    }
}

fn compare_airplane(
    closed: &mut Comparison,
    linalg: &mut Comparison,
    label: &str,
    airplane: &Airplane,
    expected: &AirplaneRecord,
) {
    closed.exact(&format!("{label}.name"), &airplane.name, &expected.name);
    closed.slice(
        &format!("{label}.xyz_ref"),
        &airplane.xyz_ref,
        &expected.xyz_ref,
    );
    closed.scalar(&format!("{label}.s_ref"), airplane.s_ref, expected.s_ref);
    closed.scalar(&format!("{label}.c_ref"), airplane.c_ref, expected.c_ref);
    closed.scalar(&format!("{label}.b_ref"), airplane.b_ref, expected.b_ref);
    // Reference axes are deliberately projected even though the compatibility
    // wing fields above retain the upstream unfolded `area()`/`span()` values.
    closed.scalar(
        &format!("{label}.s_ref_is_projected"),
        airplane.s_ref,
        airplane.wings[0].reference_area(),
    );
    closed.scalar(
        &format!("{label}.b_ref_is_projected"),
        airplane.b_ref,
        airplane.wings[0].reference_span(),
    );

    assert_eq!(airplane.wings.len(), expected.wings.len(), "{label}.wings");
    for (wing, record) in airplane.wings.iter().zip(&expected.wings) {
        compare_wing(
            closed,
            linalg,
            &format!("{label}.{}", record.name),
            wing,
            record,
        );
    }

    assert_eq!(
        airplane.fuselages.len(),
        expected.fuselages.len(),
        "{label}.fuselages"
    );
    for (fuselage, record) in airplane.fuselages.iter().zip(&expected.fuselages) {
        compare_fuselage(
            closed,
            &format!("{label}.{}", record.name),
            fuselage,
            record,
        );
    }
}

#[test]
fn the_default_aircraft_with_engines_matches_the_reference() {
    let fixture: Fixture = alas_testkit::load("geom", "builder");
    let builder = AircraftBuilder::new_reference_compatibility(Some(GeometryConfig::default()));
    let airplane = builder
        .build(None, true)
        .expect("the default aircraft builds cleanly");

    let mut closed = Comparison::new(
        "alas-geom::builder (with_engines, closed-form fields)",
        Tier::Closed,
    );
    let mut linalg = Comparison::new(
        "alas-geom::builder (with_engines, airfoil coordinates)",
        Tier::Linalg,
    );
    compare_airplane(
        &mut closed,
        &mut linalg,
        "with_engines",
        &airplane,
        &fixture.with_engines,
    );
    closed.finish();
    linalg.finish();
}

#[test]
fn the_default_aircraft_without_engines_matches_the_reference() {
    let fixture: Fixture = alas_testkit::load("geom", "builder");
    let builder = AircraftBuilder::new_reference_compatibility(Some(GeometryConfig::default()));
    let airplane = builder
        .build(None, false)
        .expect("the default aircraft builds cleanly");
    assert_eq!(airplane.fuselages.len(), 1);

    let mut closed = Comparison::new(
        "alas-geom::builder (without_engines, closed-form fields)",
        Tier::Closed,
    );
    let mut linalg = Comparison::new(
        "alas-geom::builder (without_engines, airfoil coordinates)",
        Tier::Linalg,
    );
    compare_airplane(
        &mut closed,
        &mut linalg,
        "without_engines",
        &airplane,
        &fixture.without_engines,
    );
    closed.finish();
    linalg.finish();
}
