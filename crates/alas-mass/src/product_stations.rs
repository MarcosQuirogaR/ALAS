// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The one authoritative product placement of the lumped mass groups.
//!
//! [`crate::breakdown::MassCoordinateModel::ReferenceCompatibility`] places
//! every group at a fraction of a length that was chosen for one aircraft.
//! The product analysis instead places them where the built geometry puts
//! them: the sized box plus secondary wing first moment, the tails on their own mean chords,
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
use crate::stations::{component_stations_with_gear, ComponentStations, StationError};
use crate::tanks::resolve_product_layout;
use crate::wing_reconciliation::{size_design_wing_box, DesignWingBox};

mod wing;

/// Why the shared product stations could not be placed.
#[derive(Debug, thiserror::Error)]
pub enum ProductStationError {
    /// A geometric station, including a required gear datum, is unavailable.
    #[error("component stations could not be placed: {0}")]
    Component(#[from] StationError),
    /// The structural box used in the wing first moment could not be sized.
    #[error("wing station could not be sized: {0}")]
    WingSizing(#[from] crate::wing_reconciliation::WingReconciliationError),
    /// The complete-wing first moment could not be resolved.
    #[error("{0}")]
    WingPlacement(String),
}

/// Shared stations for the product lumped groups and item ledger.
///
/// The wing position is the sized box plus the current complete-wing mass's
/// non-box remainder. Its mass must be the authoritative current buildup,
/// rather than the separately reconciled structural diagnostic total.
///
/// # Errors
///
/// [`ProductStationError`] when geometry or wing sizing is unavailable.
pub fn product_component_stations(
    config: &AlasConfig,
    design: &DesignVector,
    plane: &Airplane,
    masses: &MassBreakdown,
) -> Result<ComponentStations, ProductStationError> {
    product_component_stations_sharing_box(config, design, plane, masses, None)
}

/// [`product_component_stations`] with the box the caller already sized by
/// [`size_design_wing_box`] on the same configuration, design and aircraft.
///
/// # Errors
///
/// [`ProductStationError`] when geometry is unavailable.
pub fn product_component_stations_with_box(
    config: &AlasConfig,
    design: &DesignVector,
    plane: &Airplane,
    masses: &MassBreakdown,
    design_box: &DesignWingBox,
) -> Result<ComponentStations, ProductStationError> {
    product_component_stations_sharing_box(config, design, plane, masses, Some(design_box))
}

fn product_component_stations_sharing_box(
    config: &AlasConfig,
    design: &DesignVector,
    plane: &Airplane,
    masses: &MassBreakdown,
    design_box: Option<&DesignWingBox>,
) -> Result<ComponentStations, ProductStationError> {
    let mut stations = component_stations_with_gear(
        plane,
        &config.geometry,
        &config.requirements,
        &config.mass_model,
        &config.structures,
        &config.landing_gear,
    )?;
    stations.wing.position_m = complete_wing_centroid(config, design, plane, masses, design_box)?;
    stations.wing.method = "sized box + complete-wing non-box remainder";
    Ok(stations)
}

/// The complete-wing first moment on the shared box, sized here only when the
/// caller holds none. Both paths size at [`size_design_wing_box`]'s inputs.
fn complete_wing_centroid(
    config: &AlasConfig,
    design: &DesignVector,
    plane: &Airplane,
    masses: &MassBreakdown,
    design_box: Option<&DesignWingBox>,
) -> Result<[f64; 3], ProductStationError> {
    let design_box = match design_box {
        Some(shared) => *shared,
        None => size_design_wing_box(config, design, plane)?,
    };
    wing::complete_group_centroid(config, plane, masses.wing, design_box.primary)
        .map_err(ProductStationError::WingPlacement)
}

/// Fuel density and published usable volume for the tank arrangement.
///
/// The density is the tank inventory's
/// ([`crate::tanks::inventory_density_kg_m3`]), so the fuel that relieves the
/// wing and the fuel loaded into the tanks are one mass. A registered preset
/// evaluated at its own unmodified design vector also keeps its published
/// usable volume, a measured value for that aircraft; any other design is a
/// modified or notional one with a geometry-derived volume.
pub fn tank_reference(config: &AlasConfig, design: &DesignVector) -> (f64, Option<f64>) {
    let density = crate::tanks::inventory_density_kg_m3(config);
    let published_l = presets::get(&config.preset)
        .ok()
        .filter(|preset| *design == preset.design_vector)
        .and_then(|preset| preset.reference.usable_fuel_volume_l);
    (density, published_l)
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
    let fill_kg = tanks
        .loadable_fuel_kg(masses.physical_fuel_mass_kg()?)
        .ok()?;
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
/// switches the geometric stations off keeps the other fallback groups;
/// the complete-wing first moment remains shared with the item ledger.
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
    mass_coordinates_sharing_box(config, design, plane, masses, fallback, None)
}

/// [`product_mass_coordinates`] with the box the caller already sized by
/// [`size_design_wing_box`] on the same configuration, design and aircraft,
/// so one evaluation sizes its primary structure once.
///
/// # Errors
///
/// A message describing why the component stations could not be placed on
/// this geometry.
pub fn product_mass_coordinates_with_box(
    config: &AlasConfig,
    design: &DesignVector,
    plane: &Airplane,
    masses: &MassBreakdown,
    fallback: MassCoordinates,
    design_box: &DesignWingBox,
) -> Result<(MassCoordinates, [f64; 3]), String> {
    let (coords, cg, _propulsion_station_fallback) =
        mass_coordinates_sharing_box(config, design, plane, masses, fallback, Some(design_box))?;
    Ok((coords, cg))
}

fn mass_coordinates_sharing_box(
    config: &AlasConfig,
    design: &DesignVector,
    plane: &Airplane,
    masses: &MassBreakdown,
    fallback: MassCoordinates,
    design_box: Option<&DesignWingBox>,
) -> Result<(MassCoordinates, [f64; 3], bool), String> {
    if !config.mass_model.geometric_component_stations {
        let mut coords = fallback;
        coords.wing = complete_wing_centroid(config, design, plane, masses, design_box)
            .map_err(|error| error.to_string())?;
        let cg = calculate_physical_cg(masses, &coords);
        return Ok((coords, cg, false));
    }
    let stations =
        product_component_stations_sharing_box(config, design, plane, masses, design_box)
            .map_err(|error| error.to_string())?;
    let fuel_position =
        analyzed_fuel_centroid(config, design, plane, masses).unwrap_or(fallback.fuel);
    let coords = stations.mass_coordinates(masses, fallback.payload, fuel_position);
    let cg = calculate_physical_cg(masses, &coords);
    Ok((coords, cg, stations.propulsion_station_fallback()))
}
