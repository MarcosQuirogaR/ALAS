// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Strength-sized primary wing box against the FLOPS wing group, per preset.
//!
//! The reference-mode reconciliation requires the frozen FLOPS wing total to
//! exceed the sized box; this prints both so a rejection can be read as a
//! number rather than an opaque `structural_sizing` failure.

#![allow(clippy::print_stdout)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{calculate_flops_mass_buildup, ProductMassBuildup};
use alas_mass::wing_reconciliation::sized_primary_wing;

fn main() {
    println!("preset,flops_wing_kg,sized_box_kg,box_over_flops,ulf,dive_speed_m_s,mtow_kg");
    for preset in presets::registry() {
        let Ok(config) = AlasConfig::from_value(&serde_json::json!({"preset": preset.name})) else {
            continue;
        };
        let Ok(plane) = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), true)
        else {
            continue;
        };
        let flops_wing = match calculate_flops_mass_buildup(
            &plane,
            &config.requirements,
            &config.geometry,
            &config.control_surfaces,
            Some(&config.mass_model),
            &config.landing_gear,
            &config.cabin,
        ) {
            Ok(ProductMassBuildup::PureFlops(b)) => b.masses.wing,
            _ => f64::NAN,
        };
        let sized =
            sized_primary_wing(&config, &preset.design_vector, &plane, &config.requirements)
                .map(|(box_mass, _)| box_mass.mass_kg)
                .unwrap_or(f64::NAN);
        println!(
            "{},{:.1},{:.1},{:.3},{},{},{}",
            preset.name,
            flops_wing,
            sized,
            sized / flops_wing,
            config.requirements.ultimate_load_factor,
            config.requirements.dive_speed_m_s,
            config.requirements.mtow_kg
        );
        // Reference-mode reconciliation, step by step.
        let mut sandbox = config.clone();
        sandbox.optimizer.design_space.mode = alas_config::DesignMode::BaselineSandbox;
        let reconciled = alas_mass::wing_reconciliation::reconcile(
            &sandbox,
            &preset.design_vector,
            &plane,
            None,
        );
        println!(
            "   baseline_sandbox reconcile: {:?}",
            reconciled
                .as_ref()
                .map(|r| (
                    r.feedback.primary_mass_kg,
                    r.feedback.secondary_mass_kg,
                    r.feedback.total_wing_mass_kg
                ))
                .map_err(|e| e.to_string())
        );
        let Ok(reference_plane) = AircraftBuilder::new(Some(sandbox.geometry.clone()))
            .build(Some(&preset.design_vector), false)
        else {
            println!("   reference plane build failed");
            continue;
        };
        let reference_box = sized_primary_wing(
            &sandbox,
            &preset.design_vector,
            &reference_plane,
            &sandbox.requirements,
        )
        .map(|(b, _)| b.mass_kg)
        .map_err(|e| e.to_string());
        println!("   reference-plane (no engines) sized box: {reference_box:?}");
        let analysis_mass_model = sandbox.analysis_mass_model(sandbox.requirements.mtow_kg);
        let reference_masses =
            alas_mass::breakdown::run_mass_analysis_with_model_checked_product_with_gear(
                &reference_plane,
                &sandbox.requirements,
                &sandbox.geometry,
                &sandbox.cabin,
                &sandbox.control_surfaces,
                Some(&analysis_mass_model),
                None,
                alas_mass::breakdown::MassCoordinateModel::ReferenceCompatibility,
                &sandbox.landing_gear,
            )
            .map(|(m, c, _)| (m.wing, c.wing))
            .map_err(|e| e.to_string());
        println!("   reference-plane FLOPS wing + centroid: {reference_masses:?}");
    }
}
