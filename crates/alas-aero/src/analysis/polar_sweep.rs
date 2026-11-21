// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The compressible drag polar on the Goethert-stretched lattice (A1) and the
//! shared per-angle solve of both polar sweeps.

use alas_atmo::Atmosphere;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::spacing::linspace;

use crate::vlm::{self, VlmError};

use super::performance::{
    set_twist, PolarSweep, TrimPoint, TrimmedInviscid, TrimmedPerformance, HSTAB_NAME,
};
use super::{swept_pg_beta, AeroAnalysis};

impl AeroAnalysis<'_> {
    /// The compressible drag polar: the schedule solved on `system`, the
    /// Goethert-stretched (`1 / beta`) lattice of `self.plane`, at the true
    /// geometric angles (A1). Lift is the compressible lift, the pitching
    /// moment is `beta` times the lattice moment, the induced drag is the
    /// stretched lattice's Trefftz drag (`Gamma(y)` is the physical one in
    /// the same-angle form) and the empirical drag is added at the
    /// compressible `CL`. `alpha_deg` is the geometric angle: no relabel.
    ///
    /// # Errors
    ///
    /// See [`VlmError`].
    pub fn run_stretched_sweep_with_system(
        &self,
        stretched: &vlm::VlmSystem<'_>,
        mach: f64,
        altitude_m: f64,
        beta: f64,
    ) -> Result<PolarSweep, VlmError> {
        // The compressible lift slope is larger by about `1 / beta_w`, so the
        // same angle schedule would space the lift points that much wider and
        // leave fewer of them in the polar-fit window and near the design
        // lift. The point count grows by that factor to keep the spacing in
        // `CL` of the incompressible sweep.
        let density = (1.0 / swept_pg_beta(mach, self.sweep_deg)).max(1.0);
        let mut sweep = self.solve_schedule(stretched, mach, altitude_m, beta, density)?;
        sweep.compressible_mach = Some(mach);
        Ok(sweep)
    }

    /// The trimmed cruise condition on the Goethert-stretched lattice (A1F):
    /// `stretched` is the stretched airplane (`1 / beta`), solved at the true
    /// geometric `trim` angle with the stabilizer incidence `trim` settled.
    /// Lift, Trefftz induced drag and the empirical drag at the compressible
    /// `CL` are the compressible ones and the moment residual is `beta` times
    /// the lattice moment plus the body moment `cm_body_per_deg * alpha` the
    /// trim solved against, so every number is on the polar's basis.
    ///
    /// # Errors
    ///
    /// See [`VlmError`].
    pub fn trimmed_stretched_performance(
        &self,
        stretched: &Airplane,
        trim: &TrimPoint,
        mach: f64,
        altitude_m: f64,
        beta: f64,
        cm_body_per_deg: f64,
    ) -> Result<TrimmedPerformance, VlmError> {
        let atmosphere = Atmosphere::new(altitude_m);
        let velocity = mach * atmosphere.speed_of_sound();
        let op_point = Self::level_op_point(atmosphere, velocity, trim.trim_alpha_deg);
        let mut plane = stretched.clone();
        if !trim.trim_ih_deg.is_nan() {
            for wing in plane.wings.iter_mut().filter(|w| w.name == HSTAB_NAME) {
                set_twist(wing, trim.trim_ih_deg, !self.reference_compatibility);
            }
        }
        let system = self.assemble(&plane)?;
        let solved = system.solve(&op_point)?;
        let cd_induced = self.induced_drag(&solved, &op_point, Some(&system), &plane)?;
        let components = self.drag_components_at_attitude(
            mach,
            altitude_m,
            solved.cl_lift,
            cd_induced,
            Some(&atmosphere),
            trim.trim_alpha_deg,
        );
        Ok(TrimmedPerformance {
            l_over_d: solved.cl_lift / components.cd_total(),
            alpha_deg: trim.trim_alpha_deg,
            incidence_deg: trim.trim_ih_deg,
            cd: components.cd_total(),
            cd_parasite: components.cd_viscous(),
            cd_induced: components.cd_induced,
            cd_wave: components.cd_wave,
            cl: solved.cl_lift,
            cm_residual: beta * solved.cm_pitch + cm_body_per_deg * trim.trim_alpha_deg,
        })
    }

    /// [`Self::trimmed_inviscid`] on the Goethert-stretched lattice (A1G): the
    /// lift, Trefftz induced drag and moment residual (`beta Cm' + cm_body
    /// alpha`, the body term the compressible trim closed) of `stretched`
    /// (`goethert_stretch` of `self.plane`) at the true attitude `trim`, with
    /// the stabilizer incidence applied and the lattice taken from the
    /// geometry cache. This is the trim-node solve of the drag table;
    /// `reference_ih_deg` as in [`Self::trimmed_inviscid`].
    ///
    /// # Errors
    ///
    /// See [`VlmError`].
    #[allow(clippy::too_many_arguments)] // one named physical input each
    pub fn trimmed_inviscid_stretched(
        &self,
        stretched: &Airplane,
        trim: &TrimPoint,
        reference_ih_deg: Option<f64>,
        mach: f64,
        altitude_m: f64,
        beta: f64,
        cm_body_per_deg: f64,
    ) -> Result<TrimmedInviscid, VlmError> {
        let atmosphere = Atmosphere::new(altitude_m);
        let velocity = mach * atmosphere.speed_of_sound();
        let op_point = Self::level_op_point(atmosphere, velocity, trim.trim_alpha_deg);
        let solved =
            self.tail_set_solve(stretched, trim.trim_ih_deg, reference_ih_deg, &op_point)?;
        Ok(TrimmedInviscid {
            cm_residual: beta * solved.cm_residual + cm_body_per_deg * trim.trim_alpha_deg,
            ..solved
        })
    }

    /// One solve per angle of the configured schedule on `system`, with the
    /// empirical drag added at each solve's own `CL`. `cm_scale` multiplies
    /// the lattice moment (1 on the physical lattice, `beta` on a
    /// Goethert-stretched one). `density` multiplies the number of schedule
    /// intervals (1 keeps the configured schedule). The reporting angle is the
    /// geometric angle.
    pub(super) fn solve_schedule(
        &self,
        system: &vlm::VlmSystem<'_>,
        mach: f64,
        altitude_m: f64,
        cm_scale: f64,
        density: f64,
    ) -> Result<PolarSweep, VlmError> {
        let atmosphere = Atmosphere::new(altitude_m);
        let velocity = mach * atmosphere.speed_of_sound();
        let alphas = linspace(
            self.analysis.sweep_alpha_min_deg,
            self.analysis.sweep_alpha_max_deg,
            denser_count(self.analysis.sweep_n_points.max(0) as usize, density),
        );

        let mut sweep = PolarSweep {
            alpha_deg: Vec::with_capacity(alphas.len()),
            geometric_alpha_deg: alphas.clone(),
            compressible_mach: None,
            cl: Vec::with_capacity(alphas.len()),
            cd: Vec::with_capacity(alphas.len()),
            cd_induced: Vec::with_capacity(alphas.len()),
            cd_wave: Vec::with_capacity(alphas.len()),
            cd_parasite: Vec::with_capacity(alphas.len()),
            cm: Vec::with_capacity(alphas.len()),
            l_over_d: Vec::with_capacity(alphas.len()),
        };

        // The influence matrix depends on the geometry alone, so the whole
        // schedule shares one assembly and one factorization; each angle is
        // a new right-hand side.
        if !alphas.is_empty() {
            for alpha in &alphas {
                let op_point = Self::level_op_point(atmosphere, velocity, *alpha);
                let solved = system.solve(&op_point)?;
                let cd_induced =
                    self.induced_drag(&solved, &op_point, Some(system), system.airplane())?;
                let components = self.drag_components(
                    mach,
                    altitude_m,
                    solved.cl_lift,
                    cd_induced,
                    Some(&atmosphere),
                );
                sweep.alpha_deg.push(*alpha);
                sweep.cl.push(solved.cl_lift);
                sweep.cm.push(cm_scale * solved.cm_pitch);
                sweep.cd.push(components.cd_total());
                sweep.cd_induced.push(components.cd_induced);
                sweep.cd_wave.push(components.cd_wave);
                sweep.cd_parasite.push(components.cd_viscous());
                sweep.l_over_d.push(solved.cl_lift / components.cd_total());
            }
        }

        Ok(sweep)
    }
}

/// The point count of a schedule of `count` points whose interval count is
/// multiplied by `density` (rounded up); a schedule of fewer than two points
/// is unchanged.
fn denser_count(count: usize, density: f64) -> usize {
    if count < 2 {
        return count;
    }
    ((count - 1) as f64 * density).ceil() as usize + 1
}

#[cfg(test)]
mod tests {
    use super::denser_count;

    #[test]
    fn the_denser_count_keeps_degenerate_schedules_and_unit_density() {
        assert_eq!(denser_count(0, 1.4), 0);
        assert_eq!(denser_count(1, 1.4), 1);
        for count in [2, 15, 40] {
            assert_eq!(denser_count(count, 1.0), count);
        }
    }

    #[test]
    fn the_denser_count_rounds_the_interval_count_up() {
        // 14 intervals at 1.3 are 18.2, so 19 intervals and 20 points.
        assert_eq!(denser_count(15, 1.3), 20);
        // Exact multiples add no interval: 14 * 2 = 28 intervals.
        assert_eq!(denser_count(15, 2.0), 29);
        assert_eq!(denser_count(2, 1.01), 3);
    }
}
