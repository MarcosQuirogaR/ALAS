// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Boundary-condition choices for the OpenFOAM airfoil template.

use serde::{Deserialize, Serialize};

use super::config::FarFieldCondition;

/// Routine boundary-condition choices exposed by the CFD window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BoundarySettings {
    /// Inlet/freestream patch name from the versioned template.
    pub inlet_patch: String,
    /// Outlet patch name from the versioned template.
    pub outlet_patch: String,
    /// Airfoil wall patch name from the generated STL.
    pub airfoil_patch: String,
    /// Far-field treatment.
    pub far_field: FarFieldCondition,
    /// Static pressure reference in Pa. The incompressible `p` field stores
    /// kinematic pressure, so the dictionary value is `p_ref / rho`.
    pub pressure_reference_pa: f64,
}

impl Default for BoundarySettings {
    fn default() -> Self {
        Self {
            inlet_patch: "inlet".to_owned(),
            outlet_patch: "outlet".to_owned(),
            airfoil_patch: "airfoil".to_owned(),
            far_field: FarFieldCondition::FixedValue,
            pressure_reference_pa: 0.0,
        }
    }
}
