// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Re-trim with the geometry-only work of this candidate's earlier trims.

use crate::mdo::trim::{trim_and_polar_with_cache, TrimmedPolar};
use crate::mdo::types::CandidateFailure;
use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;

impl super::MdaContext<'_> {
    /// [`retrim_needed`] under this closure's tolerance: the configured one,
    /// or the looser one of a reused gear-translation trim (lever A13).
    pub(super) fn retrim_needed(&self, plane: &Airplane, cg_x_m: f64, cg_at_trim_x_m: f64) -> bool {
        retrim_needed_within(
            self.retrim_tolerance_pct_mac
                .unwrap_or(self.config.optimizer.objective.retrim_cg_tolerance_pct_mac),
            plane,
            cg_x_m,
            cg_at_trim_x_m,
        )
    }

    pub(super) fn trim_at(
        &self,
        plane: &mut Airplane,
        cg_x: f64,
        mass_kg: f64,
    ) -> Result<TrimmedPolar, CandidateFailure> {
        trim_and_polar_with_cache(
            self.config,
            plane,
            cg_x,
            self.dv,
            mass_kg,
            &self.vlm_cache,
            self.screening_drag_table,
        )
    }
}

/// Whether a polar trimmed with the centre of gravity at `cg_at_trim_x_m`
/// must be re-trimmed for one at `cg_x_m`: a shift of more than
/// `retrim_cg_tolerance_pct_mac` percent of the reference chord of `plane`.
pub(crate) fn retrim_needed(
    config: &AlasConfig,
    plane: &Airplane,
    cg_x_m: f64,
    cg_at_trim_x_m: f64,
) -> bool {
    retrim_needed_within(
        config.optimizer.objective.retrim_cg_tolerance_pct_mac,
        plane,
        cg_x_m,
        cg_at_trim_x_m,
    )
}

/// [`retrim_needed`] with an explicit tolerance, percent of the reference chord.
pub(crate) fn retrim_needed_within(
    tolerance_pct_mac: f64,
    plane: &Airplane,
    cg_x_m: f64,
    cg_at_trim_x_m: f64,
) -> bool {
    let mac = plane.c_ref.max(1e-9);
    let cg_shift_pct_mac = (cg_x_m - cg_at_trim_x_m).abs() / mac * 100.0;
    cg_shift_pct_mac > tolerance_pct_mac.max(0.0)
}
