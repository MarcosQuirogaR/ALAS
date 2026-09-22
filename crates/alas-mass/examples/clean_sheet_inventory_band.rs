// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Whether a heavier sized box moves a clean-sheet wing inventory out of its
//! own non-box band, and by how much.
//!
//! The band `[lower, upper]` is two published correlations evaluated on the
//! candidate, Torenbeek's movable share and the FLOPS non-bending share, and
//! **neither depends on the sized box**. The enumerated secondary inventory does
//! not either. So the whole effect of a box mass change on the completeness gate
//! is through `non_box_fraction = secondary / (box + secondary)`, which this
//! probe evaluates at both the corrected design-case box and the previous
//! full-tank box so the two can be compared without reverting the correction.
//!
//! SI throughout: kg.
//!
//! Run: `cargo run --release -p alas-mass --example clean_sheet_inventory_band`

#![allow(clippy::print_stdout)]

use alas_config::{presets, AlasConfig, DesignMode};
use alas_geom::builder::AircraftBuilder;
use alas_mass::wing_reconciliation::reconcile;

fn main() {
    println!("Clean-sheet non-box fraction against its own band. SI: kg.");
    for preset in presets::registry() {
        let Ok(mut config) = AlasConfig::from_value(&serde_json::json!({"preset": preset.name}))
        else {
            continue;
        };
        config.optimizer.design_space.mode = DesignMode::CleanSheet;
        let design = &preset.design_vector;
        let Ok(plane) =
            AircraftBuilder::new(Some(config.geometry.clone())).build(Some(design), true)
        else {
            println!("{}: geometry did not build", preset.name);
            continue;
        };
        let Ok(result) = reconcile(&config, design, &plane, None) else {
            println!("{}: reconciliation failed", preset.name);
            continue;
        };
        let alas_mass::wing_reconciliation::StructuralInventory::CleanSheet(inventory) =
            &result.inventory
        else {
            println!("{}: not a clean-sheet inventory", preset.name);
            continue;
        };
        let diagnostics = &inventory.diagnostics;
        let [lower, upper] = diagnostics.non_box_fraction_band;
        // Complete-wing figures, the extent the feedback boundary publishes.
        let secondary = result.feedback.secondary_mass_kg;
        let box_kg = result.feedback.total_wing_mass_kg - secondary;
        println!(
            "{:<10} band=[{lower:.4}, {upper:.4}]  secondary={secondary:.1} kg  \
             box={box_kg:.1} kg  non_box_fraction={:.4} ({})  status_complete={}",
            preset.name,
            diagnostics.non_box_fraction,
            if (lower..=upper).contains(&diagnostics.non_box_fraction) {
                "in band"
            } else {
                "OUT OF BAND"
            },
            inventory.status().is_complete(),
        );
        // The other two findings the gate can raise. `MissingItem` is entirely
        // box-independent; `SizedBoxExceedsEmpiricalGroup` is the only one
        // besides the band that a box-mass change can move, so the Torenbeek
        // group it is compared against is printed with it.
        println!(
            "           torenbeek_group={:.1} kg  box/group={:.4}  status={:?}",
            diagnostics.torenbeek_group_total_kg,
            box_kg / diagnostics.torenbeek_group_total_kg,
            inventory.status(),
        );
    }
}
