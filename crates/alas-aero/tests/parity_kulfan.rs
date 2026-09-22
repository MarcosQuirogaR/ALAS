// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-aero::kulfan` against AeroSandbox's `get_kulfan_parameters`
//! and `KulfanAirfoil`'s surface samplers, via
//! `golden/generators/gen_aero_kulfan.py`.
//!
//! `docs/PORTING.md` names `Tier::Linalg` for this row: the fit runs through a
//! Householder QR here and LAPACK's `gelsd` there, and two least-squares
//! routines that both minimize the same residual do not have to agree beyond
//! the conditioning of the problem. These matrices sit at about 1.1e3, so the
//! observed agreement is nearer 3e-12, but the tier states the construction,
//! not today's numbers, and a routine whose error scales with `cond(A)` is
//! exactly the case the tier table names.
//!
//! The near-zero case a symmetric section produces needs no special handling
//! here, unlike `parity_asb_vlm.rs`'s moments. A symmetric airfoil's
//! leading-edge weight is exactly zero in closed form and lands at 1e-16 in
//! both implementations, which is three orders below `Tier::Linalg`'s own
//! absolute floor; the floor already frames it, so nothing is compared against
//! a hand-picked bound.

// This file is itself a test binary, so an unwrap or expect that fails is
// the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::HashMap;

use alas_aero::kulfan::KulfanAirfoil;
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ToAirfoil {
    n_coordinates_per_side: usize,
    coordinates: Vec<[f64; 2]>,
}

#[derive(Debug, Deserialize)]
struct Case {
    n_weights_per_side: usize,
    #[serde(rename = "N1")]
    n1: f64,
    #[serde(rename = "N2")]
    n2: f64,
    coordinates: Vec<[f64; 2]>,
    lower_weights: Vec<f64>,
    upper_weights: Vec<f64>,
    leading_edge_weight: f64,
    te_thickness: f64,
    x_sample: Vec<f64>,
    upper_coordinates: Vec<[f64; 2]>,
    lower_coordinates: Vec<[f64; 2]>,
    max_thickness: f64,
    to_airfoil: Option<ToAirfoil>,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    cases: HashMap<String, Case>,
}

impl Case {
    fn input(&self) -> Vec<(f64, f64)> {
        self.coordinates.iter().map(|&[x, y]| (x, y)).collect()
    }

    fn fitted(&self) -> KulfanAirfoil {
        KulfanAirfoil::fit(&self.input(), self.n_weights_per_side, self.n1, self.n2)
            .expect("a full-rank fit, as the reference obtained")
    }
}

fn flatten(pairs: &[(f64, f64)]) -> Vec<f64> {
    pairs.iter().flat_map(|&(x, y)| [x, y]).collect()
}

fn flatten_reference(pairs: &[[f64; 2]]) -> Vec<f64> {
    pairs.iter().flat_map(|&[x, y]| [x, y]).collect()
}

#[test]
fn the_fit_matches_aerosandbox_on_every_section() {
    let fixture: Fixture = alas_testkit::load("aero", "kulfan");
    let mut comparison = Comparison::new(
        "alas-aero::kulfan (get_kulfan_parameters, method='least_squares')",
        Tier::Linalg,
    );

    for (name, case) in &fixture.cases {
        let fitted = case.fitted();
        comparison.slice(
            &format!("{name}.lower_weights"),
            &fitted.lower_weights,
            &case.lower_weights,
        );
        comparison.slice(
            &format!("{name}.upper_weights"),
            &fitted.upper_weights,
            &case.upper_weights,
        );
        comparison.scalar(
            &format!("{name}.leading_edge_weight"),
            fitted.leading_edge_weight,
            case.leading_edge_weight,
        );
        comparison.scalar(
            &format!("{name}.te_thickness"),
            fitted.te_thickness,
            case.te_thickness,
        );
    }
    comparison.finish();
}

