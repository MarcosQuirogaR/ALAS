// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The fractions below are written as literals on purpose: they are an
//! independent check of the documented sample (A400M 1.21/5.60, ATR 0.64/2.865,
//! nine jets 0.298-0.546), not of the constants the model reads.
//!
//! The ground plane of an undeclared belly clearance: the two ends of the
//! documented clearance sample of the aircraft class, and one plane for a
//! declared clearance.

// A failed unwrap or expectation is a failed test assertion.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::builder::AircraftBuilder;
use alas_mass::stations::{
    clearance_sample, component_stations_with_gear, ground_clearance_for, ground_plane_bases,
    ground_plane_z_for, ground_plane_z_m, ClearanceSample, GroundPlaneBasis,
};
use serde_json::json;

fn built(name: &str, declared: bool) -> (AlasConfig, Airplane) {
    let mut config =
        AlasConfig::from_value(&json!({ "preset": name })).expect("preset configuration");
    let preset = presets::get(name).expect("registered preset");
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("preset geometry");
    if !declared {
        config.landing_gear.fuselage_ground_clearance_m = None;
    }
    (config, plane)
}

/// The main wing lowered to the belly line: a low-wing layout.
fn low_wing(plane: &mut Airplane) {
    let belly = plane.fuselages[0]
        .xsecs
        .iter()
        .map(|x| x.xyz_c[2] - x.height / 2.0)
        .fold(f64::INFINITY, f64::min);
    for xsec in &mut plane.wings[0].xsecs {
        xsec.xyz_le[2] = belly;
    }
}

/// A jet takes the nine-jet sample ends, a sponson-gear turboprop the
/// two-aircraft sample, and a turboprop of another layout the full documented
/// sample; the small clearance puts the ground higher.
#[test]
fn an_undeclared_clearance_takes_the_ends_of_its_sample() {
    for (name, wing_low, expected, low, high, nominal) in [
        ("A320-200", false, ClearanceSample::Jet, 0.298, 0.546, 0.35),
        (
            "ATR72-600",
            false,
            ClearanceSample::SponsonTurboprop,
            0.216,
            0.223,
            0.22,
        ),
        (
            "ATR72-600",
            true,
            ClearanceSample::OtherTurboprop,
            0.216,
            0.546,
            0.35,
        ),
    ] {
        let (config, mut plane) = built(name, false);
        if wing_low {
            low_wing(&mut plane);
        }
        let fuselage = &plane.fuselages[0];
        let sample = clearance_sample(
            &plane.wings[0],
            fuselage,
            &config.geometry,
            &config.landing_gear,
        );
        assert_eq!(sample, expected, "{name}");
        let d = config.geometry.fuselage.diameter_m;
        let clearance = |basis| {
            ground_clearance_for(
                basis,
                sample,
                fuselage,
                &config.geometry,
                &config.landing_gear,
            )
        };
        assert!(
            (clearance(GroundPlaneBasis::Low) - low * d).abs() < 1e-12,
            "{name}"
        );
        assert!(
            (clearance(GroundPlaneBasis::High) - high * d).abs() < 1e-12,
            "{name}"
        );
        assert!(
            (clearance(GroundPlaneBasis::Nominal) - nominal * d).abs() < 1e-12,
            "{name}"
        );
        let z = |basis| {
            ground_plane_z_for(
                basis,
                sample,
                fuselage,
                &config.geometry,
                &config.landing_gear,
            )
        };
        assert!(
            z(GroundPlaneBasis::Low) > z(GroundPlaneBasis::High),
            "{name}"
        );
        assert_eq!(
            ground_plane_bases(&config.landing_gear),
            &[GroundPlaneBasis::Low, GroundPlaneBasis::High]
        );
    }
}

/// A turboprop outside the sponson layout is judged on the full documented
/// sample instead of being refused: its stations still resolve.
#[test]
fn an_undeclared_low_wing_turboprop_still_resolves_its_stations() {
    let (config, mut plane) = built("ATR72-600", false);
    low_wing(&mut plane);
    component_stations_with_gear(
        &plane,
        &config.geometry,
        &config.requirements,
        &config.mass_model,
        &config.structures,
        &config.landing_gear,
    )
    .expect("the full documented sample stands in, no refusal");
}

/// A declared clearance is the same plane on every basis and one pass.
#[test]
fn a_declared_clearance_is_one_plane_on_every_basis() {
    let (config, plane) = built("ATR72-600", true);
    let fuselage = &plane.fuselages[0];
    let sample = clearance_sample(
        &plane.wings[0],
        fuselage,
        &config.geometry,
        &config.landing_gear,
    );
    let declared = config.landing_gear.fuselage_ground_clearance_m.unwrap();
    for basis in [
        GroundPlaneBasis::Nominal,
        GroundPlaneBasis::Low,
        GroundPlaneBasis::High,
    ] {
        let clearance = ground_clearance_for(
            basis,
            sample,
            fuselage,
            &config.geometry,
            &config.landing_gear,
        );
        assert_eq!(clearance, declared);
    }
    assert_eq!(
        ground_plane_z_for(
            GroundPlaneBasis::High,
            sample,
            fuselage,
            &config.geometry,
            &config.landing_gear
        ),
        ground_plane_z_m(fuselage, &config.geometry, &config.landing_gear)
    );
    assert_eq!(
        ground_plane_bases(&config.landing_gear),
        &[GroundPlaneBasis::Nominal]
    );
}
