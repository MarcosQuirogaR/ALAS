// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The tail surfaces of [`AircraftBuilder`]: the full meshed build used by
//! [`AircraftBuilder::build`] and the unmeshed planform build used by tail
//! auto-sizing.

use alas_config::DesignVector;

use super::{mesh, tail_attachment, AircraftBuilder, BuildError};
use crate::aircraft::airfoil::Airfoil;
use crate::aircraft::wing::{Wing, WingXSec};

impl AircraftBuilder {
    /// Build the scaled horizontal stabilizer with its attachment (`_build_hstab`).
    pub(super) fn build_hstab(
        &self,
        dv: &DesignVector,
        tail_airfoil: &Airfoil,
        meshed: bool,
    ) -> Result<Wing, BuildError> {
        let g = &self.geometry.empennage;
        let ts = dv.tail_scale;
        let [x_hstab, _, z_hstab] = tail_attachment::root(g, dv, self.geometry_contract);
        let (tip_x, tip_y, tip_z) = g.hstab_tip_le_m;

        let wing = Wing::new(
            "Horizontal Stabilizer",
            vec![
                WingXSec::new(
                    [0.0, 0.0, 0.0],
                    g.hstab_root_chord_m * ts,
                    g.hstab_root_twist_deg,
                    tail_airfoil.clone(),
                ),
                // Only the in-plane (x, y) tip offset scales with the tail
                // scale; the vertical placement does not: `tip_le[2]`
                // reproduced unscaled from the Python source.
                WingXSec::new(
                    [tip_x * ts, tip_y * ts, tip_z],
                    g.hstab_tip_chord_m * ts,
                    g.hstab_tip_twist_deg,
                    tail_airfoil.clone(),
                ),
            ],
            true,
        );
        let wing = wing.translate([x_hstab, 0.0, z_hstab]);
        if !meshed {
            return Ok(wing);
        }
        Ok(mesh::for_contract(
            self.geometry_contract,
            &wing,
            g.n_subdivisions,
        )?)
    }

    /// The vertical stabilizer: root/tip cross-sections at the design
    /// vector's tail scale, translated aft to the tail datum:
    /// `_build_vstab`.
    pub(super) fn build_vstab(
        &self,
        dv: &DesignVector,
        tail_airfoil: &Airfoil,
        meshed: bool,
    ) -> Result<Wing, BuildError> {
        let g = &self.geometry.empennage;
        let ts = dv.tail_scale * g.vstab_scale_ratio;
        let x_vstab = (dv.fuselage_length_m - g.vstab_offset_from_tail_m) + dv.tail_x_shift_m;
        let (tip_x, tip_y, tip_z) = g.vstab_tip_le_m;

        let wing = Wing::new(
            "Vertical Stabilizer",
            vec![
                WingXSec::new(
                    [0.0, 0.0, 0.0],
                    g.vstab_root_chord_m * ts,
                    0.0,
                    tail_airfoil.clone(),
                ),
                // Only the in-plane (x, z) tip offset scales with the tail
                // scale; the spanwise placement does not: `tip_le[1]`
                // reproduced unscaled from the Python source (the fin grows
                // in Z, not Y).
                WingXSec::new(
                    [tip_x * ts, tip_y, tip_z * ts],
                    g.vstab_tip_chord_m * ts,
                    0.0,
                    tail_airfoil.clone(),
                ),
            ],
            false,
        );
        let wing = wing.translate([x_vstab, 0.0, g.vstab_z_m]);
        if !meshed {
            return Ok(wing);
        }
        Ok(mesh::for_contract(
            self.geometry_contract,
            &wing,
            g.n_subdivisions,
        )?)
    }

    /// The two tail surfaces alone, `(horizontal, vertical)`, as unmeshed
    /// two-section planforms.
    ///
    /// This is the cheap path for a solve that only needs planform area and
    /// quarter-MAC position as the tail scale changes (tail auto-sizing): a
    /// trapezoidal surface has the same area and aerodynamic centre before
    /// and after spanwise meshing, so no meshing or airfoil resolution is
    /// done. The fin scale is `dv.tail_scale` times the configured
    /// `vstab_scale_ratio`.
    ///
    /// # Errors
    ///
    /// [`BuildError::UnresolvedAirfoil`] when the tail airfoil is unknown.
    pub fn build_empennage(&self, dv: &DesignVector) -> Result<(Wing, Wing), BuildError> {
        let tail_airfoil = Self::resolve(&self.geometry.empennage.tail_airfoil)?;
        Ok((
            self.build_hstab(dv, &tail_airfoil, false)?,
            self.build_vstab(dv, &tail_airfoil, false)?,
        ))
    }
}
