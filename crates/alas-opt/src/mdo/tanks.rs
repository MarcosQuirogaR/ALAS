// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Usable fuel-tank capacity for a mission-sized candidate.
//!
//! One inventory bounds every dispatch, the sizing closure's and the full
//! analysis' alike ([`usable_fuel_capacity`]), and is the inventory the
//! item-level ledgers load: the product tank layout
//! ([`alas_mass::tanks::resolve_product_layout`]). An unchanged registered
//! preset carries its published usable volume at its published fuel
//! density; every other design the capacity of its resolved tank layout.

use alas_config::design_variables::DesignVector;
use alas_config::{presets, AlasConfig};
use alas_geom::aircraft::airplane::Airplane;
use alas_mass::tanks::{uses_preset_fuel, uses_registered_tank_layout};

/// The evidence a usable capacity rests on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsableCapacityBasis {
    /// The manufacturer's published usable volume and fuel density of an
    /// unchanged registered preset.
    PublishedPreset,
    /// The resolved tank layout on the built geometry: a changed preset or a
    /// clean-sheet design.
    ResolvedLayout,
}

/// A usable fuel capacity and its evidence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UsableFuelCapacity {
    /// Usable fuel, kg: the product tank layout's usable capacity.
    pub kg: f64,
    /// Where the value comes from.
    pub basis: UsableCapacityBasis,
}

/// Usable fuel-tank capacity of the product tank layout on `plane`, in
/// kilograms, or `None` when the configured tank arrangement cannot be
/// resolved on the built geometry, contradicts its published inventory, or
/// resolves to no positive finite volume.
///
/// The shared product resolver preserves registered cell-volume calibration
/// on redesigned geometry and honors custom declarations without restoring
/// a deleted tank. Item-level balance uses the same resolver.
pub(crate) fn tank_capacity_kg(
    config: &AlasConfig,
    plane: &Airplane,
    design: &DesignVector,
) -> Option<f64> {
    alas_mass::tanks::resolve_product_layout(config, design, plane)
        .ok()
        .map(|layout| layout.usable_capacity_kg())
        .filter(|kg| kg.is_finite() && *kg > 0.0)
}

