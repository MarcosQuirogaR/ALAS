// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-struct::loads` against `alas/physics/structural_loads.py`,
//! via `golden/generators/gen_struct_loads.py`.
//!
//! Every quantity here is closed-form `f64` arithmetic: the load-case
//! scaling, the closed-form ellipse, and a trapezoidal integral evaluated in
//! the same summation order as the reference's `np.cumsum`, so the whole row
//! is checked at `Tier::Closed`, matching `docs/PORTING.md`.

// This file is itself a test binary, so an unwrap or expect that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_config::{DesignRequirements, EngineConfig, MassModelConfig};
use alas_struct::loads::{
    cantilever_shear_moment, elliptic_distributed_load, engine_point_loads_n, load_cases,
};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ReqFields {
    gravity_m_s2: f64,
    mtow_kg: f64,
    ultimate_load_factor: f64,
    limit_load_factor_neg: f64,
}

#[derive(Debug, Deserialize)]
struct CaseFields {
    name: String,
    load_factor: f64,
    total_force_n: f64,
}

#[derive(Debug, Deserialize)]
struct LoadCasesEntry {
    req: ReqFields,
    additional_safety_factor: f64,
    cases: Vec<CaseFields>,
}

#[derive(Debug, Deserialize)]
struct EllipticEntry {
    y: Vec<f64>,
    semi_span: f64,
    total_force_n: f64,
    q: Vec<f64>,
}

#[derive(Debug, Deserialize)]
struct CantileverEntry {
    y: Vec<f64>,
    q_net: Vec<f64>,
    v: Vec<f64>,
    m: Vec<f64>,
}

#[derive(Debug, Deserialize)]
struct EngineFields {
    thrust_kn: f64,
    spanwise_positions_m: Vec<f64>,
}

#[derive(Debug, Deserialize)]
struct MassFields {
    propulsion_twr_factor: f64,
    propulsion_installation_factor: f64,
}

#[derive(Debug, Deserialize)]
struct EngineEntry {
    engine: EngineFields,
    mass_cfg: MassFields,
    gravity_m_s2: f64,
    result: Vec<[f64; 2]>,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    load_cases: Vec<LoadCasesEntry>,
    elliptic_distributed_load: Vec<EllipticEntry>,
    cantilever_shear_moment: Vec<CantileverEntry>,
    engine_point_loads_n: Vec<EngineEntry>,
}

/// A `DesignRequirements` carrying only the four fields `load_cases` reads,
/// over the crate's defaults: the fixture records exactly those four.
fn requirements_from(fields: &ReqFields) -> DesignRequirements {
    DesignRequirements {
        gravity_m_s2: fields.gravity_m_s2,
        mtow_kg: fields.mtow_kg,
        ultimate_load_factor: fields.ultimate_load_factor,
        limit_load_factor_neg: fields.limit_load_factor_neg,
        ..Default::default()
    }
}

#[test]
fn load_cases_match_the_reference() {
    let fixture: Fixture = alas_testkit::load("struct", "loads");

    let mut comparison = Comparison::new("alas-struct::loads::load_cases", Tier::Closed);
    for (index, entry) in fixture.load_cases.iter().enumerate() {
        let req = requirements_from(&entry.req);
        let actual = load_cases(&req, entry.additional_safety_factor);
        comparison.exact(
            &format!("load_cases[{index}].len"),
            &actual.len(),
            &entry.cases.len(),
        );
        for (case_index, expected) in entry.cases.iter().enumerate() {
            let got = &actual[case_index];
            comparison.exact(
                &format!("load_cases[{index}][{case_index}].name"),
                &got.name,
                &expected.name.as_str(),
            );
            comparison.scalar(
                &format!("load_cases[{index}][{case_index}].load_factor"),
                got.load_factor,
                expected.load_factor,
            );
            comparison.scalar(
                &format!("load_cases[{index}][{case_index}].total_force_n"),
                got.total_force_n,
                expected.total_force_n,
            );
        }
    }
    comparison.finish();
}

#[test]
fn the_elliptic_distributed_load_matches_the_reference() {
    let fixture: Fixture = alas_testkit::load("struct", "loads");

    let mut comparison = Comparison::new(
        "alas-struct::loads::elliptic_distributed_load",
        Tier::Closed,
    );
    for (index, entry) in fixture.elliptic_distributed_load.iter().enumerate() {
        let actual = elliptic_distributed_load(&entry.y, entry.semi_span, entry.total_force_n);
        comparison.slice(&format!("elliptic[{index}].q"), &actual, &entry.q);
    }
    comparison.finish();
}

#[test]
fn cantilever_shear_and_moment_match_the_reference() {
    let fixture: Fixture = alas_testkit::load("struct", "loads");

    let mut comparison =
        Comparison::new("alas-struct::loads::cantilever_shear_moment", Tier::Closed);
    for (index, entry) in fixture.cantilever_shear_moment.iter().enumerate() {
        let (v, m) = cantilever_shear_moment(&entry.y, &entry.q_net);
        comparison.slice(&format!("cantilever[{index}].v"), &v, &entry.v);
        comparison.slice(&format!("cantilever[{index}].m"), &m, &entry.m);
    }
    comparison.finish();
}

#[test]
fn engine_point_loads_match_the_reference() {
    let fixture: Fixture = alas_testkit::load("struct", "loads");

    let mut comparison = Comparison::new("alas-struct::loads::engine_point_loads_n", Tier::Closed);
    for (index, entry) in fixture.engine_point_loads_n.iter().enumerate() {
        let mut engine = EngineConfig {
            spanwise_positions_m: entry.engine.spanwise_positions_m.clone(),
            ..Default::default()
        };
        // Replay the fixture's thrust in the active physics payload, rather
        // than leaving the default GE9X behind a compatibility mirror.
        engine.turbofan.as_mut().unwrap().rated_thrust_kn = entry.engine.thrust_kn;
        let mass_cfg = MassModelConfig {
            propulsion_twr_factor: entry.mass_cfg.propulsion_twr_factor,
            propulsion_installation_factor: entry.mass_cfg.propulsion_installation_factor,
            ..Default::default()
        };
        let req = DesignRequirements {
            gravity_m_s2: entry.gravity_m_s2,
            ..Default::default()
        };

        let actual = engine_point_loads_n(&engine, &mass_cfg, &req);
        comparison.exact(
            &format!("engine[{index}].len"),
            &actual.len(),
            &entry.result.len(),
        );
        for (load_index, expected) in entry.result.iter().enumerate() {
            if let Some(&(y, mass)) = actual.get(load_index) {
                comparison.scalar(&format!("engine[{index}][{load_index}].y"), y, expected[0]);
                comparison.scalar(
                    &format!("engine[{index}][{load_index}].mass"),
                    mass,
                    expected[1],
                );
            }
        }
    }
    comparison.finish();
}
