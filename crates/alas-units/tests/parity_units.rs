// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares every conversion factor against SUAVE's unit table.
//!
//! The fixture holds every unit name reachable from this program's inputs,
//! collected by reading the mission runner, the weight correlations, the
//! propulsion sizing and the segment solver. Walking all of it is the point:
//! a synonym that was missed would otherwise surface as an aircraft that
//! weighs the wrong amount.
//!
//! Compared at the `closed` tier rather than exactly. The factors here are
//! written as their legal definitions, and SUAVE reaches a few of the same
//! quantities by division -- its inch is a twelfth of its foot -- so the two
//! differ in the last bit or two. That is a difference between an exact
//! definition and a rounded division, not a translation error, and the tier
//! that admits a couple of ulps is the one that says so.

use std::collections::BTreeMap;

use alas_testkit::{Comparison, Tier};

#[test]
fn every_factor_matches_suave() {
    let fixture: BTreeMap<String, BTreeMap<String, f64>> = alas_testkit::load("units", "factors");
    let reference = &fixture["to_base_si"];

    let mut comparison = Comparison::new("alas-units", Tier::Closed);
    for (name, &expected) in reference {
        match alas_units::factor(name) {
            Some(actual) => {
                comparison.scalar(name, actual, expected);
            }
            None => {
                comparison.exact(&format!("{name} (known to this program)"), &false, &true);
            }
        }
    }
    comparison.finish();
}

#[test]
fn the_fixture_covers_the_units_the_models_use() {
    let fixture: BTreeMap<String, BTreeMap<String, f64>> = alas_testkit::load("units", "factors");
    let reference = &fixture["to_base_si"];

    // Not an arbitrary sample: these are the units the transport weight
    // correlations bracket themselves with, and the ones whose absence would
    // make this comparison vacuous.
    for name in [
        "ft",
        "lb",
        "lbs",
        "lbf",
        "force_pound",
        "nmi",
        "knots",
        "degrees",
        "psi",
    ] {
        assert!(
            reference.contains_key(name),
            "the fixture is missing {name}"
        );
    }
}
