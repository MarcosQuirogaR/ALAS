// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Does every operating-empty mass this crate can produce agree?
//!
//! The ATR 72-600 has been reported at four different operating empty masses
//! across the wave's evidence. This probe asks the narrower question this crate
//! can answer on its own: given one configuration and one built aircraft, do
//! the operating empty masses reachable through `alas-mass`'s own entry points
//! agree to the kilogram?
//!
//! Two masses and two seat counts are compared per aircraft:
//!
//! * **lumped**, the eight-slot `MassBreakdown` less payload and fuel, which
//!   is what the weight-and-balance artifact publishes;
//! * **FLOPS groups**, the same run's pure-FLOPS component buildup, summed
//!   over the groups the production architecture produced;
//! * **`pax_req`**, `requirements.num_passengers`, the count the cabin and
//!   mission paths read;
//! * **`pax_flops`**, the first + business + tourist class counts the FLOPS
//!   occupant operating items are actually priced on.
//!
//! The two seat counts are the ATR 72-600's 70-versus-72 question asked inside
//! this crate. Any disagreement here is a defect in `alas-mass`. Agreement here
//! moves the question to the consumers, which resolve their own cabin layout
//! and may rewrite `num_passengers` before the mass path ever sees it.
//!
//! SI: kg. Run:
//! `cargo run --release -p alas-mass --example oew_path_parity`

#![allow(clippy::print_stdout)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{run_product_mass_analysis_with_groups, MassCoordinateModel};

fn main() {
    println!(
        "{:<12} {:>12} {:>12} {:>12} {:>10} {:>10}",
        "preset", "lumped_kg", "flops_kg", "spread_kg", "pax_req", "pax_flops"
    );
    for preset in presets::registry() {
        let Ok(config) = AlasConfig::from_value(&serde_json::json!({"preset": preset.name})) else {
            continue;
        };
        let Ok(plane) = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), true)
        else {
            continue;
        };
        let analysis_mass_model = config.analysis_mass_model(config.requirements.mtow_kg);
        let Ok((masses, _, _, groups)) = run_product_mass_analysis_with_groups(
            &plane,
            &config.requirements,
            &config.geometry,
            &config.cabin,
            &config.control_surfaces,
            Some(&analysis_mass_model),
            None,
            MassCoordinateModel::StructuralWingbox(&config.structures),
            &config.landing_gear,
        ) else {
            println!("{:<12} analysis failed", preset.name);
            continue;
        };

        // The lumped operating empty mass: every slot except payload and fuel.
        let lumped: f64 = masses
            .as_pairs()
            .into_iter()
            .filter(|(name, _)| *name != "Payload" && *name != "Fuel")
            .map(|(_, mass_kg)| mass_kg.max(0.0))
            .sum();

        // The same run's FLOPS buildup, where the production architecture
        // produced one.
        let (flops_kg, pax_flops) = groups.as_ref().map_or((f64::NAN, usize::MAX), |buildup| {
            (
                buildup
                    .masses
                    .as_pairs()
                    .into_iter()
                    .filter(|(name, _)| *name != "Payload" && *name != "Fuel")
                    .map(|(_, mass_kg)| mass_kg.max(0.0))
                    .sum(),
                config
                    .mass_model
                    .flops_transport
                    .tourist_class_passenger_count
                    .unwrap_or(0)
                    + config
                        .mass_model
                        .flops_transport
                        .business_class_passenger_count
                        .unwrap_or(0)
                    + config
                        .mass_model
                        .flops_transport
                        .first_class_passenger_count
                        .unwrap_or(0),
            )
        });

        let spread = if flops_kg.is_finite() {
            (lumped - flops_kg).abs()
        } else {
            f64::NAN
        };
        println!(
            "{:<12} {:>12.1} {:>12.1} {:>12.3} {:>10} {:>10}",
            preset.name, lumped, flops_kg, spread, config.requirements.num_passengers, pax_flops,
        );
    }
}
