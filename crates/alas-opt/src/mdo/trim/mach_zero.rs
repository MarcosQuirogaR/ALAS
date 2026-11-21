// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Mach-zero end of the drag table's induced-drag blend (A1G).
//!
//! The table is flown at every Mach of the mission, while the stretched curve
//! is the cruise Mach's: the same stretched path at `beta = 1` (the physical
//! lattice with the same airframe moment) is kept too, for the table to blend
//! toward at low speed. It starts from the cruise attitude and every node
//! re-trims, so it is the trimmed incompressible state, not the cruise one on
//! another lattice.

use std::sync::Arc;

use alas_aero::analysis::{AeroAnalysis, TrimPoint};
use alas_geom::aircraft::airplane::Airplane;
use alas_stab::trim::StabilityTrimResult;

use crate::mdo::drag_table::{DesignTrim, StretchedBasis};

/// The cruise state and the settings the Mach-zero reference is built from.
pub(super) struct MachZeroInputs<'a> {
    /// The physical (unstretched) candidate at its trim centre of gravity.
    pub plane: &'a Airplane,
    /// The converged trim, for its Jacobian.
    pub trim: StabilityTrimResult,
    /// The compressible cruise attitude the nodes start from.
    pub cruise_attitude: TrimPoint,
    /// The airframe moment slope the trim solved against, per degree.
    pub cm_body_per_deg: f64,
    pub cruise_mach: f64,
    pub cruise_altitude_m: f64,
    pub cl_max_clean: f64,
    pub cm_tolerance: f64,
}

/// The Mach-zero [`DesignTrim`] of `inputs`.
///
/// # Errors
///
/// The solve's own error text when the cruise attitude cannot be evaluated.
pub(super) fn mach_zero_reference(
    aero: &AeroAnalysis<'_>,
    inputs: MachZeroInputs<'_>,
) -> Result<Box<DesignTrim>, String> {
    let reference = aero
        .trimmed_stretched_performance(
            inputs.plane,
            &inputs.cruise_attitude,
            inputs.cruise_mach,
            inputs.cruise_altitude_m,
            1.0,
            inputs.cm_body_per_deg,
        )
        .map_err(|error| error.to_string())?;
    Ok(Box::new(DesignTrim {
        trim: inputs.trim,
        cl: reference.cl,
        cd_induced: reference.cd_induced,
        cm_residual: reference.cm_residual,
        mach: inputs.cruise_mach,
        altitude_m: inputs.cruise_altitude_m,
        cl_max_clean: inputs.cl_max_clean,
        cm_tolerance: inputs.cm_tolerance,
        stretched: Some(Arc::new(StretchedBasis {
            plane: inputs.plane.clone(),
            beta: 1.0,
            cm_body_per_deg: inputs.cm_body_per_deg,
        })),
        incompressible: None,
    }))
}
