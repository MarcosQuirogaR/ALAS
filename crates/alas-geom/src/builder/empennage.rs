// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The tail surfaces of [`AircraftBuilder`]: the full meshed build used by
//! [`AircraftBuilder::build`] and the unmeshed planform build used by tail
//! auto-sizing.

use alas_config::DesignVector;

use super::fin_root::{FinSeat, FinTrapezoid};
use super::tail_edges::{
    reference_fin_end, with_drawn_edges, with_reference_fin_to, with_section_thickness,
    StraightTail,
};
use super::tail_planform::hstab_planform;
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
        let planform = hstab_planform(g, ts)?;
        let wing = Wing::new(
            "Horizontal Stabilizer",
            vec![
                WingXSec::new(
                    [0.0, 0.0, 0.0],
                    planform.root_chord_m,
                    planform.root_twist_deg,
                    tail_airfoil.clone(),
                ),
                WingXSec::new(
                    planform.tip_le_m,
                    g.hstab_tip_chord_m * ts,
                    g.hstab_tip_twist_deg,
                    tail_airfoil.clone(),
                ),
            ],
            true,
        );
        let wing = wing.translate([
            x_hstab + planform.root_shift_m[0],
            0.0,
            z_hstab + planform.root_shift_m[2],
        ]);
        if !meshed {
            return Ok(wing);
        }
        let wing = mesh::for_contract(self.geometry_contract, &wing, g.n_subdivisions)?;
        if self.geometry_contract != GeometryContract::Product {
            return Ok(wing);
        }
        let root_le_m = [
            x_hstab + planform.root_shift_m[0],
            0.0,
            z_hstab + planform.root_shift_m[2],
        ];
        let line = StraightTail {
            root_le_m,
            root_chord_m: planform.root_chord_m,
            root_twist_deg: planform.root_twist_deg,
            tip_le_m: [
                root_le_m[0] + planform.tip_le_m[0],
                planform.tip_le_m[1],
                root_le_m[2] + planform.tip_le_m[2],
            ],
            tip_chord_m: g.hstab_tip_chord_m * ts,
            tip_twist_deg: g.hstab_tip_twist_deg,
            span_axis: 1,
        };
        with_drawn_edges(wing, &g.hstab_edge_stations, &line, ts)
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
        fin_airfoil: &Airfoil,
        meshed: bool,
        seat: &FinSeat,
    ) -> Result<Wing, BuildError> {
        let g = &self.geometry.empennage;
        let ts = dv.tail_scale * g.vstab_scale_ratio;
        let x_vstab = tail_attachment::vstab_x(g, dv);
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
                WingXSec::new([0.0, 0.0, 0.0], fin.root_chord_m, 0.0, fin_airfoil.clone()),
                WingXSec::new(
                    [
                        tip_x * ts - shift[0],
                        tip_y - shift[1],
                        tip_z * ts - shift[2],
                    ],
                    fin.tip_chord_m,
                    0.0,
                    fin_airfoil.clone(),
                ),
            ],
            false,
        );
        let mut wing = wing.translate([x_vstab + shift[0], shift[1], g.vstab_z_m + shift[2]]);
        // A fin standing on a centreline pod is built from the pod up, but its
        // reference planform, the published fin area, stays carried down to the fuselage.
        // The leading edge meets the fuselage above the built root of any fin resting on
        // it, however it rests, so only a fin the pod holds above that point is carried.
        if self.geometry_contract == GeometryContract::Product {
            let on_fuselage = seat
                .fuselage_alone()
                .map(|body| configured.carried_to_leading_edge_on(&body));
            if let (Some(root), Some(tip)) = (
                on_fuselage.filter(|root| root.root_le_m[2] < fin.root_le_m[2]),
                wing.xsecs.last().cloned(),
            ) {
                let root =
                    WingXSec::new(root.root_le_m, root.root_chord_m, 0.0, fin_airfoil.clone());
                wing.reference_xsecs = Some(vec![root, tip]);
            }
        }
        let line = StraightTail {
            root_le_m: configured.root_le_m,
            root_chord_m: configured.root_chord_m,
            root_twist_deg: 0.0,
            tip_le_m: configured.tip_le_m,
            tip_chord_m: configured.tip_chord_m,
            tip_twist_deg: 0.0,
            span_axis: 2,
        };
        let seat = tail_attachment::tailplane_seat(g, dv, self.geometry_contract);
        let configured_end = g
            .vstab_reference_end_fraction
            .filter(|_| self.geometry_contract == GeometryContract::Product);
        let to_reference_end = |fin: Wing| match reference_fin_end(configured_end, &line, seat) {
            Some(fraction) => with_reference_fin_to(fin, &line, fraction),
            None => fin,
        };
        if !meshed {
            return Ok(to_reference_end(wing));
        }
        let wing = mesh::for_contract(self.geometry_contract, &wing, g.n_subdivisions)?;
        if self.geometry_contract != GeometryContract::Product {
            return Ok(wing);
        }
        with_drawn_edges(wing, &g.vstab_edge_stations, &line, ts)
            .map(to_reference_end)
            .and_then(|fin| with_section_thickness(fin, &g.vstab_thickness_stations, &line))
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
        let context = self.empennage_context(dv)?;
        self.build_empennage_in(dv, &context)
    }

    /// The parts of the unmeshed tail build that do not depend on the tail
    /// scale or the fin scale ratio: the resolved tail airfoil and the fin
    /// seat on the fuselage (and centreline nacelles) of `dv`.
    ///
    /// A solve that varies only `tail_scale` or `vstab_scale_ratio` builds
    /// this once and calls [`Self::build_empennage_in`] per trial, which
    /// returns exactly what [`Self::build_empennage`] returns (the body does
    /// not read either scale).
    ///
    /// # Errors
    ///
    /// [`BuildError::UnresolvedAirfoil`] when the tail airfoil is unknown, or
    /// the fuselage or nacelle build error.
    pub fn empennage_context(&self, dv: &DesignVector) -> Result<EmpennageContext, BuildError> {
        let tail_airfoil = Self::resolve(&self.geometry.empennage.tail_airfoil)?;
        let fuselage = self.build_fuselage_reported(dv, &mut super::BuildReport::default())?;
        let seat = self.fin_seat(dv, &fuselage)?;
        let fin_airfoil = self.own_fin_airfoil()?;
        Ok(EmpennageContext {
            tail_airfoil,
            fin_airfoil,
            seat,
        })
    }

    /// The fin's own section when the configuration states one.
    pub(super) fn own_fin_airfoil(&self) -> Result<Option<Airfoil>, BuildError> {
        let name = self.geometry.empennage.vstab_airfoil.as_deref();
        name.map(Self::resolve).transpose()
    }

    /// [`Self::build_empennage`] with the scale-independent parts supplied by
    /// [`Self::empennage_context`], built for the same body design.
    ///
    /// # Errors
    ///
    /// The tail planform build errors.
    pub fn build_empennage_in(
        &self,
        dv: &DesignVector,
        context: &EmpennageContext,
    ) -> Result<(Wing, Wing), BuildError> {
        Ok((
            self.build_hstab(dv, &context.tail_airfoil, false)?,
            self.build_vstab(
                dv,
                context
                    .fin_airfoil
                    .as_ref()
                    .unwrap_or(&context.tail_airfoil),
                false,
                &context.seat,
            )?,
        ))
    }
}

/// The tail-scale-independent inputs of the unmeshed tail build; see
/// [`AircraftBuilder::empennage_context`].
#[derive(Debug, Clone)]
pub struct EmpennageContext {
    tail_airfoil: Airfoil,
    fin_airfoil: Option<Airfoil>,
    seat: FinSeat,
}
