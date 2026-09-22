// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-struct::sizing` against `alas.physics.structural_sizing`,
//! via `golden/generators/gen_struct_sizing.py`.
//!
//! The buildup is closed-form arithmetic over the (already `green`)
//! `WingStructureGeometry` and the shared load model, so every continuous
//! quantity: station chords, per-spar caps/webs/margins, rib spacing and the
//! mass breakdown, is checked at `Tier::Closed`, matching `docs/PORTING.md`.
//! The rib count and the sizing load-case name are an integer and a string and
//! are checked for exact equality. A `+inf` margin of safety (where the local
//! demand is below 1 N.m) is recorded as JSON `null`; the test maps that back
//! to an infinity check rather than a tolerance.
//!
//! The `WingStructureGeometry` is rebuilt exactly as the green
//! `parity_wing_structure.rs` does: default `DesignVector`/`WingConfig`, the
//! root section via `build_section`, the tip via `AirfoilLibrary::get`, with
//! the resolved spar list taken from the fixture, since `resolve_spar_geometry`
//! is not itself ported. The `StructuresConfig` is rebuilt from a default plus
//! the recorded overrides.

// This file is itself a test binary, so an unwrap or expect that fails is
// the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_config::materials;
use alas_config::{DesignRequirements, DesignVector, StructuresConfig, WingConfig};
use alas_geom::airfoil_library::{build_section, AirfoilLibrary};
use alas_geom::wing_structure::WingStructureGeometry;
use alas_struct::sizing::{size_wingbox_reference_compatibility, WingboxSizing};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;
use serde_json::{Map, Value};

#[derive(Debug, Deserialize)]
struct SparRecord {
    chord_fraction: f64,
    h: Vec<f64>,
    w_cap: Vec<f64>,
    t_cap: Vec<f64>,
    a_cap: Vec<f64>,
    t_web: f64,
    frac_moment: Vec<f64>,
    // `null` marks a `+inf` margin (demand below 1 N.m).
    margin_of_safety: Vec<Option<f64>>,
}

#[derive(Debug, Deserialize)]
struct MassBreakdownRecord {
    #[serde(rename = "Spar caps")]
    spar_caps: f64,
    #[serde(rename = "Spar webs")]
    spar_webs: f64,
    #[serde(rename = "Skin")]
    skin: f64,
    #[serde(rename = "Ribs")]
    ribs: f64,
}

#[derive(Debug, Deserialize)]
struct SizingRecord {
    y_stations: Vec<f64>,
    eta_stations: Vec<f64>,
    chord: Vec<f64>,
    spar_fracs: Vec<f64>,
    spars: Vec<SparRecord>,
    t_skin: f64,
    num_ribs: i64,
    rib_spacing_m: f64,
    mass_breakdown_kg: MassBreakdownRecord,
    total_mass_kg: f64,
    sizing_load_case: String,
}

#[derive(Debug, Deserialize)]
struct MaterialsRecord {
    skin: String,
    web: String,
    cap: String,
    rib: String,
}

#[derive(Debug, Deserialize)]
struct Case {
    name: String,
    config: Map<String, Value>,
    spar_chord_fractions: Vec<f64>,
    spar_full_span: Vec<bool>,
    materials: MaterialsRecord,
    sizing: SizingRecord,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

/// Rebuild a `StructuresConfig` from a default plus the recorded overrides,
/// the same `setattr`-after-default the generator does.
fn structures_config_for(overrides: &Map<String, Value>) -> StructuresConfig {
    let mut cfg = StructuresConfig::default();
    for (key, value) in overrides {
        match key.as_str() {
            "center_spar_enabled" => cfg.center_spar_enabled = value.as_bool().unwrap(),
            "num_ribs_override" => cfg.num_ribs_override = Some(value.as_i64().unwrap()),
            other => panic!("fixture set an unhandled StructuresConfig field: {other}"),
        }
    }
    cfg
}

fn build_geometry(case: &Case) -> WingStructureGeometry {
    let dv = DesignVector::default();
    let wing_cfg = WingConfig::default();
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
        &case.spar_chord_fractions,
        Some(&case.spar_full_span),
    )
    .expect("the fixture's spar list is valid")
}

fn compare_vec(comparison: &mut Comparison, label: &str, actual: &[f64], expected: &[f64]) {
    assert_eq!(actual.len(), expected.len(), "{label}: length");
    for (i, (&a, &e)) in actual.iter().zip(expected).enumerate() {
        comparison.scalar(&format!("{label}[{i}]"), a, e);
    }
}

