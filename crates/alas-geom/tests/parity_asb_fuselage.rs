// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-geom::asb::fuselage` against AeroSandbox's
//! `Fuselage`/`FuselageXSec`, via `golden/generators/gen_geom_asb_fuselage.py`.
//!
//! Every quantity here -- `FuselageXSec` construction from `radius` or from
//! `width`/`height`, and `.translate()` on both classes -- is closed-form
//! arithmetic, checked at `Tier::Closed`, the tier `docs/PORTING.md` names
//! for this row.
//!
//! The nose/cabin/tailcone station coordinates are rebuilt here from the same
//! literal `FuselageConfig`/`DesignVector` defaults and the same
//! `sinspace`/`linspace` formulas the generator's docstring records, mirroring
//! how `crates/alas-geom/tests/parity_asb_wing.rs` rebuilds its wings from
//! literal formulas rather than depending on a builder this phase does not
//! translate.

// This file is itself a test binary, so an unwrap or expect that fails is
// the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::f64::consts::FRAC_PI_2;

use alas_geom::asb::fuselage::{Fuselage, FuselageXSec, DEFAULT_SHAPE};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct XsecRecord {
    xyz_c: [f64; 3],
    width: f64,
    height: f64,
    shape: f64,
}

#[derive(Debug, Deserialize)]
struct FuselageRecord {
    name: String,
    xsecs: Vec<XsecRecord>,
}

#[derive(Debug, Deserialize)]
struct NacelleCase {
    untranslated: FuselageRecord,
    shift: [f64; 3],
    translated: FuselageRecord,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    main_fuselage: FuselageRecord,
    ovoid_fuselage: FuselageRecord,
    nacelle: NacelleCase,
}

// alas.config.geometry_config.FuselageConfig defaults.
const DIAMETER_M: f64 = 6.2;
const NOSE_Z_M: f64 = -0.5;
const CABIN_START_X_M: f64 = 6.0;
const CABIN_Z_M: f64 = 0.2;
const TAILCONE_LENGTH_M: f64 = 14.0;
const TAIL_Z_M: f64 = 1.8;

// alas.config.design_variables.DesignVector default.
const FUSELAGE_LENGTH_M: f64 = 76.72;

const RADIUS: f64 = DIAMETER_M / 2.0;

/// `numpy.linspace(start, stop, num, endpoint=True)`, forcing the exact
/// endpoints the way `alas-geom::asb::spacing::linspace` (private to that
/// module) does -- reimplemented here since this test crosses the crate
/// boundary.
fn linspace(start: f64, stop: f64, num: usize) -> Vec<f64> {
    let step = (stop - start) / (num - 1) as f64;
    let mut values: Vec<f64> = (0..num).map(|i| start + i as f64 * step).collect();
    let last = values.len() - 1;
    values[last] = stop;
    values
}

/// `aerosandbox.numpy.sinspace(0, 1, num)`: `1 - cos(linspace(0, pi/2, num))`,
/// with both endpoints forced exact.
fn sinspace01(num: usize) -> Vec<f64> {
    let mut spaced: Vec<f64> = linspace(0.0, FRAC_PI_2, num)
        .into_iter()
        .map(|t| 1.0 - t.cos())
        .collect();
    spaced[0] = 0.0;
    let last = spaced.len() - 1;
    spaced[last] = 1.0;
    spaced
}

/// The 20 (x, z, r) triples `_build_fuselage` computes, independent of the
/// circular/ovoid choice -- see `gen_geom_asb_fuselage.py`'s `_stations`.
fn stations() -> Vec<(f64, f64, f64)> {
    let cabin_end = FUSELAGE_LENGTH_M - TAILCONE_LENGTH_M;
    let mut stations = Vec::new();

    let x_nose = sinspace01(10);
    for &xi in &x_nose[..x_nose.len() - 1] {
        let x_val = xi * CABIN_START_X_M;
        let z_val = CABIN_Z_M + (NOSE_Z_M - CABIN_Z_M) * (1.0 - xi).powi(2);
        let r_val = RADIUS * (1.0 - (1.0 - xi).powi(2)).sqrt();
        stations.push((x_val, z_val, r_val));
    }

    stations.push((CABIN_START_X_M, CABIN_Z_M, RADIUS));
    stations.push((cabin_end, CABIN_Z_M, RADIUS));

    let x_tail = linspace(0.0, 1.0, 10);
    for &xi in &x_tail[1..] {
        let x_val = cabin_end + xi * TAILCONE_LENGTH_M;
        let z_val = CABIN_Z_M + (TAIL_Z_M - CABIN_Z_M) * xi.powf(1.5);
        let r_val = RADIUS * (1.0 - xi.powf(1.5));
        stations.push((x_val, z_val, r_val));
    }

    stations
}

