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
use alas_mass::product_stations::product_mass_coordinates;

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

/// The product placement for any caller holding a configuration.
///
/// This is a thin seam over [`alas_mass::product_stations`], which is where
/// the placement itself lives so the optimizer's search-time balance gate
/// evaluates the same stations this report does.
pub(crate) fn station_coordinates_for(
    config: &alas_config::AlasConfig,
    design: &DesignVector,
    plane: &Airplane,
    masses: &MassBreakdown,
    legacy: MassCoordinates,
) -> Result<(MassCoordinates, [f64; 3]), String> {
    product_mass_coordinates(config, design, plane, masses, legacy)
}