#[test]
fn a_pinned_trailing_edge_is_zero_and_not_merely_small() {
    // The re-solve branch assigns literal zero rather than fitting a thickness
    // that rounds to it, so this is the one quantity in the row where
    // agreement is a discrete fact about which branch ran, not a tolerance
    // question. Checking it at `Tier::Linalg` alongside everything else would
    // let a port that clamped a negative fit to -1e-13 pass.
    let fixture: Fixture = alas_testkit::load("aero", "kulfan");
    let mut comparison = Comparison::new(
        "alas-aero::kulfan (the trailing-edge re-solve branch)",
        Tier::Exact,
    );

    let mut pinned = 0;
    for (name, case) in &fixture.cases {
        if case.te_thickness != 0.0 {
            continue;
        }
        pinned += 1;
        comparison.exact(
            &format!("{name}.te_thickness is exactly zero"),
            &case.fitted().te_thickness,
            &0.0,
        );
    }
    assert!(
        pinned > 0,
        "the fixture no longer reaches the re-solve branch; gen_aero_kulfan.py \
         should have refused to write it"
    );
    comparison.finish();
}

#[test]
fn the_surface_samplers_match_aerosandbox_at_every_station() {
    let fixture: Fixture = alas_testkit::load("aero", "kulfan");
    let mut comparison = Comparison::new(
        "alas-aero::kulfan (KulfanAirfoil upper_coordinates/lower_coordinates)",
        Tier::Linalg,
    );

    for (name, case) in &fixture.cases {
        let fitted = case.fitted();
        comparison.slice(
            &format!("{name}.upper_coordinates"),
            &flatten(&fitted.upper_coordinates(&case.x_sample)),
            &flatten_reference(&case.upper_coordinates),
        );
        comparison.slice(
            &format!("{name}.lower_coordinates"),
            &flatten(&fitted.lower_coordinates(&case.x_sample)),
            &flatten_reference(&case.lower_coordinates),
        );
    }
    comparison.finish();
}

#[test]
fn max_thickness_matches_aerosandbox_on_every_section() {
    // `KulfanAirfoil` inherits `Airfoil.max_thickness`'s name and replaces
    // what is underneath it; this samples the two class-times-shape
    // surfaces analytically where `alas-geom::asb::airfoil`'s interpolates a
    // vertex list. `alas-aero::neuralfoil` reads this one, and only this one,
    // for the `t/c` that sets the supersonic end of its wave-drag schedule.
    let fixture: Fixture = alas_testkit::load("aero", "kulfan");
    let mut comparison = Comparison::new(
        "alas-aero::kulfan (KulfanAirfoil::max_thickness)",
        Tier::Linalg,
    );

    // The upstream default sample grid, `np.linspace(0, 1, 101)`.
    let sample: Vec<f64> = (0..101).map(|index| f64::from(index) / 100.0).collect();
    for (name, case) in &fixture.cases {
        comparison.scalar(
            &format!("{name}.max_thickness"),
            case.fitted().max_thickness(&sample),
            case.max_thickness,
        );
    }
    comparison.finish();
}

#[test]
fn the_reconstruction_matches_aerosandbox_where_the_fixture_records_it() {
    let fixture: Fixture = alas_testkit::load("aero", "kulfan");
    let mut comparison = Comparison::new(
        "alas-aero::kulfan (KulfanAirfoil::to_airfoil)",
        Tier::Linalg,
    );

    let mut checked = 0;
    for (name, case) in &fixture.cases {
        let Some(reference) = case.to_airfoil.as_ref() else {
            continue;
        };
        checked += 1;
        let rebuilt = case
            .fitted()
            .to_airfoil(name, reference.n_coordinates_per_side);
        comparison.slice(
            &format!("{name}.to_airfoil"),
            &flatten(&rebuilt.coordinates),
            &flatten_reference(&reference.coordinates),
        );
    }
    assert!(checked > 0, "no case records a reconstruction to compare");
    comparison.finish();
}
