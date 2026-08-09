// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-geom::airfoil_data`'s `NAMED_COORDINATES` registry against
//! `golden/geom/airfoil_data.json`, produced from
//! `alas.data.airfoil_data.NAMED_COORDINATES` by
//! `golden/generators/gen_geom_airfoil_data.py`.
//!
//! `exact` tier: this is literal coordinate data transcribed once from the
//! Python source, with no arithmetic on either side for a tolerance to
//! forgive.

use std::collections::HashMap;

use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Fixture {
    named_coordinates: HashMap<String, Vec<(f64, f64)>>,
}

#[test]
fn every_registered_section_matches_the_python_registry_exactly() {
    let fixture: Fixture = alas_testkit::load("geom", "airfoil_data");

    let mut comparison = Comparison::new("alas-geom::airfoil_data", Tier::Exact);
    for (name, expected) in &fixture.named_coordinates {
        match alas_geom::airfoil_data::get(name) {
            Some(actual) => {
                if actual.len() != expected.len() {
                    comparison.exact(
                        &format!("{name} (point count)"),
                        &actual.len(),
                        &expected.len(),
                    );
                    continue;
                }
                for (index, (&(ax, ay), &(ex, ey))) in actual.iter().zip(expected).enumerate() {
                    comparison.scalar(&format!("{name}[{index}].x"), ax, ex);
                    comparison.scalar(&format!("{name}[{index}].y"), ay, ey);
                }
            }
            None => {
                comparison.exact(&format!("{name} (present in the registry)"), &false, &true);
            }
        }
    }
    comparison.finish();
}

#[test]
fn the_registry_has_exactly_the_names_the_fixture_names() {
    let fixture: Fixture = alas_testkit::load("geom", "airfoil_data");
    let names = alas_geom::airfoil_data::names();

    assert_eq!(
        names.len(),
        fixture.named_coordinates.len(),
        "registry has {} entries, the fixture has {}",
        names.len(),
        fixture.named_coordinates.len()
    );
    for name in names {
        assert!(
            fixture.named_coordinates.contains_key(*name),
            "{name} is in the Rust registry but not in the Python fixture"
        );
    }
}

#[test]
fn the_fixture_names_the_reference_supercritical_section() {
    let fixture: Fixture = alas_testkit::load("geom", "airfoil_data");
    // The wing root/break section of the reference aircraft (see
    // alas/data/airfoil_data.py's module docstring); losing this entry from
    // the fixture would silently stop checking the one section this module
    // exists to hold.
    assert!(fixture.named_coordinates.contains_key("SC2-0714"));
}

#[test]
fn a_name_not_in_the_registry_resolves_to_none_on_both_sides() {
    // The fixture only records what NAMED_COORDINATES has; a name it does
    // not have is a property of the lookup, not the fixture, and is checked
    // directly rather than through the fixture.
    assert!(alas_geom::airfoil_data::get("naca2410").is_none());
    assert!(alas_geom::airfoil_data::get("sc2-0714").is_none());
}
