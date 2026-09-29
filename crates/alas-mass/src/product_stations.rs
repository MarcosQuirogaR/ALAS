// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The one authoritative product placement of the lumped mass groups.
//!
//! [`crate::breakdown::MassCoordinateModel::ReferenceCompatibility`] places
//! every group at a fraction of a length that was chosen for one aircraft.
//! The product analysis instead places them where the built geometry puts
//! them: the integrated wingbox centroid, the tails on their own mean chords,
//! the gear at its nose and main stations, the engines at their nacelles, the
//! payload where the detailed layout seated it and the fuel where the tank
//! arrangement holds it at the analyzed load.
//!
//! This lives here, below both `alas-opt` and `alas-pipeline`, because the
//! optimizer's search-time balance gate and the final report's
//! physical-feasibility stage must decide *the same* aircraft's centre of
//! gravity. If each derived its own placement, the search could balance a
//! candidate with the reference fractions while the report balanced it on the
//! geometric stations, and the two would disagree by several percent of the
//! mean aerodynamic chord for one identical design vector at one identical
//! closed takeoff mass.

use alas_config::design_variables::DesignVector;
use alas_config::{presets, AlasConfig};
use alas_geom::aircraft::airplane::Airplane;

use crate::breakdown::{calculate_physical_cg, MassBreakdown, MassCoordinates};
use crate::stations::component_stations_with_gear;
use crate::tanks::resolve_product_layout;

/// Fuel density and published usable volume for the tank arrangement.
///
/// A registered preset evaluated at its own unmodified design vector keeps
/// its published fuel density and usable volume, because those are measured
/// values for that aircraft. Any other design is a modified or notional one
/// and uses the configured density with a geometry-derived volume.
pub fn tank_reference(config: &AlasConfig, design: &DesignVector) -> (f64, Option<f64>) {
    let configured_density = config.mass_model.fuel_density_kg_m3;
    let Ok(preset) = presets::get(&config.preset) else {
        return (configured_density, None);
    };
    if *design != preset.design_vector {
        return (configured_density, None);
    }
    let density = preset
        .reference
        .fuel_density_kg_l
        .map_or(configured_density, |kg_l| kg_l * 1_000.0);
    (density, preset.reference.usable_fuel_volume_l)
}

/// Centroid of the analyzed fuel load as the tanks hold it, when the
/// arrangement resolves on this geometry and the load is positive.
pub fn analyzed_fuel_centroid(
    config: &AlasConfig,
    design: &DesignVector,
    plane: &Airplane,
    masses: &MassBreakdown,
) -> Option<[f64; 3]> {
    let tanks = resolve_product_layout(config, design, plane).ok()?;
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

/// Replace the fixed group points in `fallback` with the geometry-derived
/// stations and recompute the centre of gravity.
///
/// The payload point is kept from `fallback`, where the detailed layout has
/// already seated it. The fuel point is the centroid of the analyzed fuel in
/// its tanks; when no tank arrangement can be resolved on this geometry the
/// fallback wing point stands, because a missing arrangement is a reported
/// limitation, not a reason to fail the analysis. A configuration that
/// switches the geometric stations off keeps `fallback` unchanged.
///
/// # Errors
///
/// A message describing why the component stations could not be placed on
/// this geometry.
pub fn product_mass_coordinates(
    config: &AlasConfig,
    design: &DesignVector,
    plane: &Airplane,
    masses: &MassBreakdown,
    fallback: MassCoordinates,
) -> Result<(MassCoordinates, [f64; 3]), String> {
    let (coords, cg, _propulsion_station_fallback) =
        product_mass_coordinates_with_diagnostics(config, design, plane, masses, fallback)?;
    Ok((coords, cg))
}

/// [`product_mass_coordinates`], additionally reporting whether the
/// propulsion group's station is the no-nacelle fallback: see
/// [`crate::stations::ComponentStations::propulsion_station_fallback`].
///
/// A design with real propulsion mass but this flag `true` has its engines
/// placed at the wing centroid rather than a measured nacelle station; a
/// consumer that publishes or gates on the returned coordinates (an audit
/// table, a CG verdict) should check this before treating the propulsion
/// centroid as evidence rather than a placeholder.
///
/// # Errors
///
/// A message describing why the component stations could not be placed on
/// this geometry.
pub fn product_mass_coordinates_with_diagnostics(
    config: &AlasConfig,
    design: &DesignVector,
    plane: &Airplane,
    masses: &MassBreakdown,
    fallback: MassCoordinates,
) -> Result<(MassCoordinates, [f64; 3], bool), String> {
    if !config.mass_model.geometric_component_stations {
        let cg = calculate_physical_cg(masses, &fallback);
        return Ok((fallback, cg, false));
    }
    let stations = component_stations_with_gear(
        plane,
        &config.geometry,
        &config.requirements,
        &config.mass_model,
        &config.structures,
        &config.landing_gear,
    )
    .map_err(|error| format!("component stations could not be placed: {error}"))?;
    let fuel_position =
        analyzed_fuel_centroid(config, design, plane, masses).unwrap_or(fallback.fuel);
    let coords = stations.mass_coordinates(masses, fallback.payload, fuel_position);
    let cg = calculate_physical_cg(masses, &coords);
    Ok((coords, cg, stations.propulsion_station_fallback()))
}
