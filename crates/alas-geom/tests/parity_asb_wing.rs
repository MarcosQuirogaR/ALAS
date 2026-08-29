// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-geom::asb::wing` against AeroSandbox's `Wing`/`WingXSec`,
//! via `golden/generators/gen_geom_asb_wing.py`.
//!
//! Every quantity native to `Wing`/`WingXSec` itself -- `translate`,
//! `subdivide_sections`' blended `xyz_le`/`chord`/`twist`, `span`, `area`,
//! `mean_aerodynamic_chord`, `aerodynamic_center`, `taper_ratio` -- is
//! closed-form arithmetic and is checked at `Tier::Closed`, the tier
//! `docs/PORTING.md` names for this row. The one exception is the blended
//! airfoil's coordinate array that `subdivide_sections` also produces: that
//! passes through `Airfoil::repanel`'s cubic spline (`alas-math::spline`),
//! the same reason `alas-geom::asb::airfoil`'s row is `linalg`, so that one
//! comparison uses `Tier::Linalg` instead rather than pulling the whole row
//! to a looser tier it does not otherwise need.

// This file is itself a test binary, so an unwrap or expect that fails is
// the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::HashMap;

use alas_geom::asb::airfoil::Airfoil;
use alas_geom::asb::wing::{SpacingFunction, Wing, WingXSec};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct WingSummary {
    span: f64,
    area: f64,
    mean_aerodynamic_chord: f64,
    aerodynamic_center: [f64; 3],
    taper_ratio: f64,
}

#[derive(Debug, Deserialize)]
struct XsecRecord {
    xyz_le: [f64; 3],
    chord: f64,
    twist: f64,
    airfoil_name: String,
}

#[derive(Debug, Deserialize)]
struct SubdivideCase {
    ratio: usize,
    xsec_count: usize,
    xsecs: Vec<XsecRecord>,
    blended_index: usize,
    blended_airfoil_name: String,
    blended_airfoil_coordinates: Vec<(f64, f64)>,
}

#[derive(Debug, Deserialize)]
struct TranslateCase {
    shift: [f64; 3],
    xsecs: Vec<XsecRecord>,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    wings: HashMap<String, WingSummary>,
    subdivide_sections: SubdivideCase,
    translate: TranslateCase,
}

/// The three wings the fixture builds -- shaped like `aircraft_builder.py`'s
/// main wing, hstab and vstab -- rebuilt in Rust from the same literal
/// values the generator records in its own docstring.
fn build_main_wing() -> Wing {
    let root_z_m = -2.1;
    let break_z_m = -0.3;
    let tip_z_m = 2.5;
    let root_twist_deg = 4.0;
    let break_twist_deg = 2.0;
    let break_span_fraction = 0.35;
    let outboard_sweep_decrement_deg: f64 = 2.0;

    let span_m: f64 = 71.75;
    let root_chord_m = 16.50;
    let break_chord_m = 7.80;
    let tip_chord_m = 1.60;
    let sweep_deg: f64 = 34.00;
    let tip_twist_deg = 0.00;

    let semi_span = span_m / 2.0;
    let y_break = break_span_fraction * semi_span;
    let sweep_in = sweep_deg.to_radians();
    let sweep_out = (sweep_deg - outboard_sweep_decrement_deg).to_radians();
    let dx_break = y_break * sweep_in.tan();
    let dx_tip = dx_break + (semi_span - y_break) * sweep_out.tan();

    let root_section = Airfoil::from_name("naca4412").expect("naca4412 parses");
    let tip_airfoil = Airfoil::from_name("naca2410").expect("naca2410 parses");

    Wing::new(
        "Main Wing",
        vec![
            WingXSec::new(
                [0.0, 0.0, root_z_m],
                root_chord_m,
                root_twist_deg,
                root_section.clone(),
            ),
            WingXSec::new(
                [dx_break, y_break, break_z_m],
                break_chord_m,
                break_twist_deg,
                root_section,
            ),
            WingXSec::new(
                [dx_tip, semi_span, tip_z_m],
                tip_chord_m,
                tip_twist_deg,
                tip_airfoil,
            ),
        ],
        true,
    )
}

fn build_hstab() -> Wing {
    let tail_airfoil = Airfoil::from_name("naca0012").expect("naca0012 parses");
    Wing::new(
        "Horizontal Stabilizer",
        vec![
            WingXSec::new([0.0, 0.0, 0.0], 8.0, -2.0, tail_airfoil.clone()),
            WingXSec::new([7.5, 11.0, 1.0], 2.2, -2.0, tail_airfoil),
        ],
        true,
    )
}

fn build_vstab() -> Wing {
    let tail_airfoil = Airfoil::from_name("naca0012").expect("naca0012 parses");
    Wing::new(
        "Vertical Stabilizer",
        vec![
            WingXSec::new([0.0, 0.0, 0.0], 9.5, 0.0, tail_airfoil.clone()),
            WingXSec::new([9.0, 0.0, 9.8], 3.2, 0.0, tail_airfoil),
        ],
        false,
    )
}

