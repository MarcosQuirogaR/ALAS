// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-aero::operating_point` against AeroSandbox's
//! `OperatingPoint`, via `golden/generators/gen_aero_operating_point.py`.
//!
//! Everything checked here is closed-form `f64` arithmetic: dot products,
//! a handful of sines and cosines, a 3x3 rotation product, so the whole
//! row is compared at `Tier::Closed`, the tier `docs/PORTING.md` names for
//! it.

// This file is itself a test binary, so an unwrap or expect that fails is
// the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::HashMap;

use alas_aero::operating_point::{AxisFrame, OperatingPoint};
use alas_atmo::{Atmosphere, GAS_CONSTANT_AIR};
use alas_testkit::reference_ambient::reference_ambient;
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Inputs {
    altitude_m: f64,
    velocity: f64,
    alpha: f64,
    beta: f64,
    p: f64,
    q: f64,
    r: f64,
}

#[derive(Debug, Deserialize)]
struct ConvertAxesCase {
    from_axes: String,
    to_axes: String,
    vector: [f64; 3],
    result: [f64; 3],
}

#[derive(Debug, Deserialize)]
struct Case {
    inputs: Inputs,
    dynamic_pressure: f64,
    freestream_velocity_geometry_axes: [f64; 3],
    rotation_velocity_geometry_axes: Vec<[f64; 3]>,
    convert_axes: Vec<ConvertAxesCase>,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    cases: HashMap<String, Case>,
    points: Vec<[f64; 3]>,
}

fn axis_frame(name: &str) -> AxisFrame {
    match name {
        "geometry" => AxisFrame::Geometry,
        "body" => AxisFrame::Body,
        "wind" => AxisFrame::Wind,
        "stability" => AxisFrame::Stability,
        other => panic!("unknown axis frame in fixture: {other}"),
    }
}

/// The standard atmosphere at the case altitude with the temperature that
/// gives the density the reference's fitted atmosphere returned there.
///
/// The fixture's dynamic pressure was produced with that fitted atmosphere,
/// which differs from the U.S. Standard Atmosphere, 1976 the product
/// evaluates by up to 1.7 % in density. The reference's state at the altitude
/// is read from `golden/atmo/differentiable.json`; the model is driven with
/// its density through the pressure of the level and a temperature deviation,
/// so the comparison is of `q = rho V^2 / 2` and not of the two atmospheres.
/// Pressure and density are tied by the gas constant of the product
/// (287.0528742 J/(kg K) against the reference's 287.0530720), which is why
/// the density, not the pressure, is the matched quantity: `q` follows from it
/// exactly.
fn atmosphere_with_reference_density(altitude_m: f64) -> Atmosphere {
    let reference = reference_ambient(altitude_m);
    let level = Atmosphere::new(altitude_m);
    let temperature = level.pressure() / (GAS_CONSTANT_AIR * reference.density_kg_m3);
    level.with_temperature_deviation(temperature - level.temperature())
}

fn build_point(inputs: &Inputs) -> OperatingPoint {
    OperatingPoint::new(
        atmosphere_with_reference_density(inputs.altitude_m),
        inputs.velocity,
        inputs.alpha,
        inputs.beta,
        inputs.p,
        inputs.q,
        inputs.r,
    )
}

#[test]
fn dynamic_pressure_and_freestream_velocity_match_aerosandbox() {
    let fixture: Fixture = alas_testkit::load("aero", "operating_point");
    let mut comparison = Comparison::new(
        "alas-aero::operating_point (dynamic_pressure, freestream_velocity_geometry_axes)",
        Tier::Closed,
    );

    for (name, case) in &fixture.cases {
        let point = build_point(&case.inputs);
        comparison.scalar(
            &format!("{name}.dynamic_pressure"),
            point.dynamic_pressure(),
            case.dynamic_pressure,
        );
        comparison.slice(
            &format!("{name}.freestream_velocity_geometry_axes"),
            &point.freestream_velocity_geometry_axes(),
            &case.freestream_velocity_geometry_axes,
        );
    }
    comparison.finish();
}

#[test]
fn rotation_velocity_at_every_sample_point_matches_aerosandbox() {
    let fixture: Fixture = alas_testkit::load("aero", "operating_point");
    let mut comparison = Comparison::new(
        "alas-aero::operating_point (rotation_velocity_geometry_axes)",
        Tier::Closed,
    );

    for (name, case) in &fixture.cases {
        let point = build_point(&case.inputs);
        let actual = point.rotation_velocity_geometry_axes(&fixture.points);
        if actual.len() != case.rotation_velocity_geometry_axes.len() {
            comparison.exact(
                &format!("{name}.rotation_velocity_geometry_axes (count)"),
                &actual.len(),
                &case.rotation_velocity_geometry_axes.len(),
            );
            continue;
        }
        for (index, (a, e)) in actual
            .iter()
            .zip(&case.rotation_velocity_geometry_axes)
            .enumerate()
        {
            comparison.slice(
                &format!("{name}.rotation_velocity_geometry_axes[{index}]"),
                a,
                e,
            );
        }
    }
    comparison.finish();
}

#[test]
fn convert_axes_matches_aerosandbox_on_every_reached_pair() {
    let fixture: Fixture = alas_testkit::load("aero", "operating_point");
    let mut comparison = Comparison::new("alas-aero::operating_point (convert_axes)", Tier::Closed);

    for (name, case) in &fixture.cases {
        let point = build_point(&case.inputs);
        for (index, entry) in case.convert_axes.iter().enumerate() {
            let from_axes = axis_frame(&entry.from_axes);
            let to_axes = axis_frame(&entry.to_axes);
            let (x, y, z) = point.convert_axes(
                entry.vector[0],
                entry.vector[1],
                entry.vector[2],
                from_axes,
                to_axes,
            );
            comparison.slice(
                &format!(
                    "{name}.convert_axes[{index}] ({} -> {})",
                    entry.from_axes, entry.to_axes
                ),
                &[x, y, z],
                &entry.result,
            );
        }
    }
    comparison.finish();
}