fn build_main_fuselage() -> Fuselage {
    let xsecs = stations()
        .into_iter()
        .map(|(x, z, r)| {
            FuselageXSec::new([x, 0.0, z], Some(r), None, None, DEFAULT_SHAPE)
                .expect("radius alone is valid")
        })
        .collect();
    Fuselage::new("Fuselage", xsecs)
}

fn build_ovoid_fuselage(height_m: f64) -> Fuselage {
    let xsecs = stations()
        .into_iter()
        .map(|(x, z, r)| {
            let local_width = r * 2.0;
            let local_height = r * 2.0 * (height_m / DIAMETER_M);
            FuselageXSec::new(
                [x, 0.0, z],
                None,
                Some(local_width),
                Some(local_height),
                2.0,
            )
            .expect("width and height together are valid")
        })
        .collect();
    Fuselage::new("Fuselage", xsecs)
}

fn build_nacelle() -> Fuselage {
    // alas.config.geometry_config.EngineConfig defaults.
    let nacelle_profile: [(f64, f64); 6] = [
        (0.0, 0.40),
        (0.6, 0.92),
        (1.2, 1.0),
        (4.3, 1.0),
        (5.9, 0.82),
        (7.8, 0.45),
    ];
    let radius_scale_m = 2.1;

    let xsecs = nacelle_profile
        .into_iter()
        .map(|(x, r)| {
            FuselageXSec::new(
                [x, 0.0, 0.0],
                Some(radius_scale_m * r),
                None,
                None,
                DEFAULT_SHAPE,
            )
            .expect("radius alone is valid")
        })
        .collect();
    Fuselage::new("Nacelle R", xsecs)
}

fn compare_xsecs(
    comparison: &mut Comparison,
    label: &str,
    actual: &[FuselageXSec],
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
            &format!("{label}[{index}].xyz_c"),
            &xsec.xyz_c,
            &record.xyz_c,
        );
        comparison.scalar(&format!("{label}[{index}].width"), xsec.width, record.width);
        comparison.scalar(
            &format!("{label}[{index}].height"),
            xsec.height,
            record.height,
        );
        comparison.scalar(&format!("{label}[{index}].shape"), xsec.shape, record.shape);
    }
}

#[test]
fn main_fuselage_matches_aerosandbox() {
    let fixture: Fixture = alas_testkit::load("geom", "asb_fuselage");

    let mut comparison = Comparison::new("alas-geom::asb::fuselage (main_fuselage)", Tier::Closed);
    let built = build_main_fuselage();
    comparison.exact(
        "main_fuselage.name",
        &built.name,
        &fixture.main_fuselage.name,
    );
    compare_xsecs(
        &mut comparison,
        "main_fuselage",
        &built.xsecs,
        &fixture.main_fuselage.xsecs,
    );
    comparison.finish();
}

#[test]
fn ovoid_fuselage_matches_aerosandbox() {
    let fixture: Fixture = alas_testkit::load("geom", "asb_fuselage");

    let mut comparison = Comparison::new("alas-geom::asb::fuselage (ovoid_fuselage)", Tier::Closed);
    let built = build_ovoid_fuselage(7.5);
    comparison.exact(
        "ovoid_fuselage.name",
        &built.name,
        &fixture.ovoid_fuselage.name,
    );
    compare_xsecs(
        &mut comparison,
        "ovoid_fuselage",
        &built.xsecs,
        &fixture.ovoid_fuselage.xsecs,
    );
    comparison.finish();
}

#[test]
fn nacelle_translate_matches_aerosandbox() {
    let fixture: Fixture = alas_testkit::load("geom", "asb_fuselage");

    let nacelle = build_nacelle();
    let mut comparison = Comparison::new(
        "alas-geom::asb::fuselage (nacelle, untranslated)",
        Tier::Closed,
    );
    comparison.exact(
        "nacelle.name",
        &nacelle.name,
        &fixture.nacelle.untranslated.name,
    );
    compare_xsecs(
        &mut comparison,
        "nacelle",
        &nacelle.xsecs,
        &fixture.nacelle.untranslated.xsecs,
    );
    comparison.finish();

    let translated = nacelle.translate(fixture.nacelle.shift);
    let mut translate_comparison = Comparison::new(
        "alas-geom::asb::fuselage (nacelle, translated)",
        Tier::Closed,
    );
    translate_comparison.exact(
        "nacelle.translated.name",
        &translated.name,
        &fixture.nacelle.translated.name,
    );
    compare_xsecs(
        &mut translate_comparison,
        "nacelle.translated",
        &translated.xsecs,
        &fixture.nacelle.translated.xsecs,
    );
    translate_comparison.finish();
}