fn compare_margin(
    label: &str,
    actual: &[f64],
    expected: &[Option<f64>],
    comparison: &mut Comparison,
) {
    assert_eq!(actual.len(), expected.len(), "{label}: length");
    for (i, (&a, e)) in actual.iter().zip(expected).enumerate() {
        match e {
            None => assert!(
                a.is_infinite() && a > 0.0,
                "{label}[{i}]: expected +inf, got {a}"
            ),
            Some(value) => {
                comparison.scalar(&format!("{label}[{i}]"), a, *value);
            }
        }
    }
}

fn compare(comparison: &mut Comparison, name: &str, got: &WingboxSizing, want: &SizingRecord) {
    // Exact discrete outputs.
    assert_eq!(got.num_ribs, want.num_ribs, "{name}: num_ribs");
    assert_eq!(
        got.sizing_load_case, want.sizing_load_case,
        "{name}: sizing_load_case"
    );
    assert_eq!(got.spars.len(), want.spars.len(), "{name}: spar count");

    compare_vec(
        comparison,
        &format!("{name}: y_stations"),
        &got.y_stations,
        &want.y_stations,
    );
    compare_vec(
        comparison,
        &format!("{name}: eta_stations"),
        &got.eta_stations,
        &want.eta_stations,
    );
    compare_vec(
        comparison,
        &format!("{name}: chord"),
        &got.chord,
        &want.chord,
    );
    compare_vec(
        comparison,
        &format!("{name}: spar_fracs"),
        &got.spar_fracs,
        &want.spar_fracs,
    );

    comparison.scalar(&format!("{name}: t_skin"), got.t_skin, want.t_skin);
    comparison.scalar(
        &format!("{name}: rib_spacing_m"),
        got.rib_spacing_m,
        want.rib_spacing_m,
    );
    comparison.scalar(
        &format!("{name}: total_mass_kg"),
        got.total_mass_kg,
        want.total_mass_kg,
    );
    comparison.scalar(
        &format!("{name}: mass.spar_caps"),
        got.mass_breakdown_kg.spar_caps,
        want.mass_breakdown_kg.spar_caps,
    );
    comparison.scalar(
        &format!("{name}: mass.spar_webs"),
        got.mass_breakdown_kg.spar_webs,
        want.mass_breakdown_kg.spar_webs,
    );
    comparison.scalar(
        &format!("{name}: mass.skin"),
        got.mass_breakdown_kg.skin,
        want.mass_breakdown_kg.skin,
    );
    comparison.scalar(
        &format!("{name}: mass.ribs"),
        got.mass_breakdown_kg.ribs,
        want.mass_breakdown_kg.ribs,
    );

    for (i, (gs, ws)) in got.spars.iter().zip(&want.spars).enumerate() {
        let sp = format!("{name}: spar[{i}]");
        comparison.scalar(
            &format!("{sp}.chord_fraction"),
            gs.chord_fraction,
            ws.chord_fraction,
        );
        comparison.scalar(&format!("{sp}.t_web"), gs.t_web, ws.t_web);
        compare_vec(comparison, &format!("{sp}.h"), &gs.h, &ws.h);
        compare_vec(comparison, &format!("{sp}.w_cap"), &gs.w_cap, &ws.w_cap);
        compare_vec(comparison, &format!("{sp}.t_cap"), &gs.t_cap, &ws.t_cap);
        compare_vec(comparison, &format!("{sp}.a_cap"), &gs.a_cap, &ws.a_cap);
        compare_vec(
            comparison,
            &format!("{sp}.frac_moment"),
            &gs.frac_moment,
            &ws.frac_moment,
        );
        compare_margin(
            &format!("{sp}.margin_of_safety"),
            &gs.margin_of_safety,
            &ws.margin_of_safety,
            comparison,
        );
    }
}

#[test]
fn size_wingbox_matches_python_across_structures_config_cases() {
    let fixture: Fixture = alas_testkit::load("struct", "sizing");
    let req = DesignRequirements::default();

    let mut comparison = Comparison::new("alas-struct::sizing::size_wingbox", Tier::Closed);
    for case in &fixture.cases {
        let wsg = build_geometry(case);
        let cfg = structures_config_for(&case.config);
        let skin_mat = materials::get(&case.materials.skin).expect("skin material resolves");
        let web_mat = materials::get(&case.materials.web).expect("web material resolves");
        let cap_mat = materials::get(&case.materials.cap).expect("cap material resolves");
        let rib_mat = materials::get(&case.materials.rib).expect("rib material resolves");

        let sizing = size_wingbox_reference_compatibility(
            &wsg, &cfg, &req, skin_mat, web_mat, cap_mat, rib_mat,
        );
        compare(&mut comparison, &case.name, &sizing, &case.sizing);
    }
    comparison.finish();
}
