// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The lumped mass coordinates the product analysis balances on.
//!
//! The reference implementation places every group at a fraction of a
//! length that was chosen for one aircraft. The product analysis places
//! them at the stations the built geometry gives: the integrated wingbox
//! centroid, the tails on their own mean chords, the gear at its nose and
//! main stations, the engines at their nacelles and the fuel where the tank
//! arrangement holds it at the analyzed load. Those are the same stations
//! the item ledger is built from, so the trim anchor, the model envelope
//! and the ledger agree about where the aircraft balances.

use alas_config::design_variables::DesignVector;
use alas_geom::aircraft::airplane::Airplane;
use alas_mass::breakdown::{calculate_physical_cg, MassBreakdown, MassCoordinates};
use alas_mass::stations::component_stations;
use alas_mass::tanks::FuelTankLayout;

use crate::feasibility::tank_reference;

use super::FullAnalysis;

impl FullAnalysis {
    /// Replace the frozen group points with geometry-derived stations and
    /// recompute the centre of gravity, unless this analysis replays the
    /// reference implementation or the configuration keeps the frozen
    /// placement.
    ///
    /// The payload point is kept from `legacy`, where the detailed layout has
    /// already placed it. The fuel point is the centroid of the analyzed
    /// fuel in its tanks; when no tank can be resolved on this geometry the
    /// frozen wing point stands, because a missing tank arrangement is a
    /// reported limitation, not a reason to fail the analysis.
    pub(crate) fn station_coordinates(
        &self,
        design: &DesignVector,
        plane: &Airplane,
        masses: &MassBreakdown,
        legacy: MassCoordinates,
    ) -> Result<(MassCoordinates, [f64; 3]), String> {
        if self.reference_compatibility {
            let cg = calculate_physical_cg(masses, &legacy);
            return Ok((legacy, cg));
        }
        station_coordinates_for(&self.config, design, plane, masses, legacy)
    }
}

/// The product placement for any caller holding a configuration: the
/// geometry-derived stations when the configuration asks for them, the
/// frozen points otherwise.
pub(crate) fn station_coordinates_for(
    config: &alas_config::AlasConfig,
    design: &DesignVector,
    plane: &Airplane,
    masses: &MassBreakdown,
    legacy: MassCoordinates,
) -> Result<(MassCoordinates, [f64; 3]), String> {
    if !config.mass_model.geometric_component_stations {
        let cg = calculate_physical_cg(masses, &legacy);
        return Ok((legacy, cg));
    }
    let stations = component_stations(
        plane,
        &config.geometry,
        &config.requirements,
        &config.mass_model,
        &config.structures,
    )
    .map_err(|error| format!("component stations could not be placed: {error}"))?;
    let fuel_position =
        analyzed_fuel_centroid(config, design, plane, masses).unwrap_or(legacy.fuel);
    let coords = stations.mass_coordinates(masses, legacy.payload, fuel_position);
    let cg = calculate_physical_cg(masses, &coords);
    Ok((coords, cg))
}

/// Centroid of the analyzed fuel load as the tanks hold it, when the
/// arrangement resolves on this geometry and the load is positive.
pub(crate) fn analyzed_fuel_centroid(
    config: &alas_config::AlasConfig,
    design: &DesignVector,
    plane: &Airplane,
    masses: &MassBreakdown,
) -> Option<[f64; 3]> {
    let (density_kg_m3, published_total_l) = tank_reference(config, design);
    let tanks = FuelTankLayout::resolve(
        plane,
        &config.geometry,
        &config.structures,
        &config.fuel_tanks,
        &config.fuel_policy,
        density_kg_m3,
        published_total_l,
    )
    .ok()?;
    let fill_kg = masses
        .physical_fuel_mass_kg()?
        .min(tanks.usable_capacity_kg());
    if fill_kg <= 0.0 {
        return None;
    }
    let centroid = tanks.distribute(fill_kg).ok()?.properties(&tanks).cg_m;
    centroid
        .iter()
        .all(|value| value.is_finite())
        .then_some(centroid)
}
