// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-mass::breakdown` against `alas.physics.mass`, via
//! `golden/generators/gen_mass_breakdown.py`.
//!
//! The fixture runs `run_mass_analysis` on the frozen-reference
//! `AircraftBuilder::new_reference_compatibility(GeometryConfig()).build`
//! aircraft (with engines, so the nacelle branch of `define_mass_coordinates`
//! is reached), so comparing against it exercises `calculate_component_masses`,
//! `define_mass_coordinates` and `calculate_physical_cg` together on the real
//! built plane rather than a synthetic probe. Each case spreads the buildup
//! over `DesignRequirements`/`MassModelConfig` overrides and the
//! payload-layout branch; the plane and `GeometryConfig` stay at their
//! defaults throughout, matching the generator.
//!
//! Every quantity here is closed-form `f64` arithmetic over the (already
//! `green`) built geometry -- component mass fractions, empirical Torenbeek
//! weights, mass-weighted centroids -- so the whole row is checked at
//! `Tier::Closed`, matching `docs/PORTING.md`.

// This file is itself a test binary, so an unwrap or expect that fails is
// the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::BTreeMap;

use alas_config::{DesignRequirements, GeometryConfig, MassModelConfig};
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{run_mass_analysis, PayloadLayoutSummary};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;
use serde_json::{Map, Value};

#[derive(Debug, Deserialize)]
struct PayloadLayoutRecord {
    total_mass: f64,
    cg_x: f64,
    cg_y: f64,
}

#[derive(Debug, Deserialize)]
struct Case {
    name: String,
    requirements: Map<String, Value>,
    mass_model: Map<String, Value>,
    payload_layout: Option<PayloadLayoutRecord>,
    masses: BTreeMap<String, f64>,
    coords: BTreeMap<String, [f64; 3]>,
    cg: [f64; 3],
}

#[derive(Debug, Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

/// Apply the recorded overrides onto a default `DesignRequirements`, the same
/// `setattr`-after-default the generator does. An unrecognized key is a
/// fixture the test does not know how to reproduce, so it panics rather than
/// silently ignoring it.
fn requirements_for(overrides: &Map<String, Value>) -> DesignRequirements {
    let mut req = DesignRequirements::default();
    for (key, value) in overrides {
        match key.as_str() {
            "mtow_kg" => req.mtow_kg = value.as_f64().unwrap(),
            "num_passengers" => req.num_passengers = value.as_i64().unwrap(),
            "aircraft_type" => req.aircraft_type = value.as_str().unwrap().to_owned(),
            "cargo_payload_kg" => req.cargo_payload_kg = value.as_f64().unwrap(),
            other => panic!("fixture set an unhandled DesignRequirements field: {other}"),
        }
    }
    req
}

fn mass_model_for(overrides: &Map<String, Value>) -> MassModelConfig {
    let mut mm = MassModelConfig::default();
    for (key, value) in overrides {
        match key.as_str() {
            "suspended_mass_fraction" => mm.suspended_mass_fraction = value.as_f64().unwrap(),
            "systems_mass_fraction" => mm.systems_mass_fraction = value.as_f64().unwrap(),
            "furnishings_mass_fraction" => mm.furnishings_mass_fraction = value.as_f64().unwrap(),
            "cabin_payload_density_kg_m" => mm.cabin_payload_density_kg_m = value.as_f64().unwrap(),
            other => panic!("fixture set an unhandled MassModelConfig field: {other}"),
        }
    }
    mm
}

#[test]
fn run_mass_analysis_matches_python_across_requirements_and_layout_overrides() {
    let fixture: Fixture = alas_testkit::load("mass", "breakdown");

    // The frozen parity aircraft, built once.  The product builder owns a
    // newer transport-planform default; this fixture must replay the geometry
    // used by the Python evidence rather than silently comparing two aircraft.
    let builder = AircraftBuilder::new_reference_compatibility(Some(GeometryConfig::default()));
    let plane = builder
        .build(None, true)
        .expect("the nominal aircraft builds");
    let geometry = &builder.geometry;

    let mut comparison = Comparison::new("alas-mass::breakdown::run_mass_analysis", Tier::Closed);
    for case in &fixture.cases {
        let requirements = requirements_for(&case.requirements);
        let mass_model = mass_model_for(&case.mass_model);
        let layout = case.payload_layout.as_ref().map(|l| PayloadLayoutSummary {
            total_mass: l.total_mass,
            cg_x: l.cg_x,
            cg_y: l.cg_y,
        });

        let (masses, coords, cg) = run_mass_analysis(
            &plane,
            &requirements,
            geometry,
            Some(&mass_model),
            layout.as_ref(),
        );

        for (name, mass) in masses.as_pairs() {
            let expected = case
                .masses
                .get(name)
                .unwrap_or_else(|| panic!("case {} has no mass for {name}", case.name));
            comparison.scalar(&format!("{}: mass[{name}]", case.name), mass, *expected);
        }

        for (name, xyz) in coords.as_pairs() {
            let expected = case
                .coords
                .get(name)
                .unwrap_or_else(|| panic!("case {} has no coordinate for {name}", case.name));
            for (axis, label) in ["x", "y", "z"].iter().enumerate() {
                comparison.scalar(
                    &format!("{}: coord[{name}].{label}", case.name),
                    xyz[axis],
                    expected[axis],
                );
            }
        }

        for (axis, label) in ["x", "y", "z"].iter().enumerate() {
            comparison.scalar(
                &format!("{}: cg.{label}", case.name),
                cg[axis],
                case.cg[axis],
            );
        }
    }
    comparison.finish();
}
