// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-geom::wing_structure` against
//! `alas.geometry.wing_structure.WingStructureGeometry`, via
//! `golden/generators/gen_geom_wing_structure.py`.
//!
//! Two tiers are in play, matching `docs/PORTING.md`'s `closed` row for this
//! module with one carve-out. Everything the module computes from planform
//! geometry alone: `local_chord`/`x_le`/`z_le`/`rib_vector`/
//! `le_direction`, `get_rib_lengths`, `compute_spar_intersections`, and every
//! [`RibStation`](alas_geom::wing_structure::RibStation) field except its
//! surface points, never reads an airfoil coordinate and is checked at
//! [`Tier::Closed`]. `airfoil_zu_zl`, `spar_height`, and rib
//! `extrados`/`intrados` all sample the root section this test builds with
//! `build_section`, which repanels through a cubic spline
//! (`alas-math::spline`) on its way there: the same reason
//! `alas-geom::airfoil_library`'s own row is `linalg`, so those are checked
//! at [`Tier::Linalg`] instead of pulling the whole module to a looser tier
//! it does not otherwise need.

// This file is itself a test binary, so an unwrap or expect that fails is
// the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_config::{DesignVector, WingConfig};
use alas_geom::airfoil_library::{build_section, AirfoilLibrary};
use alas_geom::wing_structure::WingStructureGeometry;
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Config {
    spar_chord_fractions: Vec<f64>,
    spar_full_span: Vec<bool>,
    root_airfoil: String,
    tip_airfoil: String,
    num_ribs: usize,
    num_pts_chord: usize,
}

#[derive(Debug, Deserialize)]
struct Derived {
    semi_span: f64,
    break_eta: f64,
    y_break: f64,
    sweep_in: f64,
    sweep_out: f64,
    dx_break: f64,
    dx_tip: f64,
    spar_fracs_sorted: Vec<f64>,
    spar_full_span_sorted: Vec<bool>,
}

#[derive(Debug, Deserialize)]
struct PlanformCase {
    eta: f64,
    local_chord: f64,
    x_le: f64,
    z_le: f64,
    rib_vector: [f64; 2],
    le_direction: [f64; 2],
}

#[derive(Debug, Deserialize)]
struct AirfoilCase {
    eta: f64,
    xc_frac: f64,
    zu: f64,
    zl: f64,
}

#[derive(Debug, Deserialize)]
struct RibLengthsCase {
    y_le_val: f64,
    eta: f64,
    x_le_val: f64,
    aft_x: f64,
    aft_y: f64,
    l_nominal: f64,
    l_actual: f64,
    truncated: bool,
    spar_intersections: Vec<Option<f64>>,
}

#[derive(Debug, Deserialize)]
struct RibStationRecord {
    index: usize,
    eta: f64,
    y_station: f64,
    is_full: bool,
    frac_actual: f64,
    extrados: Vec<[f64; 3]>,
    intrados: Vec<[f64; 3]>,
    j_spars: Vec<i32>,
    rib_dir_xy: [f64; 2],
}

#[derive(Debug, Deserialize)]
struct Fixture {
    config: Config,
    derived: Derived,
    planform: Vec<PlanformCase>,
    airfoil_zu_zl: Vec<AirfoilCase>,
    rib_lengths_and_spars: Vec<RibLengthsCase>,
    rib_stations: Vec<RibStationRecord>,
}

/// Rebuild the same geometry the generator did: the default `DesignVector`
/// and `WingConfig`, the root section via `build_section` on the configured
/// root airfoil, and the tip section resolved directly: the exact call
/// pattern `pipeline.py`'s structural-analysis stage uses.
fn build_geometry(fixture: &Fixture) -> WingStructureGeometry {
    let dv = DesignVector::default();
    let wing_cfg = WingConfig::default();
    assert_eq!(wing_cfg.root_airfoil, fixture.config.root_airfoil);
    assert_eq!(wing_cfg.tip_airfoil, fixture.config.tip_airfoil);

    let root_base =
        AirfoilLibrary::get(&wing_cfg.root_airfoil).expect("the configured root airfoil resolves");
    let root_section =
        build_section(&dv, &root_base.coordinates).expect("the root section repanels cleanly");
    let tip_airfoil =
        AirfoilLibrary::get(&wing_cfg.tip_airfoil).expect("the configured tip airfoil resolves");

    WingStructureGeometry::new(
        &dv,
        &wing_cfg,
        &root_section,
        &tip_airfoil,
        &fixture.config.spar_chord_fractions,
        Some(&fixture.config.spar_full_span),
    )
    .expect("the fixture's spar list is valid")
}

fn compare_option_scalar(
    comparison: &mut Comparison,
    name: &str,
    actual: Option<f64>,
    expected: Option<f64>,
) {
    match (actual, expected) {
        (Some(a), Some(e)) => {
            comparison.scalar(name, a, e);
        }
        (None, None) => {}
        _ => {
            comparison.exact(name, &actual.is_some(), &expected.is_some());
        }
    }
}

