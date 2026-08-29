// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Shared finalization for mass and balance analyses.
//!
//! Keeping payload replacement and fuel recomputation in one private helper
//! prevents the legacy and checked product seams from drifting.

use alas_config::DesignRequirements;

use crate::breakdown::{
    calculate_physical_cg, MassBreakdown, MassCoordinates, PayloadLayoutSummary, OEW_KEYS,
};

pub(crate) fn complete_mass_analysis(
    mut masses: MassBreakdown,
    mut coordinates: MassCoordinates,
    requirements: &DesignRequirements,
    payload_layout: Option<&PayloadLayoutSummary>,
) -> (MassBreakdown, MassCoordinates, [f64; 3]) {
    if let Some(layout) = payload_layout {
        if layout.total_mass > 0.0 {
            let m_oew: f64 = OEW_KEYS
                .iter()
                .map(|&key| masses.get(key).unwrap_or(0.0))
                .sum();
            masses.payload = layout.total_mass;
            masses.fuel = requirements.mtow_kg - (m_oew + masses.payload);
            let z_payload = coordinates.payload[2];
            coordinates.payload = [layout.cg_x, layout.cg_y, z_payload];
        }
    }

    let cg = calculate_physical_cg(&masses, &coordinates);
    (masses, coordinates, cg)
}
