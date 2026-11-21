// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The reported polar on the drag the missions fly.
//!
//! The polar a report starts with is an untrimmed lattice sweep: the
//! stabilizer at its generic incidence and the wing's declared twist carried
//! as a zero-lift span loading. The missions, the sizing and the reported
//! trimmed point fly the trimmed drag table instead (`alas_opt::mdo`
//! `drag_table`), whose induced term takes the twist as the handbook
//! allowance and whose compressibility drag is tabulated on the same law.
//! Once a report is bound to its mission, the drag columns of its polar are
//! the table's at each swept lift coefficient, at the cruise Mach and the
//! table's reference altitude, so the reported design point, the maximum
//! lift-to-drag ratio, the polar fit and the efficiency figure show the drag
//! the range and the fuel were computed with. Lift, moment and the angle axes
//! stay the sweep's.

use super::*;

impl FullAnalysis {
    /// Put the drag columns of `report.polar` on the trimmed drag table and
    /// refit the polar; `false`, and the report unchanged, when the report
    /// carries no table (no built fuel artifacts) or the table has no finite
    /// positive drag at a swept point.
    pub(super) fn rebase_polar_on_flown_drag(&self, report: &mut AnalysisReport) -> bool {
        let Some(artifacts) = report.fuel.built_artifacts() else {
            return false;
        };
        let Some(table) = artifacts.drag.table() else {
            return false;
        };
        let mach = self.config.requirements.cruise_mach;
        let altitude_m = table.reference_altitude_m();
        let mut columns = Vec::with_capacity(report.polar.cl.len());
        for &cl in &report.polar.cl {
            let cd = table.cd(cl, mach, altitude_m);
            let induced = table.induced_cd_at(cl, mach);
            let wave = table.wave_cd(cl, mach);
            if !(cd.is_finite() && cd > 0.0 && induced.is_finite() && wave.is_finite()) {
                return false;
            }
            columns.push((cd, induced, wave));
        }
        let polar = &mut report.polar;
        for (index, (cd, induced, wave)) in columns.into_iter().enumerate() {
            polar.cd[index] = cd;
            polar.cd_induced[index] = induced;
            polar.cd_wave[index] = wave;
            polar.cd_parasite[index] = cd - induced - wave;
            polar.l_over_d[index] = polar.cl[index] / cd;
        }
        report.polar_fit = self.fit_polar(&report.airplane, &report.polar);
        true
    }
}
