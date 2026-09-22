// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares the three reference-data tables this crate embeds against the
//! ones the reference implementation registers.
//!
//! The tables are the port's one deliberate change of medium: upstream writes
//! them as constructor calls in Python, and here they are JSON. That makes
//! this test do double duty. It checks the numbers agree, and it checks that
//! the copy the crate embeds and the copy under `golden/` have not drifted,
//! two files with the same content is exactly the arrangement where they
//! quietly stop having the same content.
//!
//! Comparison is on parsed, typed values rather than on the raw documents. A
//! field elevation upstream is a Python `int` and here is an `f64`, so the
//! two serialize as `25` and `25.0`; comparing the text would fail on a
//! difference that does not exist, while comparing the numbers catches every
//! difference that does.

// This file is itself a test binary, so an unwrap that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_config::airports::{self, Airport};
use alas_config::engines::{self, EngineSpec};
use alas_config::materials::{self, MaterialSpec};
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Deserialize)]
struct MaterialsFixture {
    materials: Vec<MaterialSpec>,
    available: Vec<String>,
}

#[derive(Deserialize)]
struct EnginesFixture {
    engines: Vec<EngineSpec>,
    available: Vec<String>,
    nacelle_profiles: std::collections::BTreeMap<String, Vec<Vec<f64>>>,
}

#[derive(Deserialize)]
struct AirportsFixture {
    airports: Vec<Airport>,
}

#[test]
fn every_material_matches_the_reference() {
    let fixture: MaterialsFixture = alas_testkit::load("config", "materials");
    let mut comparison = Comparison::new("alas-config::materials", Tier::Exact);

    let embedded = materials::database();
    comparison.exact(
        "registration order",
        &embedded.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
        &fixture
            .materials
            .iter()
            .map(|m| m.name.as_str())
            .collect::<Vec<_>>(),
    );

    for expected in &fixture.materials {
        let Ok(actual) = materials::get(&expected.name) else {
            comparison.exact(&expected.name, &"absent".to_owned(), &"present".to_owned());
            continue;
        };
        let at = |field: &str| format!("{}.{field}", expected.name);
        comparison
            .exact(&at("category"), &actual.category, &expected.category)
            .scalar(&at("e_pa"), actual.e_pa, expected.e_pa)
            .scalar(&at("nu"), actual.nu, expected.nu)
            .scalar(&at("rho_kg_m3"), actual.rho_kg_m3, expected.rho_kg_m3)
            .scalar(&at("f_allow_pa"), actual.f_allow_pa, expected.f_allow_pa);
    }

    comparison.exact(
        "available()",
        &materials::available()
            .iter()
            .map(|&name| name.to_owned())
            .collect::<Vec<_>>(),
        &fixture.available,
    );
    comparison.finish();
}

#[test]
fn every_engine_matches_the_reference() {
    let fixture: EnginesFixture = alas_testkit::load("config", "engines");
    let mut comparison = Comparison::new("alas-config::engines", Tier::Exact);

    let registered = engines::database()
        .iter()
        .map(|e| e.name.as_str())
        .collect::<Vec<_>>();
    for expected in &fixture.engines {
        comparison.exact(
            &format!("reference engine {} remains registered", expected.name),
            &registered.contains(&expected.name.as_str()),
            &true,
        );
    }

    for expected in &fixture.engines {
        let Ok(actual) = engines::get(&expected.name) else {
            comparison.exact(&expected.name, &"absent".to_owned(), &"present".to_owned());
            continue;
        };
        let mut expected = expected.clone();
        if expected.name == "PW1500G" {
            // The family alias resolves to the identity-qualified PW1521G-3;
            // EASA IM.E.090 and ICAO EEDB provenance are tested in engines.rs.
            assert_eq!(
                (
                    expected.thrust_kn,
                    expected.bypass_ratio,
                    expected.overall_pressure_ratio
                ),
                (104.5, 12.0, 35.0)
            );
            expected.thrust_kn = 97.73;
            expected.bypass_ratio = 11.37;
            expected.overall_pressure_ratio = 35.11;
        }
        let at = |field: &str| format!("{}.{field}", expected.name);
        comparison
            .exact(
                &at("manufacturer"),
                &actual.manufacturer,
                &expected.manufacturer,
            )
            .scalar(&at("thrust_kn"), actual.thrust_kn, expected.thrust_kn)
            .scalar(
                &at("fan_diameter_m"),
                actual.fan_diameter_m,
                expected.fan_diameter_m,
            )
            .scalar(
                &at("bypass_ratio"),
                actual.bypass_ratio,
                expected.bypass_ratio,
            )
            .scalar(
                &at("nacelle_length_m"),
                actual.nacelle_length_m,
                expected.nacelle_length_m,
            )
            .scalar(
                &at("nacelle_max_radius_m"),
                actual.nacelle_max_radius_m,
                expected.nacelle_max_radius_m,
            )
            .scalar(
                &at("overall_pressure_ratio"),
                actual.overall_pressure_ratio,
                expected.overall_pressure_ratio,
            )
            .scalar(
                &at("turbine_inlet_temp_k"),
                actual.turbine_inlet_temp_k,
                expected.turbine_inlet_temp_k,
            )
            .scalar(
                &at("fan_pressure_ratio"),
                actual.fan_pressure_ratio,
                expected.fan_pressure_ratio,
            )
            .scalar(
                &at("cruise_tsfc_kg_kgf_hr"),
                actual.cruise_tsfc_kg_kgf_hr,
                expected.cruise_tsfc_kg_kgf_hr,
            );
    }

    let available = engines::available();
    for expected in &fixture.available {
        comparison.exact(
            &format!("reference available engine {expected}"),
            &available.contains(&expected.as_str()),
            &true,
        );
    }
    comparison.finish();
}