fn compare_summary(comparison: &mut Comparison, label: &str, wing: &Wing, expected: &WingSummary) {
    comparison.scalar(&format!("{label}.span"), wing.span(), expected.span);
    comparison.scalar(&format!("{label}.area"), wing.area(), expected.area);
    comparison.scalar(
        &format!("{label}.mean_aerodynamic_chord"),
        wing.mean_aerodynamic_chord(),
        expected.mean_aerodynamic_chord,
    );
    let ac = wing.aerodynamic_center(0.25);
    comparison.slice(
        &format!("{label}.aerodynamic_center"),
        &ac,
        &expected.aerodynamic_center,
    );
    comparison.scalar(
        &format!("{label}.taper_ratio"),
        wing.taper_ratio(),
        expected.taper_ratio,
    );
}

fn compare_xsecs(
    comparison: &mut Comparison,
    label: &str,
    actual: &[WingXSec],
    expected: &[XsecRecord],
) {
    if actual.len() != expected.len() {
        comparison.exact(
            &format!("{label} (xsec count)"),
            &actual.len(),
            &expected.len(),
        );
        return;
    }
    for (index, (xsec, record)) in actual.iter().zip(expected).enumerate() {
        comparison.slice(
            &format!("{label}[{index}].xyz_le"),
            &xsec.xyz_le,
            &record.xyz_le,
        );
        comparison.scalar(&format!("{label}[{index}].chord"), xsec.chord, record.chord);
        comparison.scalar(&format!("{label}[{index}].twist"), xsec.twist, record.twist);
        comparison.exact(
            &format!("{label}[{index}].airfoil_name"),
            &xsec.airfoil.name,
            &record.airfoil_name,
        );
    }
}

#[test]
fn wing_summary_quantities_match_aerosandbox() {
    let fixture: Fixture = alas_testkit::load("geom", "asb_wing");

    let mut comparison = Comparison::new(
        "alas-geom::asb::wing (span/area/MAC/AC/taper)",
        Tier::Closed,
    );
    compare_summary(
        &mut comparison,
        "main_wing",
        &build_main_wing(),
        &fixture.wings["main_wing"],
    );
    compare_summary(
        &mut comparison,
        "hstab",
        &build_hstab(),
        &fixture.wings["hstab"],
    );
    compare_summary(
        &mut comparison,
        "vstab",
        &build_vstab(),
        &fixture.wings["vstab"],
    );
    comparison.finish();
}

#[test]
fn translate_matches_aerosandbox() {
    let fixture: Fixture = alas_testkit::load("geom", "asb_wing");

    let translated = build_main_wing().translate(fixture.translate.shift);
    let mut comparison = Comparison::new("alas-geom::asb::wing (translate)", Tier::Closed);
    compare_xsecs(
        &mut comparison,
        "main_wing.translate",
        &translated.xsecs,
        &fixture.translate.xsecs,
    );
    comparison.finish();
}

#[test]
fn subdivide_sections_matches_aerosandbox() {
    let fixture: Fixture = alas_testkit::load("geom", "asb_wing");
    let case = &fixture.subdivide_sections;

    let subdivided = build_main_wing()
        .subdivide_sections(case.ratio, SpacingFunction::Linspace)
        .expect("ratio=8 is valid and every blend repanels cleanly");

    let mut comparison = Comparison::new("alas-geom::asb::wing (subdivide_sections)", Tier::Closed);
    comparison.exact("xsec_count", &subdivided.xsecs.len(), &case.xsec_count);
    compare_xsecs(
        &mut comparison,
        "main_wing.subdivide_sections",
        &subdivided.xsecs,
        &case.xsecs,
    );
    comparison.finish();

    // The blended airfoil's coordinate array passes through `repanel`'s
    // cubic spline, so it is compared separately at `Tier::Linalg` -- see
    // the module doc.
    let blended = &subdivided.xsecs[case.blended_index].airfoil;
    let mut blend_comparison = Comparison::new(
        "alas-geom::asb::wing (subdivide_sections, blended airfoil)",
        Tier::Linalg,
    );
    blend_comparison.exact(
        "blended_airfoil_name",
        &blended.name,
        &case.blended_airfoil_name,
    );
    if blended.coordinates.len() != case.blended_airfoil_coordinates.len() {
        blend_comparison.exact(
            "blended_airfoil_coordinates (point count)",
            &blended.coordinates.len(),
            &case.blended_airfoil_coordinates.len(),
        );
    } else {
        for (index, (&(ax, ay), &(ex, ey))) in blended
            .coordinates
            .iter()
            .zip(&case.blended_airfoil_coordinates)
            .enumerate()
        {
            blend_comparison.scalar(&format!("blended_airfoil_coordinates[{index}].x"), ax, ex);
            blend_comparison.scalar(&format!("blended_airfoil_coordinates[{index}].y"), ay, ey);
        }
    }
    blend_comparison.finish();
}
