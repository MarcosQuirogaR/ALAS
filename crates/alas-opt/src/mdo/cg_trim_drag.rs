// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Trimmed induced drag as a function of the centre of gravity.
//!
//! The drag table a candidate flies on is trimmed at one centre of gravity
//! ([`CandidateFuelArtifacts::trim_cg_x_m`]). Where the centre of gravity sits
//! sets the tail load that holds the pitching moment, and with it the trimmed
//! induced drag: a download on the tail makes the wing carry more than the
//! weight and the tail adds induced drag of its own. That is the trim drag
//! (Obert, *Aerodynamic Design of Transport Aircraft*, 2009, Eqs. 27.2-27.4,
//! p. 356: `CD_trim = (CL_tail-off^2 - CL_trim^2) / (pi A_W e) +
//! CL_h^2 (S_h/S_W) / (pi A_h e) + CL_h sin(eps) S_h/S_W`, the tail lift
//! following from the moment about the centre of gravity, `CL_h = Cm / V_h`;
//! Torenbeek, *Advanced Aircraft Design*, 2013, Sec. 4.5: trim drag depends on
//! the centre-of-gravity position and follows from biplane theory, Sec. 4.3).
//!
//! Rather than a closed formula with a separate wing-body moment estimate, the
//! same vortex-lattice trim that built the table is solved again at other
//! centre-of-gravity stations, so wing, tail, downwash and interference are the
//! model's own. In linear lattice theory the trimmed circulation is linear in
//! the lift coefficient and in the station, so the induced drag is a quadratic
//! in each: three stations fix the station dependence exactly (the lift
//! dependence is the table's). The reference station is the artifacts' own
//! table, so a leg flown at the reference station gets no increment at all.

use std::sync::Arc;

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;

use super::drag_table::TrimmedDragTable;
use super::trim::trim_and_polar_with_cache;
use super::types::{CandidateDrag, CandidateFuelArtifacts};

/// Smallest spacing of the stations beyond the range asked for, as a fraction of
/// the reference chord: the quadratic is fixed by three trims, and nodes closer
/// than this (a fraction of the chord the lattice resolves poorly) would
/// amplify that noise wherever the curve is read away from them.
const MINIMUM_PAD_FRACTION_OF_MAC: f64 = 0.05;

/// Trimmed induced-drag tables at three centre-of-gravity stations, the middle
/// one the reference table the mission flies on.
#[derive(Debug, Clone)]
pub struct CgTrimDrag {
    /// `(station m, table)`, ascending in station; the reference is `reference`.
    nodes: [(f64, Arc<TrimmedDragTable>); 3],
    reference: usize,
}

impl CgTrimDrag {
    /// Trim `plane` at two more stations bracketing `cg_range_m` (and the
    /// reference station) and keep the tables, at the cruise lift of
    /// `cruise_mass_kg` (which only places the table's nodes: the table spans
    /// the lift range).
    ///
    /// # Errors
    ///
    /// A message when the drag is an external solver's polar (it carries no
    /// station dependence to extend), or when the aircraft cannot be trimmed at
    /// a station: the stabiliser cannot hold the moment there.
    pub fn build(
        config: &AlasConfig,
        artifacts: &CandidateFuelArtifacts,
        plane: &Airplane,
        design: &DesignVector,
        cg_range_m: [f64; 2],
        cruise_mass_kg: f64,
    ) -> Result<Self, String> {
        let CandidateDrag::Table(reference) = &artifacts.drag else {
            return Err(
                "the aircraft's drag is an external solver's polar, which has no centre-of-gravity dependence to extend"
                    .to_owned(),
            );
        };
        let x_ref = artifacts.trim_cg_x_m;
        let pad = (MINIMUM_PAD_FRACTION_OF_MAC * plane.c_ref)
            .max(0.1 * (cg_range_m[1] - cg_range_m[0]).abs());
        let low = cg_range_m[0].min(x_ref) - pad;
        let high = cg_range_m[1].max(x_ref) + pad;
        let config = artifacts.config_with_clmax(config);
        let trimmed = |station_m: f64| -> Result<Arc<TrimmedDragTable>, String> {
            let mut plane = plane.clone();
            let polar = trim_and_polar_with_cache(
                &config,
                &mut plane,
                station_m,
                design,
                cruise_mass_kg,
                &Default::default(),
                false,
            )
            .map_err(|failure| {
                format!(
                    "the aircraft cannot be trimmed with the centre of gravity at x = {station_m:.2} m ({failure})"
                )
            })?;
            match polar.drag {
                CandidateDrag::Table(table) => Ok(table),
                CandidateDrag::External(_) => Err("the trim returned an external polar".to_owned()),
            }
        };
        Ok(Self {
            nodes: [
                (low, trimmed(low)?),
                (x_ref, reference.clone()),
                (high, trimmed(high)?),
            ],
            reference: 1,
        })
    }

    /// The stations the quadratic was fixed at, lowest and highest, m: the curve
    /// is an interpolation between them and an extrapolation outside.
    #[must_use]
    pub fn station_range_m(&self) -> [f64; 2] {
        [self.nodes[0].0, self.nodes[2].0]
    }

    /// The reference station, m.
    #[must_use]
    pub fn reference_station_m(&self) -> f64 {
        self.nodes[self.reference].0
    }

    /// Trimmed induced drag at `cl` and flight Mach `mach` with the centre of
    /// gravity at `station_m`, by the quadratic through the three stations,
    /// less the same at the reference station: the drag coefficient to add to
    /// the reference table's. The stretched tables carry the cruise-Mach curve
    /// blended to the incompressible one by Mach (`induced_cd_at`), so a climb
    /// or descent leg reads its own Mach, not the cruise one.
    #[must_use]
    pub fn increment_cd(&self, cl: f64, mach: f64, station_m: f64) -> f64 {
        let [(x0, t0), (x1, t1), (x2, t2)] = &self.nodes;
        let value = |x: f64| {
            let at = |table: &TrimmedDragTable| table.induced_cd_at(cl, mach);
            let (v0, v1, v2) = (at(t0), at(t1), at(t2));
            v0 * (x - x1) * (x - x2) / ((x0 - x1) * (x0 - x2))
                + v1 * (x - x0) * (x - x2) / ((x1 - x0) * (x1 - x2))
                + v2 * (x - x0) * (x - x1) / ((x2 - x0) * (x2 - x1))
        };
        value(station_m) - t1.induced_cd_at(cl, mach)
    }
}
