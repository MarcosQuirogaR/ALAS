// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The published and modelled nominal fuel capacity of a registered preset.

use alas_config::AlasConfig;

/// The published usable fuel mass of the registered aircraft a reference
/// adaptation redesigns, kg, or `None` in any other mode or when the preset
/// carries no published figure. A clean-sheet study has no such requirement,
/// and the working-default FLOPS capacity is a placeholder, not a source.
pub(super) fn published_fuel(config: &AlasConfig) -> Option<(Option<f64>, Option<f64>)> {
    if config.optimizer.design_space.mode != alas_config::DesignMode::ReferenceAdaptation {
        return None;
    }
    let reference = &alas_config::presets::get(&config.preset).ok()?.reference;
    let positive = |v: Option<f64>| v.filter(|x| x.is_finite() && *x > 0.0);
    Some((
        positive(reference.usable_fuel_mass_kg),
        positive(reference.usable_fuel_volume_l),
    ))
}

/// Modelled usable tank capacity of the registered preset design, kg, for a
/// reference adaptation; `None` in any other mode or when it cannot be
/// resolved. Resolved once per complete configuration
/// ([`crate::mdo::nominal_cache`]): geometry, structures, tank declarations and
/// fuel density all move it.
pub(super) fn nominal_tank_capacity_kg(config: &AlasConfig) -> Option<f64> {
    published_fuel(config)?;
    NOMINAL_TANK_CAPACITY_KG.get_or_resolve(config, |config| {
        let design = alas_config::presets::get(&config.preset)
            .ok()?
            .design_vector;
        let plane = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), false)
            .ok()?;
        crate::mdo::tanks::tank_capacity_kg(config, &plane, &design)
    })
}

/// The cache of [`nominal_tank_capacity_kg`].
pub(super) static NOMINAL_TANK_CAPACITY_KG: crate::mdo::nominal_cache::NominalCache<f64> =
    crate::mdo::nominal_cache::NominalCache::new();
