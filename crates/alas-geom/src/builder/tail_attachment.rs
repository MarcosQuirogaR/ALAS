// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Preserve explicitly attached tail reference geometry during design scaling.

use super::GeometryContract;
use alas_config::{DesignVector, EmpennageConfig};

pub(super) fn root(g: &EmpennageConfig, dv: &DesignVector, contract: GeometryContract) -> [f64; 3] {
    let mut x = dv.fuselage_length_m - g.hstab_offset_from_tail_m + dv.tail_x_shift_m;
    let mut z = g.hstab_z_m;
    // Identify the scaffold's geometric relationship, not an aircraft name.
    // Conventional and independently positioned tails retain their root.
    let attached = g.vstab_tip_le_m.1.abs() < 1e-8
        && (g.vstab_offset_from_tail_m - g.hstab_offset_from_tail_m - g.vstab_tip_le_m.0).abs()
            < 1e-8
        && (g.hstab_z_m - g.vstab_z_m - g.vstab_tip_le_m.2).abs() < 1e-8;
    if contract == GeometryContract::Product && attached {
        let fin_scale = dv.tail_scale * g.vstab_scale_ratio;
        x += g.vstab_tip_le_m.0 * (fin_scale - 1.0);
        z += g.vstab_tip_le_m.2 * (fin_scale - 1.0);
    }
    [x, 0.0, z]
}
