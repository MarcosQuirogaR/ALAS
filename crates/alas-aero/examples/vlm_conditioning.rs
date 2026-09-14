// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What the influence matrix's conditioning does as the mesh is refined.
//!
//! `VlmSystem::solve` already computes a normalized residual and a pivot-ratio
//! proxy for every solve and checks only that they are finite. This example
//! measures them across a mesh grid, so a rejection threshold can be chosen
//! from the separation between meshes that produce physics and meshes that
//! produce a converged-looking non-answer, rather than guessed.

#![allow(clippy::print_stdout)]

use alas_aero::operating_point::OperatingPoint;
use alas_aero::vlm::VlmSystem;
use alas_atmo::Atmosphere;
use alas_config::AlasConfig;
use alas_geom::builder::AircraftBuilder;

fn main() -> Result<(), String> {
    let preset = std::env::args().nth(1).unwrap_or_else(|| "AVE".to_owned());
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
        .map_err(|error| error.to_string())?;
    let design = alas_config::presets::get(&preset)
        .map(|entry| entry.design_vector)
        .map_err(|error| error.to_string())?;
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&design), true)
        .map_err(|error| error.to_string())?;

    let atmo = Atmosphere::new(config.requirements.cruise_altitude_m);
    let velocity = config.requirements.cruise_mach * atmo.speed_of_sound();
    let op = OperatingPoint::new(atmo, velocity, 2.0, 0.0, 0.0, 0.0, 0.0);

    println!("preset,span,chord,panels,cl,normalized_residual,pivot_ratio,minimum_pivot");
    for span in [1_usize, 2, 3, 4, 6, 10] {
        for chord in [1_usize, 2, 4, 8, 16] {
            let Ok(system) = VlmSystem::assemble(&plane, span, chord) else {
                continue;
            };
            if system.panel_count() > 6_000 {
                continue;
            }
            match system.solve(&op) {
                Ok(result) => println!(
                    "{preset},{span},{chord},{},{:.6},{:.6e},{:.6e},{:.6e}",
                    system.panel_count(),
                    result.cl_lift,
                    result.solve_diagnostics.normalized_residual,
                    result.solve_diagnostics.pivot_ratio,
                    result.solve_diagnostics.minimum_pivot,
                ),
                Err(error) => {
                    println!(
                        "{preset},{span},{chord},{},,{error:?}",
                        system.panel_count()
                    );
                }
            }
        }
    }
    Ok(())
}
