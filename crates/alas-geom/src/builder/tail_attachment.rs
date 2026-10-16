// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Longitudinal datum of the tail surfaces, and preservation of explicitly
//! attached tail reference geometry during design scaling.
//!
//! Frame: x metres aft of the nose tip, z up. The datum of both surfaces is
//! the leading edge of the **root line section**: for the fin, the section at
//! `vstab_z_m` (the crown in the presets); for the tailplane, its root
//! station. By default that x is `fuselage_length_m - offset_from_tail_m`:
//! it is measured from the design vector's overall fuselage length (the
//! nominal tail end of the lofted body), not from the fin's own geometry and
//! not from the end of any tail-cone fairing. An explicit `*_root_le_x_m`
//! replaces it with an absolute station from the nose. `tail_x_shift_m` moves
//! both either way.

use super::GeometryContract;
use alas_config::{DesignVector, EmpennageConfig};

/// Fin root-line leading edge x, m: the explicit station when configured,
/// else forward of the fuselage length by the configured offset.
pub(super) fn vstab_x(g: &EmpennageConfig, dv: &DesignVector) -> f64 {
    match g.vstab_root_le_x_m {
        Some(x) => x + dv.tail_x_shift_m,
        None => (dv.fuselage_length_m - g.vstab_offset_from_tail_m) + dv.tail_x_shift_m,
    }
}

/// Tailplane root-station leading edge x, m; see [`vstab_x`].
pub(super) fn hstab_x(g: &EmpennageConfig, dv: &DesignVector) -> f64 {
    match g.hstab_root_le_x_m {
        Some(x) => x + dv.tail_x_shift_m,
        None => dv.fuselage_length_m - g.hstab_offset_from_tail_m + dv.tail_x_shift_m,
    }
}

/// Largest distance, in x and in z, between the tailplane root and the fin tip
/// leading edge for the tailplane to count as seated on the fin (T-tail).
///
/// geo/close 2026-10-12: was exact coincidence. The drawn ATR 72-600 and A400M
/// tailplanes sit 0.6 m and 1.0 m below the drawn fin top (the fin cap and tip
/// fairing rise above them), so the seat is a fixed offset from the fin tip
/// leading edge that scales with the fin like the coincident seat did. Every
/// conventional tail is several metres from its fin tip.
const T_TAIL_SEAT_TOLERANCE_M: f64 = 1.1;

/// Whether the tailplane root sits on the fin tip region (T-tail).
///
/// The scaffold's geometric relationship is identified, not an aircraft
/// name: the fin tip lies on the plane of symmetry and the tailplane root
/// (x, z) lies within [`T_TAIL_SEAT_TOLERANCE_M`] of the fin tip leading edge.
/// Conventional and independently positioned tails are not attached.
fn attached(g: &EmpennageConfig, dv: &DesignVector) -> bool {
    let along = vstab_x(g, dv) + g.vstab_tip_le_m.0 - hstab_x(g, dv);
    let up = g.hstab_z_m - g.vstab_z_m - g.vstab_tip_le_m.2;
    g.vstab_tip_le_m.1.abs() < 1e-8
        && along.abs() <= T_TAIL_SEAT_TOLERANCE_M
        && up.abs() <= T_TAIL_SEAT_TOLERANCE_M
}

pub(super) fn root(g: &EmpennageConfig, dv: &DesignVector, contract: GeometryContract) -> [f64; 3] {
    let mut x = hstab_x(g, dv);
    let mut z = g.hstab_z_m;
    if contract == GeometryContract::Product && attached(g, dv) {
        let fin_scale = dv.tail_scale * g.vstab_scale_ratio;
        x += g.vstab_tip_le_m.0 * (fin_scale - 1.0);
        z += g.vstab_tip_le_m.2 * (fin_scale - 1.0);
    }
    [x, 0.0, z]
}
