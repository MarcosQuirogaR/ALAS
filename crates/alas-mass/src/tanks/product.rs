// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! One tank inventory for search, dispatch and item-level mass ledgers.
//!
//! A registered preset at its own design vector carries the inventory its
//! manufacturer publishes: the published usable volume at the published fuel
//! density. Its registered cells are reconciled onto that volume, so the
//! dispatch limit, the loaded ledger fuel and the balance fuel are one
//! number. A redesigned preset scales the registered cells onto its own spar
//! box ([`FuelTankLayout::resolve_scaled`]); a clean-sheet or custom layout
//! takes the resolved cell volumes.

use alas_config::{presets, AlasConfig, DesignVector, MassModelConfig};
use alas_geom::{aircraft::airplane::Airplane, builder::AircraftBuilder};

use super::{CapacitySource, FuelTank, FuelTankLayout, TankLayoutError};

/// Largest relative difference accepted between a registered layout's cell
/// total and its preset's published usable volume before reconciliation.
///
/// The cells come from AC, TCDS and weight-and-balance manuals while the
/// total comes from the planning document; modification states and rounding
/// between those sources stay within 2 % on every registered preset
/// (`alas_config::preset_fuel_tanks` checks the same band). A larger gap is
/// an unexplained inventory and is rejected, not absorbed.
pub const PUBLISHED_INVENTORY_TOLERANCE: f64 = 0.02;

/// Whether the live tank configuration is the registered reference arrangement.
pub fn uses_registered_tank_layout(config: &AlasConfig) -> bool {
    alas_config::preset_fuel_tanks::layout_for(&config.preset)
        .is_some_and(|reference| reference == config.fuel_tanks)
}

/// Whether `config` still prices fuel at the preset's own model density, so
/// the preset's published fuel figures apply to it.
pub fn uses_preset_fuel(config: &AlasConfig, preset: &presets::AircraftPreset) -> bool {
    let model_density = preset.mass_model.as_ref().map_or_else(
        || MassModelConfig::default().fuel_density_kg_m3,
        |mass| mass.fuel_density_kg_m3,
    );
    config.mass_model.fuel_density_kg_m3 == model_density
}

/// Fuel density the tank inventory is priced at, kg/m^3.
///
/// A preset whose configured density is still its own model density carries
/// the fuel its manufacturer quotes the published usable mass at: the
/// published density (`AircraftReferenceData::fuel_density_kg_l`), else the
/// published usable mass over the published usable volume. Density is a
/// property of the fuel, so a redesigned wing keeps it. A configured density
/// the user changed, or an aircraft with neither figure, uses the configured
/// value.
pub fn inventory_density_kg_m3(config: &AlasConfig) -> f64 {
    let configured = config.mass_model.fuel_density_kg_m3;
    presets::get(&config.preset)
        .ok()
        .filter(|preset| uses_preset_fuel(config, preset))
        .and_then(|preset| {
            let reference = &preset.reference;
            reference.fuel_density_kg_l.or_else(|| {
                reference
                    .usable_fuel_mass_kg
                    .zip(reference.usable_fuel_volume_l)
                    .map(|(kg, litres)| kg / litres)
            })
        })
        .map_or(configured, |kg_l| kg_l * 1_000.0)
}

/// The published usable volume a registered tank arrangement is reconciled
/// onto, L: the published volume, else the published usable mass at the
/// inventory density while that density is the preset's own.
fn published_inventory_l(
    config: &AlasConfig,
    preset: &presets::AircraftPreset,
    density_kg_m3: f64,
) -> Option<f64> {
    preset.reference.usable_fuel_volume_l.or_else(|| {
        preset
            .reference
            .usable_fuel_mass_kg
            .filter(|_| uses_preset_fuel(config, preset))
            .map(|kg| kg / density_kg_m3 * 1.0e3)
    })
}

/// Resolve all installed tanks. A registered arrangement is reconciled onto
/// its preset's published inventory
/// ([`FuelTankLayout::reconciled_to_published_volume`]) on the preset
/// geometry, and a redesigned aircraft scales those reconciled cells onto
/// its own spar box, so the inventory is continuous at the preset design.
/// Custom cell declarations remain authoritative and never inherit a
/// different aircraft's total.
///
/// # Errors
/// Returns invalid layout/policy/geometry evidence without inventing capacity,
/// and [`TankLayoutError::PublishedInventoryMismatch`] when a preset's cells
/// disagree with its published inventory beyond
/// [`PUBLISHED_INVENTORY_TOLERANCE`].
pub fn resolve_product_layout(
    config: &AlasConfig,
    design: &DesignVector,
    plane: &Airplane,
) -> Result<FuelTankLayout, TankLayoutError> {
    let preset = presets::get(&config.preset)
        .ok()
        .filter(|_| uses_registered_tank_layout(config));
    let published_l = preset.and_then(|p| p.reference.usable_fuel_volume_l);
    let density = inventory_density_kg_m3(config);
    let inventory_l = preset.and_then(|p| published_inventory_l(config, p, density));
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
            inventory_l,
        );
    }
    let layout = FuelTankLayout::resolve(
        plane,
        &config.geometry,
        &config.structures,
        &config.fuel_tanks,
        &config.fuel_policy,
        density,
        published_l,
    )?;
    match inventory_l {
        Some(litres) => layout.reconciled_to_published_volume(litres),
        None => Ok(layout),
    }
}

impl FuelTankLayout {
    /// This layout with its volume-scaled tanks multiplied by one factor so
    /// the total usable volume equals `published_l`, priced at the layout
    /// density. A declared-volume tank (a fuselage auxiliary) keeps its
    /// declared volume, as it does on a redesigned wing.
    ///
    /// The factor spreads a sub-tolerance difference (rounding between
    /// sources, or fuel the planning total counts outside the tanks, such as
    /// the A380's 793 L of line and engine fuel) over the cells in
    /// proportion to their volume.
    ///
    /// # Errors
    /// [`TankLayoutError::PublishedInventoryMismatch`] when the cell total
    /// is not finite and positive, differs from `published_l` by more than
    /// [`PUBLISHED_INVENTORY_TOLERANCE`], or leaves no scalable cell.
    pub fn reconciled_to_published_volume(
        &self,
        published_l: f64,
    ) -> Result<Self, TankLayoutError> {
        let litres = |tank: &FuelTank| tank.usable_volume_m3 * 1.0e3;
        let declared = |tank: &FuelTank| matches!(tank.capacity_source, CapacitySource::Declared);
        let cells_l: f64 = self.tanks.iter().map(litres).sum();
        let declared_l: f64 = self
            .tanks
            .iter()
            .filter(|tank| declared(tank))
            .map(litres)
            .sum();
        let factor = (published_l - declared_l) / (cells_l - declared_l);
        let consistent = cells_l.is_finite()
            && published_l.is_finite()
            && published_l > 0.0
            && (cells_l - published_l).abs() <= PUBLISHED_INVENTORY_TOLERANCE * published_l
            && factor.is_finite()
            && factor > 0.0;
        if !consistent {
            return Err(TankLayoutError::PublishedInventoryMismatch {
                cells_l,
                published_l,
            });
        }
        let mut reconciled = self.clone();
        for tank in reconciled.tanks.iter_mut().filter(|tank| !declared(tank)) {
            tank.usable_volume_m3 *= factor;
            tank.usable_capacity_kg = tank.usable_volume_m3 * self.density_kg_m3;
            tank.unusable_kg *= factor;
        }
        Ok(reconciled)
    }
}
