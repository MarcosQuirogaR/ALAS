// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The tail surfaces of [`AircraftBuilder`]: the full meshed build used by
//! [`AircraftBuilder::build`] and the unmeshed planform build used by tail
//! auto-sizing.

use alas_config::DesignVector;

use super::fin_root::{FinSeat, FinTrapezoid};
use super::{mesh, tail_attachment, AircraftBuilder, BuildError, GeometryContract};
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
    ///
    /// The product contract attaches the root to the body on `seat` (see
    /// [`super::fin_root`]); the reference-compatibility contract keeps the
    /// configured root, as the frozen artifacts do.
    pub(super) fn build_vstab(
        &self,
        dv: &DesignVector,
        tail_airfoil: &Airfoil,
        meshed: bool,
        seat: &FinSeat,
    ) -> Result<Wing, BuildError> {
        let g = &self.geometry.empennage;
        let ts = dv.tail_scale * g.vstab_scale_ratio;
        let x_vstab = (dv.fuselage_length_m - g.vstab_offset_from_tail_m) + dv.tail_x_shift_m;
        let (tip_x, tip_y, tip_z) = g.vstab_tip_le_m;
        // Only the in-plane (x, z) tip offset scales with the tail scale;
        // the spanwise placement does not: `tip_le[1]` reproduced unscaled
        // from the Python source (the fin grows in Z, not Y).
        let configured = FinTrapezoid {
            root_le_m: [x_vstab, 0.0, g.vstab_z_m],
            root_chord_m: g.vstab_root_chord_m * ts,
            tip_le_m: [x_vstab + tip_x * ts, tip_y, g.vstab_z_m + tip_z * ts],
            tip_chord_m: g.vstab_tip_chord_m * ts,
        };
        let fin = match self.geometry_contract {
            GeometryContract::Product => configured.attached_to(seat),
            GeometryContract::ReferenceCompatibility => configured,
        };
        // The root's displacement from the configured root: zero when the
        // fin is not moved, so the tip offset stays exactly the scaled one.
        let shift = [
            fin.root_le_m[0] - configured.root_le_m[0],
            fin.root_le_m[1] - configured.root_le_m[1],
            fin.root_le_m[2] - configured.root_le_m[2],
        ];

        let wing = Wing::new(
            "Vertical Stabilizer",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], fin.root_chord_m, 0.0, tail_airfoil.clone()),
                WingXSec::new(
                    [
                        tip_x * ts - shift[0],
                        tip_y - shift[1],
                        tip_z * ts - shift[2],
                    ],
                    fin.tip_chord_m,
                    0.0,
                    tail_airfoil.clone(),
                ),
            ],
            false,
        );
        let wing = wing.translate([x_vstab + shift[0], shift[1], g.vstab_z_m + shift[2]]);
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
        let fuselage = self.build_fuselage(dv)?;
        let seat = self.fin_seat(dv, &fuselage)?;
        Ok((
            self.build_hstab(dv, &tail_airfoil, false)?,
            self.build_vstab(dv, &tail_airfoil, false, &seat)?,
        ))
    }
}