#[test]
fn every_nacelle_profile_matches_the_reference() {
    // The silhouette is derived from the engine's length rather than stored,
    // so it is the one part of this table that is computed, and a wrong
    // station fraction draws a plausible nacelle of the wrong shape.
    let fixture: EnginesFixture = alas_testkit::load("config", "engines");
    let mut comparison = Comparison::new("alas-config::engines nacelle profiles", Tier::Closed);

    for (name, expected) in &fixture.nacelle_profiles {
        let engine = engines::get(name).expect("the fixture's engines are all registered");
        let actual = engine.nacelle_profile();
        assert_eq!(
            actual.len(),
            expected.len(),
            "{name}: the profile has {} points, the reference has {}",
            actual.len(),
            expected.len()
        );
        for (index, (&(x, radius), point)) in actual.iter().zip(expected).enumerate() {
            comparison
                .scalar(&format!("{name}[{index}].x"), x, point[0])
                .scalar(&format!("{name}[{index}].radius"), radius, point[1]);
        }
    }
    comparison.finish();
}

#[test]
fn every_airport_matches_the_reference() {
    let fixture: AirportsFixture = alas_testkit::load("config", "airports");
    let mut comparison = Comparison::new("alas-config::airports", Tier::Exact);

    comparison.exact(
        "table order",
        &airports::database()
            .iter()
            .filter(|airport| {
                fixture
                    .airports
                    .iter()
                    .any(|reference| reference.icao == airport.icao)
            })
            .map(|a| a.icao.as_str())
            .collect::<Vec<_>>(),
        &fixture
            .airports
            .iter()
            .map(|a| a.icao.as_str())
            .collect::<Vec<_>>(),
    );

    for expected in &fixture.airports {
        let Ok(actual) = airports::get(&expected.icao) else {
            comparison.exact(&expected.icao, &"absent".to_owned(), &"present".to_owned());
            continue;
        };
        let at = |field: &str| format!("{}.{field}", expected.icao);
        comparison
            .exact(&at("name"), &actual.name, &expected.name)
            .exact(&at("notes"), &actual.notes, &expected.notes)
            .scalar(&at("elevation_m"), actual.elevation_m, expected.elevation_m)
            .scalar(&at("toda_m"), actual.toda_m, expected.toda_m)
            .scalar(&at("lda_m"), actual.lda_m, expected.lda_m)
            .scalar(
                &at("isa_deviation_c"),
                actual.isa_deviation_c,
                expected.isa_deviation_c,
            )
            .scalar(
                &at("latitude_deg"),
                actual.latitude_deg,
                expected.latitude_deg,
            )
            .scalar(
                &at("longitude_deg"),
                actual.longitude_deg,
                expected.longitude_deg,
            );
    }
    comparison.finish();
}

#[derive(Deserialize)]
struct SpecFixture {
    name: String,
    default: f64,
    lower: f64,
    upper: f64,
    unit: String,
    description: String,
    decimals: i64,
}

#[derive(Deserialize)]
struct DesignVariablesFixture {
    specs: Vec<SpecFixture>,
    default_vector: Vec<f64>,
    bounds: Vec<Vec<f64>>,
}

#[test]
fn the_design_space_matches_the_reference() {
    // Order is checked before anything else: a table agreeing entry by entry
    // but not in order would have the optimizer perturbing one variable while
    // the geometry builder read another, with no error raised anywhere.
    let fixture: DesignVariablesFixture = alas_testkit::load("config", "design_variables");
    let mut comparison = Comparison::new("alas-config::design_variables", Tier::Exact);

    comparison.exact(
        "variable order",
        &alas_config::DESIGN_VARIABLE_SPECS
            .iter()
            .map(|spec| spec.name)
            .collect::<Vec<_>>(),
        &fixture
            .specs
            .iter()
            .map(|spec| spec.name.as_str())
            .collect::<Vec<_>>(),
    );

    for (actual, expected) in alas_config::DESIGN_VARIABLE_SPECS
        .iter()
        .zip(&fixture.specs)
    {
        let at = |field: &str| format!("{}.{field}", expected.name);
        comparison
            .exact(&at("unit"), &actual.unit.to_owned(), &expected.unit)
            .exact(
                &at("description"),
                &actual.description.to_owned(),
                &expected.description,
            )
            .exact(&at("decimals"), &actual.decimals, &expected.decimals)
            .scalar(&at("default"), actual.default, expected.default)
            .scalar(&at("lower"), actual.lower, expected.lower)
            .scalar(&at("upper"), actual.upper, expected.upper);
    }

    comparison.slice(
        "the nominal design vector",
        &alas_config::DesignVector::default().to_array(),
        &fixture.default_vector,
    );

    let bounds = alas_config::DesignVector::bounds();
    comparison.slice(
        "bounds, lower",
        &bounds.iter().map(|&(low, _)| low).collect::<Vec<_>>(),
        &fixture
            .bounds
            .iter()
            .map(|pair| pair[0])
            .collect::<Vec<_>>(),
    );
    comparison.slice(
        "bounds, upper",
        &bounds.iter().map(|&(_, high)| high).collect::<Vec<_>>(),
        &fixture
            .bounds
            .iter()
            .map(|pair| pair[1])
            .collect::<Vec<_>>(),
    );
    comparison.finish();
}
