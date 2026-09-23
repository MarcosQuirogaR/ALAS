// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-struct::nastran`'s solution decks against
//! `alas.integration.nastran_runner`, via
//! `golden/generators/gen_struct_nastran.py`.
//!
//! These decks are written card by card rather than assembled through a model,
//! so unlike the mesh they are compared as *text*: the fixture holds each
//! deck's lines and the test holds the port to producing the same ones. That is
//! the strongest claim available here and the right one: a case-control line
//! that differs by a character is a different solve, not a rounding difference.
//!
//! One family of lines is exempt, and it is the same seam the mesh row draws.
//! A `FORCE` card's magnitude distributes a load case's total over the mesh's
//! front-spar node line, so it is arithmetic over grid coordinates rather than
//! a copied constant; those lines are parsed and their magnitude compared at
//! [`Tier::Closed`], with the card's set, grid and direction still compared
//! exactly. Eight significant digits of `%.8g` would otherwise turn a
//! last-ulp difference into a text mismatch.
//!
//! The formatter every card goes through is checked directly, against a table
//! of values chosen to reach each of its branches, because a fixture built from
//! this program's own configuration would only ever exercise one of them.

// This file is itself a test binary, so an unwrap or expect that fails is
// the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod support;

use alas_config::{DesignRequirements, EngineConfig, MassModelConfig};
use alas_struct::mesh::build_wing_mesh_bdf;
use alas_struct::nastran::{
    build_sol101_bulk, build_sol103_bulk, build_sol111_random_bulk, build_sol111_sine_bulk,
    monitor_set,
};
use alas_struct::sizing::size_wingbox_reference_compatibility;
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;
use serde_json::{Map, Value};
use support::{build_geometry, materials_for, structures_config_for, MaterialsRecord};

/// The mesh include path the decks are written against, which the generator
/// passes and the runner will.
const MESH_INCLUDE: &str = "../wing_mesh.bdf";

#[derive(Debug, Deserialize)]
struct MonitorRecord {
    root: i64,
    kink: i64,
    engine: i64,
    tip: i64,
}

#[derive(Debug, Deserialize)]
struct LoadCaseRecord {
    name: String,
    load_factor: f64,
    total_force_n: f64,
}

