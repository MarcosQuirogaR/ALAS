// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Usable fuel-tank capacity for a mission-sized candidate.

use alas_config::design_variables::DesignVector;
use alas_config::{presets, AlasConfig};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::builder::AircraftBuilder;
use alas_mass::tanks::FuelTankLayout;

/// Usable fuel-tank capacity on `plane`, in kilograms, or `None` when the
/// configured tank arrangement cannot be resolved on the built geometry.
///
/// A registered preset's published volumes are honoured as they stand only
/// when `design` still equals the preset's own design vector: the same
/// rule `alas_pipeline::feasibility::assess_fuel_capacity` applies to the
/// finalist report; `alas-opt` cannot depend on `alas-pipeline`, so the
/// small preset lookup is reproduced here. For any other design the
/// published cells are carried onto the candidate as per-cell factors
/// against the preset's own geometry
/// ([`FuelTankLayout::resolve_scaled`]), so a wider or thicker wing gains
/// the tank volume its spar box actually offers instead of keeping a typed
/// litre count.
pub(crate) fn tank_capacity_kg(
    config: &AlasConfig,
    plane: &Airplane,
    design: &DesignVector,
) -> Option<f64> {
    let preset = presets::get(&config.preset).ok();
    let published_l = preset.and_then(|preset| preset.reference.usable_fuel_volume_l);
    let reference = preset
        .filter(|preset| preset.design_vector != *design)
        .and_then(|preset| {
            AircraftBuilder::new(Some(config.geometry.clone()))
                .build(Some(&preset.design_vector), false)
                .ok()
        });
    let layout = match reference {
        Some(reference) => FuelTankLayout::resolve_scaled(
            plane,
            &reference,
            &config.geometry,
            &config.structures,
            &config.fuel_tanks,
            &config.fuel_policy,
            config.mass_model.fuel_density_kg_m3,
            published_l,
        ),
        None => FuelTankLayout::resolve(
            plane,
            &config.geometry,
            &config.structures,
            &config.fuel_tanks,
            &config.fuel_policy,
            config.mass_model.fuel_density_kg_m3,
            published_l,
        ),
    };
    layout.ok().map(|layout| layout.usable_capacity_kg())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::DesignVector;

    /// A candidate whose span exceeds the preset's grows tank capacity with
    /// its spar box, and the preset's own design keeps its published total.
    #[test]
    fn a_wider_wing_gains_capacity_and_the_preset_keeps_its_published_total() {
        let preset = presets::get("A320-200").unwrap_or_else(|error| panic!("{error}"));
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset.name }))
            .unwrap_or_else(|error| panic!("{error}"));
        let reference_plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), false)
            .unwrap_or_else(|error| panic!("{error}"));
        let reference_kg = tank_capacity_kg(&config, &reference_plane, &preset.design_vector)
            .unwrap_or_else(|| panic!("the preset arrangement resolves"));
        let published_kg = preset.reference.usable_fuel_volume_l.unwrap_or(f64::NAN)
            * 1.0e-3
            * config.mass_model.fuel_density_kg_m3;
        assert!((reference_kg - published_kg).abs() < 0.02 * published_kg);

        let mut wider: DesignVector = preset.design_vector;
        wider.span_m *= 1.10;
        wider.root_chord_m *= 1.10;
        let wider_plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&wider), false)
            .unwrap_or_else(|error| panic!("{error}"));
        let wider_kg = tank_capacity_kg(&config, &wider_plane, &wider)
            .unwrap_or_else(|| panic!("the scaled arrangement resolves"));
        assert!(
            wider_kg > reference_kg,
            "{wider_kg} kg vs {reference_kg} kg"
        );
    }
}