#[test]
fn derived_and_planform_quantities_match_the_reference() {
    let fixture: Fixture = alas_testkit::load("geom", "wing_structure");
    let geometry = build_geometry(&fixture);

    let mut comparison = Comparison::new(
        "alas-geom::wing_structure (derived fields + planform)",
        Tier::Closed,
    );

    let derived = &fixture.derived;
    comparison
        .scalar("semi_span", geometry.semi_span, derived.semi_span)
        .scalar("break_eta", geometry.break_eta, derived.break_eta)
        .scalar("y_break", geometry.y_break, derived.y_break)
        .scalar("sweep_in", geometry.sweep_in, derived.sweep_in)
        .scalar("sweep_out", geometry.sweep_out, derived.sweep_out)
        .scalar("dx_break", geometry.dx_break, derived.dx_break)
        .scalar("dx_tip", geometry.dx_tip, derived.dx_tip)
        .slice(
            "spar_fracs_sorted",
            &geometry.spar_fracs,
            &derived.spar_fracs_sorted,
        );
    comparison.exact(
        "spar_full_span_sorted",
        &geometry.spar_full_span,
        &derived.spar_full_span_sorted,
    );

    for case in &fixture.planform {
        let label = format!("eta={}", case.eta);
        comparison.scalar(
            &format!("{label}.local_chord"),
            geometry.local_chord(case.eta),
            case.local_chord,
        );
        comparison.scalar(&format!("{label}.x_le"), geometry.x_le(case.eta), case.x_le);
        comparison.scalar(&format!("{label}.z_le"), geometry.z_le(case.eta), case.z_le);
        let (rvx, rvy) = geometry.rib_vector(case.eta);
        comparison.slice(
            &format!("{label}.rib_vector"),
            &[rvx, rvy],
            &case.rib_vector,
        );
        let (ldx, ldy) = geometry.le_direction(case.eta);
        comparison.slice(
            &format!("{label}.le_direction"),
            &[ldx, ldy],
            &case.le_direction,
        );
    }

    comparison.finish();
}

#[test]
fn rib_lengths_and_spar_intersections_match_the_reference() {
    let fixture: Fixture = alas_testkit::load("geom", "wing_structure");
    let geometry = build_geometry(&fixture);

    let mut comparison = Comparison::new(
        "alas-geom::wing_structure (get_rib_lengths / compute_spar_intersections)",
        Tier::Closed,
    );

    for case in &fixture.rib_lengths_and_spars {
        let label = format!("eta={}", case.eta);
        let (l_nominal, l_actual) =
            geometry.get_rib_lengths(case.y_le_val, case.x_le_val, case.aft_x, case.aft_y);
        comparison.scalar(&format!("{label}.l_nominal"), l_nominal, case.l_nominal);
        comparison.scalar(&format!("{label}.l_actual"), l_actual, case.l_actual);
        comparison.exact(
            &format!("{label}.truncated"),
            &(l_actual < l_nominal - 1e-9),
            &case.truncated,
        );

        let intersections = geometry.compute_spar_intersections(
            case.y_le_val,
            case.x_le_val,
            case.aft_x,
            case.aft_y,
            l_nominal,
        );
        assert_eq!(intersections.len(), case.spar_intersections.len());
        for (index, (&actual, &expected)) in intersections
            .iter()
            .zip(&case.spar_intersections)
            .enumerate()
        {
            compare_option_scalar(
                &mut comparison,
                &format!("{label}.spar_intersections[{index}]"),
                actual,
                expected,
            );
        }
    }

    comparison.finish();
}

#[test]
fn airfoil_zu_zl_matches_the_reference() {
    let fixture: Fixture = alas_testkit::load("geom", "wing_structure");
    let geometry = build_geometry(&fixture);

    let mut comparison = Comparison::new("alas-geom::wing_structure (airfoil_zu_zl)", Tier::Linalg);
    for case in &fixture.airfoil_zu_zl {
        let (zu, zl) = geometry.airfoil_zu_zl(case.eta, case.xc_frac);
        let label = format!("eta={},xc={}", case.eta, case.xc_frac);
        comparison
            .scalar(&format!("{label}.zu"), zu, case.zu)
            .scalar(&format!("{label}.zl"), zl, case.zl);
    }
    comparison.finish();
}

#[test]
fn get_rib_stations_matches_the_reference_end_to_end() {
    let fixture: Fixture = alas_testkit::load("geom", "wing_structure");
    let geometry = build_geometry(&fixture);

    let stations = geometry.get_rib_stations(fixture.config.num_ribs, fixture.config.num_pts_chord);
    assert_eq!(stations.len(), fixture.rib_stations.len());

    let mut closed = Comparison::new(
        "alas-geom::wing_structure (get_rib_stations, non-airfoil fields)",
        Tier::Closed,
    );
    let mut linalg = Comparison::new(
        "alas-geom::wing_structure (get_rib_stations, surface points)",
        Tier::Linalg,
    );

    for (actual, expected) in stations.iter().zip(&fixture.rib_stations) {
        let label = format!("station[{}]", expected.index);
        closed.exact(&format!("{label}.index"), &actual.index, &expected.index);
        closed.scalar(&format!("{label}.eta"), actual.eta, expected.eta);
        closed.scalar(
            &format!("{label}.y_station"),
            actual.y_station,
            expected.y_station,
        );
        closed.exact(
            &format!("{label}.is_full"),
            &actual.is_full,
            &expected.is_full,
        );
        closed.scalar(
            &format!("{label}.frac_actual"),
            actual.frac_actual,
            expected.frac_actual,
        );
        closed.exact(
            &format!("{label}.j_spars"),
            &actual.j_spars,
            &expected.j_spars,
        );
        closed.slice(
            &format!("{label}.rib_dir_xy"),
            &[actual.rib_dir_xy.0, actual.rib_dir_xy.1],
            &expected.rib_dir_xy,
        );

        if actual.extrados.len() != expected.extrados.len() {
            closed.exact(
                &format!("{label}.extrados (point count)"),
                &actual.extrados.len(),
                &expected.extrados.len(),
            );
            continue;
        }
        for (index, (a, e)) in actual.extrados.iter().zip(&expected.extrados).enumerate() {
            linalg.slice(&format!("{label}.extrados[{index}]"), a, e);
        }
        for (index, (a, e)) in actual.intrados.iter().zip(&expected.intrados).enumerate() {
            linalg.slice(&format!("{label}.intrados[{index}]"), a, e);
        }
    }

    closed.finish();
    linalg.finish();
}
