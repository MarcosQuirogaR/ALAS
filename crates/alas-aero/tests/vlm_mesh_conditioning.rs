// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A vortex lattice may refuse to answer. It may not answer wrongly.
//!
//! `AnalysisConfig::spanwise_resolution` multiplies a surface the geometry
//! builder has already subdivided, so a large value produces sliver panels
//! whose horseshoe legs approach collinearity. The linear solve stays accurate
//! throughout: the normalized residual sits at machine precision at every
//! mesh measured; while the circulation it returns stops being a flow field:
//! before this was guarded, the A320 at ten spanwise by one chordwise returned
//! a lift coefficient of -2.1e7 with `converged == true` and every downstream
//! finiteness check passing.
//!
//! `alas_config::validation` rejects that configuration range up front. This
//! is the backstop for callers that assemble a [`VlmSystem`] directly, and it
//! pins the property rather than the threshold: whatever the solver decides
//! about a mesh, it must not hand back a lift coefficient no aircraft has.

// This file is itself a test binary, so a failed unwrap is the assertion
// failing rather than library code panicking.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_aero::operating_point::OperatingPoint;
use alas_aero::vlm::VlmSystem;
use alas_atmo::Atmosphere;
use alas_config::AlasConfig;
use alas_geom::builder::AircraftBuilder;

/// Beyond this the number is not a lift coefficient. A transport wing trims
/// near 0.5 and stalls well under 3; the guarded failures overshot by seven
/// orders of magnitude, so this bound needs no precision to be decisive.
const IMPLAUSIBLE_CL: f64 = 5.0;

fn preset_airplane(preset: &str) -> (alas_geom::aircraft::airplane::Airplane, AlasConfig) {
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
        .expect("a registered preset loads");
    let design = alas_config::presets::get(preset)
        .expect("a registered preset resolves")
        .design_vector;
    let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&design), true)
        .expect("the preset geometry builds");
    (airplane, config)
}

#[test]
fn no_mesh_returns_a_lift_coefficient_no_aircraft_has() {
    for preset in ["A320-200", "AVE"] {
        let (airplane, config) = preset_airplane(preset);
        let atmosphere = Atmosphere::new(config.requirements.cruise_altitude_m);
        let velocity = config.requirements.cruise_mach * atmosphere.speed_of_sound();
        let op_point = OperatingPoint::new(atmosphere, velocity, 2.0, 0.0, 0.0, 0.0, 0.0);

        for spanwise in [1_usize, 2, 3, 4, 6, 10] {
            for chordwise in [1_usize, 2, 4, 8] {
                let Ok(system) = VlmSystem::assemble(&airplane, spanwise, chordwise) else {
                    continue;
                };
                // Refusing is a correct outcome; answering absurdly is not.
                if let Ok(result) = system.solve(&op_point) {
                    assert!(
                        result.cl_lift.is_finite() && result.cl_lift.abs() < IMPLAUSIBLE_CL,
                        "{preset} at {spanwise}x{chordwise} returned CL = {} \
                         instead of refusing",
                        result.cl_lift
                    );
                }
            }
        }
    }
}

#[test]
fn the_meshes_the_product_ships_are_all_solvable() {
    // The guard must not be reachable from any shipped configuration: every
    // fidelity preset, on every registered aircraft, has to solve.
    for preset in alas_config::presets::available() {
        let (airplane, config) = preset_airplane(preset);
        let atmosphere = Atmosphere::new(config.requirements.cruise_altitude_m);
        let velocity = config.requirements.cruise_mach * atmosphere.speed_of_sound();
        let op_point = OperatingPoint::new(atmosphere, velocity, 2.0, 0.0, 0.0, 0.0, 0.0);

        for fidelity in alas_config::fidelity_presets::registry() {
            for (spanwise, chordwise) in [
                (
                    fidelity.analysis.spanwise_resolution,
                    fidelity.analysis.chordwise_resolution,
                ),
                (
                    fidelity.analysis.fine_spanwise_resolution,
                    fidelity.analysis.fine_chordwise_resolution,
                ),
            ] {
                let system = VlmSystem::assemble(&airplane, spanwise as usize, chordwise as usize)
                    .unwrap_or_else(|error| {
                        panic!(
                            "{preset}/{}: assembling {spanwise}x{chordwise} failed: {error}",
                            fidelity.name
                        )
                    });
                let result = system.solve(&op_point).unwrap_or_else(|error| {
                    panic!(
                        "{preset}/{}: solving {spanwise}x{chordwise} failed: {error}",
                        fidelity.name
                    )
                });
                assert!(
                    result.cl_lift.is_finite() && result.cl_lift.abs() < IMPLAUSIBLE_CL,
                    "{preset}/{}: {spanwise}x{chordwise} gave CL = {}",
                    fidelity.name,
                    result.cl_lift
                );
            }
        }
    }
}
