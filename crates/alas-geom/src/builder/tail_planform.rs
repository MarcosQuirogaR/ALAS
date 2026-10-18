// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Tailplane root-station extension and the dorsal fin fillet.
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

use super::{mesh, AircraftBuilder, BuildError};
use crate::aircraft::wing::{Wing, WingXSec};

/// Dorsal tip chord as a fraction of its root chord: a nearly sharp tip, as a
/// zero-chord section cannot be meshed.
const DORSAL_TIP_CHORD_FRACTION: f64 = 0.02;

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
    /// The airplane's wings: the main wing, both stabilizers and, when
    /// `vstab_dorsal_in_aero` is set, the dorsal fin fillet.
    pub(super) fn with_tail_surfaces(
        &self,
        dv: &alas_config::DesignVector,
        main_wing: Wing,
        tail_airfoil: &crate::aircraft::airfoil::Airfoil,
        seat: &super::fin_root::FinSeat,
    ) -> Result<Vec<Wing>, BuildError> {
        let mut wings = vec![
            main_wing,
            self.build_hstab(dv, tail_airfoil, true)?,
            self.build_vstab(dv, tail_airfoil, true, seat)?,
        ];
        if self.geometry.empennage.vstab_dorsal_in_aero {
            wings.extend(self.build_dorsal_fin(dv)?);
        }
        Ok(wings)
    }

    /// The dorsal fin fillet as a meshed lifting surface, or `None` when no
    /// dorsal is configured. Geometry for overlays and export; the airplane
    /// from [`AircraftBuilder::build`] carries it only with
    /// `vstab_dorsal_in_aero`.
    ///
    /// The strip lies in the fin plane: its root is on the fin root line
    /// (`vstab_z_m`) at `vstab_dorsal_x_start_m` and runs to the fin leading
    /// edge, which it meets `vstab_dorsal_height_m` above the root line with
    /// a nearly sharp tip.
    ///
    /// # Errors
    ///
    /// [`BuildError::UnresolvedAirfoil`] for an unknown tail airfoil;
    /// [`BuildError::TailPlacement`] when only one dorsal field is set, the
    /// start is not ahead of the fin leading edge, the height is not
    /// positive, or the fin has no height.
    pub fn build_dorsal_fin(
        &self,
        dv: &alas_config::DesignVector,
    ) -> Result<Option<Wing>, BuildError> {
        let g = &self.geometry.empennage;
        let (x_start, height) = match (g.vstab_dorsal_x_start_m, g.vstab_dorsal_height_m) {
            (None, None) => return Ok(None),
            (Some(x), Some(h)) => (x + dv.tail_x_shift_m, h),
            _ => {
                return Err(BuildError::TailPlacement(
                    "the dorsal fin needs both its start x and its height".to_owned(),
                ))
            }
        };
        let x_fin = super::tail_attachment::vstab_x(g, dv);
        let (tip_x, tip_y, tip_z) = g.vstab_tip_le_m;
        let ts = dv.tail_scale * g.vstab_scale_ratio;
        // Fin leading edge height-fraction at the dorsal tip: the fin edges
        // are straight, so scale cancels in tip_x / tip_z.
        let fin_height = tip_z * ts;
        if !(x_start.is_finite() && height.is_finite() && height > 0.0 && fin_height > 0.0) {
            return Err(BuildError::TailPlacement(
                "the dorsal height must be positive on a fin with height".to_owned(),
            ));
        }
        let fraction = height / fin_height;
        let le_at_tip = [tip_x * ts * fraction, tip_y * fraction, height];
        let root_chord = x_fin - x_start;
        if root_chord <= 0.0 {
            return Err(BuildError::TailPlacement(format!(
                "the dorsal start x {x_start} must be ahead of the fin root leading edge {x_fin}"
            )));
        }
        let tip_chord = DORSAL_TIP_CHORD_FRACTION * root_chord;
        let airfoil = Self::resolve(&g.tail_airfoil)?;
        let wing = Wing::new(
            "Dorsal Fin",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], root_chord, 0.0, airfoil.clone()),
                WingXSec::new(
                    [
                        x_fin + le_at_tip[0] - tip_chord - x_start,
                        le_at_tip[1],
                        le_at_tip[2],
                    ],
                    tip_chord,
                    0.0,
                    airfoil,
                ),
            ],
            false,
        )
        .translate([x_start, 0.0, g.vstab_z_m]);
        Ok(Some(mesh::for_contract(
            self.geometry_contract,
            &wing,
            g.n_subdivisions,
        )?))
    }
}
