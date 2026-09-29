// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! One tank inventory for search, dispatch and item-level mass ledgers.

use alas_config::{presets, AlasConfig, DesignVector};
use alas_geom::{aircraft::airplane::Airplane, builder::AircraftBuilder};

use super::{FuelTankLayout, TankLayoutError};

/// Whether the live tank configuration is the registered reference arrangement.
pub fn uses_registered_tank_layout(config: &AlasConfig) -> bool {
    alas_config::preset_fuel_tanks::layout_for(&config.preset)
        .is_some_and(|reference| reference == config.fuel_tanks)
}

/// Resolve all installed tanks, scaling reference cells only for an unchanged
/// registered arrangement on a redesigned aircraft. Custom cell declarations
/// remain authoritative and never inherit a different aircraft's total.
///
/// # Errors
/// Returns invalid layout/policy/geometry evidence without inventing capacity.
pub fn resolve_product_layout(
    config: &AlasConfig,
    design: &DesignVector,
    plane: &Airplane,
) -> Result<FuelTankLayout, TankLayoutError> {
    let preset = presets::get(&config.preset)
        .ok()
        .filter(|_| uses_registered_tank_layout(config));
    let published_l = preset.and_then(|p| p.reference.usable_fuel_volume_l);
    let density = config.mass_model.fuel_density_kg_m3;
    if let Some(preset) = preset.filter(|p| p.design_vector != *design) {
        let reference = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), false)
            .map_err(|error| {
                TankLayoutError::InvalidConfig(format!("reference tank geometry: {error}"))
            })?;
        return FuelTankLayout::resolve_scaled(
            plane,
            &reference,
            &config.geometry,
            &config.structures,
            &config.fuel_tanks,
            &config.fuel_policy,
            density,
            published_l,
        );
    }
    FuelTankLayout::resolve(
        plane,
        &config.geometry,
        &config.structures,
        &config.fuel_tanks,
        &config.fuel_policy,
        density,
        published_l,
    )
}