/// The usable fuel capacity a dispatch of `design` on `plane` under
/// `config` is bounded by: the product tank layout's, whose basis is the
/// published inventory for an unchanged registered preset (its design
/// vector, its registered tank arrangement, its own fuel density and a
/// published usable volume or mass).
/// `None` when the layout cannot be resolved.
pub fn usable_fuel_capacity(
    config: &AlasConfig,
    design: &DesignVector,
    plane: &Airplane,
) -> Option<UsableFuelCapacity> {
    let kg = tank_capacity_kg(config, plane, design)?;
    let published = presets::get(&config.preset).is_ok_and(|preset| {
        *design == preset.design_vector
            && uses_registered_tank_layout(config)
            && uses_preset_fuel(config, preset)
            && (preset.reference.usable_fuel_volume_l.is_some()
                || preset.reference.usable_fuel_mass_kg.is_some())
    });
    Some(UsableFuelCapacity {
        kg,
        basis: if published {
            UsableCapacityBasis::PublishedPreset
        } else {
            UsableCapacityBasis::ResolvedLayout
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::presets;
    use alas_config::DesignVector;
    use alas_geom::builder::AircraftBuilder;
    use alas_mass::tanks::{resolve_product_layout, TankLayoutError};

    /// Every registered preset at its own design is bounded by one tank
    /// inventory: the dispatch limit is the published usable volume at the
    /// published density, and the ledger tanks load exactly that mass. A
    /// load one kilogram above it is rejected rather than clipped.
    #[test]
    fn dispatch_ledger_and_balance_share_one_inventory_on_every_preset() {
        for preset in presets::registry() {
            let name = preset.name;
            let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            let plane = AircraftBuilder::new(Some(config.geometry.clone()))
                .build(Some(&preset.design_vector), false)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            let dispatch = usable_fuel_capacity(&config, &preset.design_vector, &plane)
                .unwrap_or_else(|| panic!("{name}: the preset inventory resolves"));
            let tanks = resolve_product_layout(&config, &preset.design_vector, &plane)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            assert_eq!(dispatch.kg, tanks.usable_capacity_kg(), "{name}");
            if let (Some(litres), Some(kg_l)) = (
                preset.reference.usable_fuel_volume_l,
                preset.reference.fuel_density_kg_l,
            ) {
                assert_eq!(
                    dispatch.basis,
                    UsableCapacityBasis::PublishedPreset,
                    "{name}"
                );
                let published_kg = litres * kg_l;
                assert!(
                    (dispatch.kg - published_kg).abs() < 1.0e-9 * published_kg,
                    "{name}: {} kg against {published_kg} kg published",
                    dispatch.kg
                );
            }
            // The quoted usable mass is the same inventory rounded to the
            // kilogram at source.
            if let Some(quoted_kg) = preset.reference.usable_fuel_mass_kg {
                assert_eq!(dispatch.basis, UsableCapacityBasis::PublishedPreset);
                assert!(
                    (dispatch.kg - quoted_kg).abs() <= 0.5,
                    "{name}: {} kg against {quoted_kg} kg quoted",
                    dispatch.kg
                );
            }
            let loaded_kg = tanks
                .loadable_fuel_kg(dispatch.kg)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            let state = tanks
                .distribute(loaded_kg)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            assert!(
                (state.total_kg() - dispatch.kg).abs() < 1.0e-9 * dispatch.kg,
                "{name}"
            );
            assert!(matches!(
                tanks.loadable_fuel_kg(dispatch.kg + 1.0),
                Err(TankLayoutError::Overflow { .. })
            ));
            // A redesign scales the same reconciled cells, so the inventory
            // is continuous at the preset design.
            let mut nearby = preset.design_vector;
            nearby.span_m *= 1.0 + 1.0e-6;
            let nearby_plane = AircraftBuilder::new(Some(config.geometry.clone()))
                .build(Some(&nearby), false)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            let nearby_kg = usable_fuel_capacity(&config, &nearby, &nearby_plane)
                .unwrap_or_else(|| panic!("{name}: the nearby inventory resolves"))
                .kg;
            assert!(
                (nearby_kg - dispatch.kg).abs() < 1.0e-4 * dispatch.kg,
                "{name}: {nearby_kg} kg beside {} kg",
                dispatch.kg
            );
        }
    }

    /// The A220-300 publishes 21,504.92 L at 0.8089 kg/L (17,395 kg) while
    /// its registered cells total 21,508 L: the 3 L rounding difference is
    /// reconciled onto the published volume, and a published volume 3 %
    /// away from the cells is an unexplained inventory and is rejected.
    #[test]
    fn a_preset_inventory_outside_the_source_tolerance_is_rejected() {
        let preset = presets::get("A220-300").unwrap_or_else(|error| panic!("{error}"));
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset.name }))
            .unwrap_or_else(|error| panic!("{error}"));
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), false)
            .unwrap_or_else(|error| panic!("{error}"));
        let tanks = resolve_product_layout(&config, &preset.design_vector, &plane)
            .unwrap_or_else(|error| panic!("{error}"));
        assert!((tanks.density_kg_m3 - 808.9).abs() < 1.0e-9);
        assert!((tanks.usable_capacity_kg() - 21_504.92 * 0.8089).abs() < 1.0e-6);
        assert!(matches!(
            tanks.reconciled_to_published_volume(1.03 * 21_504.92),
            Err(TankLayoutError::PublishedInventoryMismatch { .. })
        ));
    }

    /// A candidate whose span exceeds the preset's grows tank capacity with
    /// its spar box.
    #[test]
    fn a_wider_wing_gains_capacity() {
        let preset = presets::get("A320-200").unwrap_or_else(|error| panic!("{error}"));
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset.name }))
            .unwrap_or_else(|error| panic!("{error}"));
        let reference_plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), false)
            .unwrap_or_else(|error| panic!("{error}"));
        let reference_kg = tank_capacity_kg(&config, &reference_plane, &preset.design_vector)
            .unwrap_or_else(|| panic!("the preset arrangement resolves"));

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