#[derive(Debug, Deserialize)]
struct DeckRecord {
    sol101: Vec<String>,
    sol103: Vec<String>,
    sol111_sine: Vec<String>,
    sol111_random: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Case {
    name: String,
    config: Map<String, Value>,
    spar_chord_fractions: Vec<f64>,
    spar_full_span: Vec<bool>,
    materials: MaterialsRecord,
    monitors: MonitorRecord,
    /// `[grid, span]` along the front spar's upper line.
    front_spar_nid_y: Vec<(i64, f64)>,
    semi_span_m: f64,
    load_cases: Vec<LoadCaseRecord>,
    decks: DeckRecord,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    cases: Vec<Case>,
    /// `[value, rendered]` for the free-field float formatter.
    format_samples: Vec<(f64, String)>,
}

/// Split a `FORCE` card into its exact fields and its magnitude.
///
/// `FORCE,sid,grid,cid,magnitude,0.,0.,1.`, everything but field four is a
/// copied constant, so only field four is a number that has to be compared as
/// one.
fn split_force(line: &str) -> Option<(Vec<&str>, f64)> {
    let fields: Vec<&str> = line.split(',').collect();
    if fields.first() != Some(&"FORCE") || fields.len() != 8 {
        return None;
    }
    let magnitude = fields[4].parse().ok()?;
    let mut exact: Vec<&str> = Vec::with_capacity(7);
    exact.extend_from_slice(&fields[..4]);
    exact.extend_from_slice(&fields[5..]);
    Some((exact, magnitude))
}

fn compare_deck(
    exact: &mut Comparison,
    numeric: &mut Comparison,
    label: &str,
    got: &str,
    want: &[String],
) {
    let lines: Vec<&str> = got.lines().collect();
    exact.exact(&format!("{label}: line count"), &lines.len(), &want.len());
    for (index, (line, reference)) in lines.iter().zip(want).enumerate() {
        match (split_force(line), split_force(reference)) {
            (Some((got_fields, got_magnitude)), Some((want_fields, want_magnitude))) => {
                exact.exact(&format!("{label}[{index}] card"), &got_fields, &want_fields);
                numeric.scalar(
                    &format!("{label}[{index}] magnitude"),
                    got_magnitude,
                    want_magnitude,
                );
            }
            _ => {
                exact.exact(
                    &format!("{label}[{index}]"),
                    &(*line).to_string(),
                    reference,
                );
            }
        }
    }
}

#[test]
fn the_free_field_formatter_renders_what_python_renders() {
    let fixture: Fixture = alas_testkit::load("struct", "nastran");
    let mut comparison = Comparison::new("alas-struct::nastran (free-field format)", Tier::Exact);
    for (value, expected) in &fixture.format_samples {
        let rendered = alas_struct::nastran::free_field(*value);
        comparison.exact(&format!("_f({value:e})"), &rendered, expected);
    }
    comparison.finish();
}

#[test]
fn the_solution_decks_match_python_line_for_line() {
    let fixture: Fixture = alas_testkit::load("struct", "nastran");
    // The frozen Python fixture was generated with the historical 9.81 m/s^2
    // gravity; production requirements use standard gravity (9.80665 m/s^2).
    let req = DesignRequirements {
        gravity_m_s2: 9.81,
        ..DesignRequirements::default()
    };
    let engine_cfg = EngineConfig::default();
    let mass_cfg = MassModelConfig::default();

    let mut exact = Comparison::new("alas-struct::nastran (deck text)", Tier::Exact);
    let mut numeric = Comparison::new("alas-struct::nastran (force magnitudes)", Tier::Closed);

    for case in &fixture.cases {
        let wsg = build_geometry(&case.spar_chord_fractions, &case.spar_full_span);
        let mut cfg = structures_config_for(&case.config);
        // The frozen Python generator's implicit StructuresConfig band is
        // 500 Hz and its implicit modal count is 30, while the product
        // defaults are intentionally bounded for an interactive run. Make
        // reference-default cases explicit here so this deck parity test
        // exercises the configured contract (including the random-only
        // branch) instead of silently comparing two default policies. The
        // dynamics/coarse-step cases already carry explicit values.
        if !case.config.contains_key("freq_sweep_max_hz") {
            cfg.freq_sweep_max_hz = 500.0;
        }
        if !case.config.contains_key("n_modes") {
            cfg.n_modes = 30;
        }
        let [skin, web, cap, rib] = materials_for(&case.materials);
        let sizing = size_wingbox_reference_compatibility(&wsg, &cfg, &req, skin, web, cap, rib);
        let (deck, _, node_index) = build_wing_mesh_bdf(
            &wsg,
            &sizing,
            &cfg,
            &engine_cfg,
            &mass_cfg,
            &req,
            skin,
            web,
            cap,
            rib,
        )
        .expect("the nominal wingbox meshes without a fatal health finding");

        // The mesh-derived inputs the decks read, checked before the decks
        // themselves so a disagreement says which of the two moved.
        let monitors = monitor_set(&node_index);
        let label = &case.name;
        exact.exact(
            &format!("{label}: monitor.root"),
            &monitors.root,
            &case.monitors.root,
        );
        exact.exact(
            &format!("{label}: monitor.kink"),
            &monitors.kink,
            &case.monitors.kink,
        );
        exact.exact(
            &format!("{label}: monitor.engine"),
            &monitors.engine,
            &case.monitors.engine,
        );
        exact.exact(
            &format!("{label}: monitor.tip"),
            &monitors.tip,
            &case.monitors.tip,
        );

        let front_upper = node_index.spar_upper_nids.first().expect("a front spar");
        let grids: Vec<i64> = case.front_spar_nid_y.iter().map(|&(nid, _)| nid).collect();
        exact.exact(&format!("{label}: front spar grids"), front_upper, &grids);
        for &(nid, y) in &case.front_spar_nid_y {
            numeric.scalar(&format!("{label}: y[{nid}]"), deck.node_y(nid), y);
        }
        numeric.scalar(
            &format!("{label}: semi span"),
            deck.node_y(node_index.tip_nid),
            case.semi_span_m,
        );

        let cases = alas_struct::loads::load_cases(&req, cfg.additional_safety_factor);
        exact.exact(
            &format!("{label}: load case count"),
            &cases.len(),
            &case.load_cases.len(),
        );
        for (load_case, want) in cases.iter().zip(&case.load_cases) {
            exact.exact(
                &format!("{label}: case name"),
                &load_case.name.to_string(),
                &want.name,
            );
            numeric.scalar(
                &format!("{label}: {} load factor", want.name),
                load_case.load_factor,
                want.load_factor,
            );
            numeric.scalar(
                &format!("{label}: {} total force", want.name),
                load_case.total_force_n,
                want.total_force_n,
            );
        }

        for (solution, got, want) in [
            (
                "sol101",
                build_sol101_bulk(&deck, &node_index, &req, &cfg, MESH_INCLUDE),
                &case.decks.sol101,
            ),
            (
                "sol103",
                build_sol103_bulk(&cfg, MESH_INCLUDE),
                &case.decks.sol103,
            ),
            (
                "sol111_sine",
                build_sol111_sine_bulk(&cfg, &node_index, MESH_INCLUDE),
                &case.decks.sol111_sine,
            ),
            (
                "sol111_random",
                build_sol111_random_bulk(&cfg, &node_index, MESH_INCLUDE),
                &case.decks.sol111_random,
            ),
        ] {
            compare_deck(
                &mut exact,
                &mut numeric,
                &format!("{label}: {solution}"),
                &got,
                want,
            );
        }
    }

    exact.finish();
    numeric.finish();
}
