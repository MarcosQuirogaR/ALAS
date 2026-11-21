// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Tailplane root-station extension and the set of tail surfaces.
//!
//! Frame: x metres aft of the nose tip, y to starboard, z up.
//!
//! Area convention (aero, stability, mass and the mission surrogate all read
//! it from the built surfaces, never from configuration): the tailplane's
//! `Wing::reference_area` is the trapezoid between its **centreline** section
//! and the tip, carried through the fuselage (the gross, Airbus and Boeing
//! reference convention, not the exposed area). The fin's `unfolded_area` is
//! the trapezoid between the attachment-height root (see `fin_root`) and the
//! tip, so it includes the sliver of fin below the root line.

use alas_config::EmpennageConfig;

use super::{AircraftBuilder, BuildError};
use crate::aircraft::wing::Wing;

/// The tailplane's centreline sections, derived from the configured root.
pub(super) struct HstabPlanform {
    /// Chord at the centreline, m.
    pub root_chord_m: f64,
    /// Incidence at the centreline, degrees.
    pub root_twist_deg: f64,
    /// Tip leading edge relative to the centreline leading edge, m.
    pub tip_le_m: [f64; 3],
    /// Centreline leading edge relative to the configured root station, m.
    pub root_shift_m: [f64; 3],
}

/// The tailplane planform at tail scale `ts`.
///
/// Without `hstab_root_station_y_m` the configured root is the centreline
/// section and nothing moves. With it, the configured root chord, incidence,
/// height and leading-edge x describe the drawn root at that station, and
/// the straight edges are extended linearly to the centreline.
pub(super) fn hstab_planform(g: &EmpennageConfig, ts: f64) -> Result<HstabPlanform, BuildError> {
    let (tip_x, tip_y, tip_z) = g.hstab_tip_le_m;
    // Only the in-plane (x, y) tip offset scales with the tail scale; the
    // vertical placement does not: `tip_le[2]` reproduced unscaled from the
    // Python source.
    let Some(station) = g.hstab_root_station_y_m else {
        return Ok(HstabPlanform {
            root_chord_m: g.hstab_root_chord_m * ts,
            root_twist_deg: g.hstab_root_twist_deg,
            tip_le_m: [tip_x * ts, tip_y * ts, tip_z],
            root_shift_m: [0.0; 3],
        });
    };
    if !(station.is_finite() && station >= 0.0 && station < tip_y) {
        return Err(BuildError::TailPlacement(format!(
            "hstab_root_station_y_m {station} must lie in [0, tip y {tip_y})"
        )));
    }
    // Edges run from the station (y = station) to the tip (y = tip_y); the
    // centreline lies `t` station-to-tip spans inboard of the station.
    let t = station / (tip_y - station);
    let root_chord = g.hstab_root_chord_m * ts;
    let tip_chord = g.hstab_tip_chord_m * ts;
    Ok(HstabPlanform {
        root_chord_m: root_chord + t * (root_chord - tip_chord),
        root_twist_deg: g.hstab_root_twist_deg
            - t * (g.hstab_tip_twist_deg - g.hstab_root_twist_deg),
        tip_le_m: [tip_x * ts * (1.0 + t), tip_y * ts, tip_z * (1.0 + t)],
        root_shift_m: [-t * tip_x * ts, 0.0, -t * tip_z],
    })
}

impl AircraftBuilder {
    /// The airplane's wings: the main wing, both stabilizers and the dorsal
    /// fin fillet when one is configured. Product geometry always carries the
    /// dorsal; the reference-compatibility contract, whose frozen aircraft has
    /// none, only with `vstab_dorsal_in_aero`.
    pub(super) fn with_tail_surfaces(
        &self,
        dv: &alas_config::DesignVector,
        main_wing: Wing,
        tail_airfoil: &crate::aircraft::airfoil::Airfoil,
        seat: &super::fin_root::FinSeat,
        fuselage: &crate::aircraft::fuselage::Fuselage,
    ) -> Result<Vec<Wing>, BuildError> {
        let own_fin = self.own_fin_airfoil()?;
        let fin_airfoil = own_fin.as_ref().unwrap_or(tail_airfoil);
        let fin = self.build_vstab(dv, fin_airfoil, true, seat)?;
        let dorsal = if self.geometry_contract == super::GeometryContract::Product
            || self.geometry.empennage.vstab_dorsal_in_aero
        {
            self.dorsal_on(dv, &fin, seat)?
        } else {
            None
        };
        let pod_surfaces = if self.geometry_contract == super::GeometryContract::Product {
            self.pod_surfaces(dv, &fin, seat)?
        } else {
            Vec::new()
        };
        let ventral = if self.geometry_contract == super::GeometryContract::Product {
            self.ventral_fin(dv, fuselage)?
        } else {
            None
        };
        let mut wings = vec![main_wing, self.build_hstab(dv, tail_airfoil, true)?, fin];
        wings.extend(dorsal);
        wings.extend(pod_surfaces);
        wings.extend(ventral);
        Ok(wings)
    }
}
