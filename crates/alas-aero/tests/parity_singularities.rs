// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-aero::singularities` against AeroSandbox's
//! `calculate_induced_velocity_horseshoe`, via
//! `golden/generators/gen_aero_singularities.py`.
//!
//! `docs/PORTING.md` names `Tier::Linalg` for the `alas-aero::asb_vlm` row as
//! a whole; this fixture is one of the two the row's module doc splits out
//! (the kernel standalone, and the full solve), and both are checked at that
//! same tier rather than a tighter one this row's ledger entry does not name.

// This file is itself a test binary, so an unwrap or expect that fails is
// the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::HashMap;

use alas_aero::singularities::calculate_induced_velocity_horseshoe;
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct SingleVortexCase {
    field: [f64; 3],
    left: [f64; 3],
    right: [f64; 3],
    gamma: f64,
    vortex_core_radius: f64,
    trailing_vortex_direction: [f64; 3],
    result: [f64; 3],
}

#[derive(Debug, Deserialize)]
struct MultiVortexCase {
    lefts: Vec<[f64; 3]>,
    rights: Vec<[f64; 3]>,
    gammas: Vec<f64>,
    vortex_core_radius: f64,
    trailing_vortex_direction: [f64; 3],
    field_points: Vec<[f64; 3]>,
    summed_results: Vec<[f64; 3]>,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    single_vortex: HashMap<String, SingleVortexCase>,
    multi_vortex_broadcast: MultiVortexCase,
}

#[test]
fn a_single_horseshoe_matches_aerosandbox_on_every_case() {
    let fixture: Fixture = alas_testkit::load("aero", "singularities");
    let mut comparison = Comparison::new(
        "alas-aero::singularities (calculate_induced_velocity_horseshoe)",
        Tier::Linalg,
    );

    for (name, case) in &fixture.single_vortex {
        let actual = calculate_induced_velocity_horseshoe(
            case.field,
            case.left,
            case.right,
            case.trailing_vortex_direction,
            case.gamma,
            case.vortex_core_radius,
        );
        comparison.slice(name, &actual, &case.result);
    }
    comparison.finish();
}

#[test]
fn multiple_horseshoes_summed_at_a_point_match_aerosandbox() {
    let fixture: Fixture = alas_testkit::load("aero", "singularities");
    let case = &fixture.multi_vortex_broadcast;
    let mut comparison = Comparison::new(
        "alas-aero::singularities (multi-vortex broadcast/sum)",
        Tier::Linalg,
    );

    for (index, &field) in case.field_points.iter().enumerate() {
        let mut summed = [0.0; 3];
        for (vortex_index, &gamma) in case.gammas.iter().enumerate() {
            let contribution = calculate_induced_velocity_horseshoe(
                field,
                case.lefts[vortex_index],
                case.rights[vortex_index],
                case.trailing_vortex_direction,
                gamma,
                case.vortex_core_radius,
            );
            for axis in 0..3 {
                summed[axis] += contribution[axis];
            }
        }
        comparison.slice(
            &format!("field_points[{index}]"),
            &summed,
            &case.summed_results[index],
        );
    }
    comparison.finish();
}
