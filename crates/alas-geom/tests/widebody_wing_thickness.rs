// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Lofted thickness of the B787-9 preset wing against a published
//! contemporary widebody thickness distribution.
//!
//! Boeing publishes no 787 section data. The NASA Common Research Model is
//! the public M 0.85 widebody wing NASA/TP-20210023843 (2022, Table I) sets
//! beside the 787-9: Vassberg, DeHaan, Rivers and Wahls, "Development of a
//! Common Research Model for Applied CFD Validation Studies", AIAA 2008-6919
//! (NTRS 20080034653), Table 2, gives t/c 0.1542 at the centreline, 0.1380 at
//! the side of body (10 % semispan), 0.1052 at the 37 % yehudi break and
//! 0.0950 at the tip, about 10.8 % averaged over the exposed wing.

// A test asserts on values it built here, so a failed expect is the assertion
// failing rather than a library invariant being broken.
#![allow(clippy::expect_used)]

use alas_config::presets;
use alas_geom::builder::AircraftBuilder;

/// Area-weighted t/c of CRM Table 2 from `eta_from` to the tip: (eta,
/// planform chord in inches, t/c) at the 21 defining stations.
fn crm_area_weighted_thickness(eta_from: f64) -> f64 {
    const STATIONS: [(f64, f64, f64); 21] = [
        (0.00, 536.181, 0.1542),
        (0.10, 468.511, 0.1380),
        (0.15, 434.674, 0.1280),
        (0.20, 400.835, 0.1198),
        (0.25, 366.996, 0.1137),
        (0.30, 333.157, 0.1092),
        (0.35, 299.317, 0.1060),
        (0.37, 285.782, 0.1052),
        (0.40, 277.288, 0.1038),
        (0.45, 263.130, 0.1019),
        (0.50, 248.973, 0.1000),
        (0.55, 234.816, 0.0988),
        (0.60, 220.658, 0.0978),
        (0.65, 206.501, 0.0970),
        (0.70, 192.344, 0.0962),
        (0.75, 178.186, 0.0958),
        (0.80, 164.029, 0.0955),
        (0.85, 149.872, 0.0953),
        (0.90, 135.714, 0.0952),
        (0.95, 121.557, 0.0951),
        (1.00, 107.400, 0.0950),
    ];
    let (mut area, mut moment) = (0.0, 0.0);
    for pair in STATIONS.windows(2).filter(|pair| pair[0].0 >= eta_from) {
        let panel = (pair[1].0 - pair[0].0) * (pair[0].1 + pair[1].1) / 2.0;
        area += panel;
        moment += panel * (pair[0].2 + pair[1].2) / 2.0;
    }
    moment / area
}

/// Area-weighted t/c of the lofted main wing outboard of `y_from_m`.
fn lofted_area_weighted_thickness(wing: &alas_geom::aircraft::wing::Wing, y_from_m: f64) -> f64 {
    let samples: Vec<f64> = (0..=200).map(|i| f64::from(i) / 200.0).collect();
    let (mut area, mut moment) = (0.0, 0.0);
    for pair in wing.xsecs.windows(2) {
        if pair[0].xyz_le[1] < y_from_m - 1.0e-9 {
            continue;
        }
        let span = pair[1].xyz_le[1] - pair[0].xyz_le[1];
        let panel = span * (pair[0].chord + pair[1].chord) / 2.0;
        let thickness = (pair[0].airfoil.max_thickness(&samples)
            + pair[1].airfoil.max_thickness(&samples))
            / 2.0;
        area += panel;
        moment += panel * thickness;
    }
    moment / area
}

#[test]
fn b787_lofted_thickness_matches_the_common_research_model_average() {
    let preset = presets::get("B787-9").expect("B787 preset");
    let plane = AircraftBuilder::new(Some(preset.geometry.clone()))
        .build(Some(&preset.design_vector), false)
        .expect("preset geometry");
    let wing = &plane.wings[0];
    let semispan_m = preset.design_vector.span_m / 2.0;
    let side_of_body_m = preset
        .geometry
        .wing
        .side_of_body_span_fraction
        .expect("the B787 declares a side-of-body station")
        * semispan_m;

    // The loft carries one section from the centreline to the kink and
    // blends it into the tip section, so it cannot follow the CRM's
    // root-to-kink thickness taper; the root section is the SC(2) member
    // whose loft lands on the CRM area-weighted mean. Whole wing: CRM
    // 0.115; exposed wing: CRM 0.108.
    let whole = lofted_area_weighted_thickness(wing, 0.0);
    let exposed = lofted_area_weighted_thickness(wing, side_of_body_m);
    let crm_whole = crm_area_weighted_thickness(0.0);
    let crm_exposed = crm_area_weighted_thickness(0.10);
    assert!(
        (crm_exposed - 0.108).abs() < 1.0e-3,
        "CRM exposed {crm_exposed}"
    );
    assert!(
        (whole - crm_whole).abs() < 0.005,
        "lofted whole-wing t/c {whole} against CRM {crm_whole}"
    );
    assert!(
        (exposed - crm_exposed).abs() < 0.01,
        "lofted exposed t/c {exposed} against CRM {crm_exposed}"
    );
}
