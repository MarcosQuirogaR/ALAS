// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The structural inventory decision, per registered aircraft, as numbers.
//!
//! One row per preset carrying everything the
//! `structural_inventory_unverified` residual is decided on: the frozen
//! empirical wing, the strength-sized complete box, the non-box remainder the
//! reconciliation is left with, the controlling strength margin and whether it
//! is a real deficit or the noise floor of a zero-margin design, and the
//! declared-versus-geometric integral wing fuel that relieves the sizing case.
//!
//! SI throughout: kg, m, N.m.
//!
//! Run: `cargo run --release -p alas-mass --example structural_inventory_matrix`

#![allow(clippy::print_stdout)]

use alas_config::{presets, AlasConfig, DesignMode};
use alas_geom::builder::AircraftBuilder;
use alas_geom::wing_structure::WingStructureGeometry;
use alas_mass::breakdown::{calculate_flops_mass_buildup, ProductMassBuildup};
use alas_mass::wing_reconciliation::{reconcile, sized_primary_wing, StructuralInventory};
use alas_struct::sizing::{box_chord_band, sizing_stations, MARGIN_NUMERICAL_ZERO};

fn trapezoid(values: &[f64], stations: &[f64]) -> f64 {
    let mut acc = 0.0;
    for index in 0..values.len().saturating_sub(1) {
        acc += (stations[index + 1] - stations[index]) * (values[index + 1] + values[index]) / 2.0;
    }
    acc
}

fn main() {
    println!("MARGIN_NUMERICAL_ZERO = {MARGIN_NUMERICAL_ZERO:.6e} (4 x f64::EPSILON)");
    println!();
    for preset in presets::registry() {
        let Ok(mut config) = AlasConfig::from_value(&serde_json::json!({"preset": preset.name}))
        else {
            println!("{}: configuration did not load", preset.name);
            continue;
        };
        config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
        let design = &preset.design_vector;
        let Ok(plane) =
            AircraftBuilder::new(Some(config.geometry.clone())).build(Some(design), true)
        else {
            println!("{}: geometry did not build", preset.name);
            continue;
        };

        let empirical = match calculate_flops_mass_buildup(
            &plane,
            &config.requirements,
            &config.geometry,
            &config.control_surfaces,
            Some(&config.mass_model),
            &config.landing_gear,
            &config.cabin,
        ) {
            Ok(ProductMassBuildup::PureFlops(buildup)) => buildup.masses.wing,
            _ => f64::NAN,
        };

        let sized = sized_primary_wing(&config, design, &plane, &config.requirements);
        let reconciled = reconcile(&config, design, &plane, None);

        println!("== {} ==", preset.name);
        match (&sized, &reconciled) {
            (Ok((_, sizing)), Ok(result)) => {
                let complete_box = 2.0 * sizing.total_mass_kg;
                let remainder = result.feedback.secondary_mass_kg;
                let controlling = sizing.controlling_margin();
                let margin = sizing.minimum_margin_of_safety();
                let inventory = match &result.inventory {
                    StructuralInventory::FrozenReference => "FrozenReference (complete)".to_owned(),
                    StructuralInventory::ReferenceExceededBySizedBox {
                        reference_total_kg,
                        sized_box_kg,
                    } => format!(
                        "ReferenceExceededBySizedBox (INCOMPLETE) by {:.1} kg (+{:.1} %)",
                        sized_box_kg - reference_total_kg,
                        100.0 * (sized_box_kg / reference_total_kg - 1.0)
                    ),
                    StructuralInventory::CleanSheet(_) => "CleanSheet".to_owned(),
                };
                println!(
                    "  empirical_wing_kg={empirical:.1} complete_box_kg={complete_box:.1} \
                     box_over_wing={:.4} non_box_remainder_kg={remainder:.1} ({:.1} %)",
                    complete_box / empirical,
                    100.0 * remainder / empirical
                );
                println!("  inventory: {inventory}");
                println!(
                    "  min_margin={margin:.6e} strength_margins_pass={} numerical_zero={} \
                     controlling=(spar {}, station {}, eta {:.4})",
                    sizing.strength_margins_pass(),
                    sizing.controlling_margin_is_numerical_zero(),
                    controlling.map_or(usize::MAX, |c| c.spar_index),
                    controlling.map_or(usize::MAX, |c| c.station_index),
                    controlling.map_or(f64::NAN, |c| c.eta),
                );
                println!(
                    "  caps={:.1} webs={:.1} skin={:.1} ribs={:.1} semi_total={:.1} kg",
                    sizing.mass_breakdown_kg.spar_caps,
                    sizing.mass_breakdown_kg.spar_webs,
                    sizing.mass_breakdown_kg.skin,
                    sizing.mass_breakdown_kg.ribs,
                    sizing.total_mass_kg,
                );
            }
            (Err(error), _) => println!("  sizing failed: {error}"),
            (_, Err(error)) => println!("  reconciliation failed: {error}"),
        }

        // The relieving fuel, declared against geometric, on the same grid the
        // sizer used.
        let (Some(root), Some(tip)) = (
            plane
                .wings
                .iter()
                .find(|wing| wing.name == "Main Wing")
                .and_then(|wing| wing.xsecs.first()),
            plane
                .wings
                .iter()
                .find(|wing| wing.name == "Main Wing")
                .and_then(|wing| wing.xsecs.last()),
        ) else {
            continue;
        };
        let (fractions, full_span) = config.structures.resolved_spars();
        let Ok(wsg) = WingStructureGeometry::new(
            design,
            &config.geometry.wing,
            &root.airfoil,
            &tip.airfoil,
            &fractions,
            Some(&full_span),
        ) else {
            continue;
        };
        let stations = sizing_stations(&wsg, &config.structures);
        let (front, rear) = box_chord_band(&wsg);
        let geometric =
            alas_struct::tanks::integral_fuel_running_mass_kg_m(&wsg, &stations, front, rear);
        let geometric_kg = trapezoid(&geometric, &stations);
        let engines = alas_struct::loads::engine_point_loads_n(
            &config.geometry.engine,
            &config.mass_model,
            &config.requirements,
        );
        println!(
            "  relieving fuel kg/semi-wing: geometric={geometric_kg:.0}  \
             wing-mounted point masses: {:?}",
            engines
                .iter()
                .map(|&(y, mass)| ((y * 100.0).round() / 100.0, mass.round()))
                .collect::<Vec<_>>()
        );
    }
}
