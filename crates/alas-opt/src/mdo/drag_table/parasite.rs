// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Exact component Reynolds evaluation on the candidate's Mach factors.

use super::{grid::lerp, TrimmedDragTable};
use alas_aero::analysis::AeroAnalysis;
use alas_atmo::Atmosphere;

impl TrimmedDragTable {
    /// Zero-lift parasite drag `CD0(M, h)`.
    pub fn cd0(&self, mach: f64, altitude_m: f64) -> f64 {
        self.parasite_components(mach, altitude_m)
            .map(|(_, cd)| cd)
            .sum()
    }

    /// Lift-dependent viscous drag `kappa CD0 CL^2` for the zero-lift
    /// parasite drag `cd0` (AERL-04 / DRG-01): zero while the configured
    /// `lift_dependent_viscous_fraction` is zero.
    pub fn viscous_lift_cd(&self, cl: f64, cd0: f64) -> f64 {
        self.viscous_lift_fraction * cd0 * cl * cl
    }

    /// [`Self::viscous_lift_cd`] at `mach` and ISA altitude `altitude_m`.
    pub fn viscous_lift_cd_at(&self, cl: f64, mach: f64, altitude_m: f64) -> f64 {
        self.viscous_lift_cd(cl, self.cd0(mach, altitude_m))
    }

    /// [`Self::viscous_lift_cd`] on an actual Reynolds-number-per-metre state
    /// (see [`Self::cd0_at_reynolds_per_m`]).
    pub fn viscous_lift_cd_at_reynolds_per_m(
        &self,
        cl: f64,
        mach: f64,
        reynolds_per_m: f64,
    ) -> f64 {
        self.viscous_lift_cd(cl, self.cd0_at_reynolds_per_m(mach, reynolds_per_m))
    }

    /// Total span efficiency `CL^2 / (pi AR (CD - CD0 - CDw))` implied by the
    /// induced, trim and lift-dependent viscous drag together:
    /// `1/e = 1/e_inviscid + kappa CD0 pi AR`.
    pub fn effective_oswald_total(&self, cl: f64, mach: f64, altitude_m: f64) -> f64 {
        let lift_drag = self.induced_cd(cl) + self.viscous_lift_cd_at(cl, mach, altitude_m);
        cl * cl / (std::f64::consts::PI * self.aspect_ratio * lift_drag)
    }

    /// Clean total drag `CD0 + CDv + CDi_trim + CDw` on an actual
    /// Reynolds-number-per-metre state, the sum
    /// [`Self::cd`] takes on the standard atmosphere.
    pub fn cd_at_reynolds_per_m(&self, cl: f64, mach: f64, reynolds_per_m: f64) -> f64 {
        let cd0 = self.cd0_at_reynolds_per_m(mach, reynolds_per_m);
        cd0 + self.viscous_lift_cd(cl, cd0) + self.induced_cd(cl) + self.wave_cd(cl, mach)
    }

    /// [`Self::cd`] without the lift-dependent viscous term: the polar a
    /// high-lift configuration adds its own increment to. The term is a
    /// clean-configuration one (Torenbeek 2013, p. 96).
    pub fn cd_without_lift_viscous(&self, cl: f64, mach: f64, altitude_m: f64) -> f64 {
        self.cd0(mach, altitude_m) + self.induced_cd(cl) + self.wave_cd(cl, mach)
    }

    /// [`Self::cd_at_reynolds_per_m`] without the lift-dependent viscous
    /// term, the base of a high-lift configuration (take-off, landing).
    pub fn cd_without_lift_viscous_at_reynolds_per_m(
        &self,
        cl: f64,
        mach: f64,
        reynolds_per_m: f64,
    ) -> f64 {
        self.cd0_at_reynolds_per_m(mach, reynolds_per_m)
            + self.induced_cd(cl)
            + self.wave_cd(cl, mach)
    }

    /// Parasite drag at the actual atmospheric Reynolds number per metre,
    /// `rho V / mu` (1/m). Each component retains its own reference length.
    /// Native mission atmosphere states use this same table, including an
    /// ISA temperature deviation, without reconstructing a parasite model.
    pub fn cd0_at_reynolds_per_m(&self, mach: f64, reynolds_per_m: f64) -> f64 {
        self.parasite_components_at_reynolds_per_m(mach, reynolds_per_m)
            .map(|(_, cd)| cd)
            .sum()
    }

    /// Each parasite component's drag at `mach` and `altitude_m`, in
    /// build-up order, labelled with the wing or body name.
    pub fn parasite_components(
        &self,
        mach: f64,
        altitude_m: f64,
    ) -> impl Iterator<Item = (&str, f64)> + '_ {
        let atmosphere = Atmosphere::new(altitude_m);
        let velocity = mach * atmosphere.speed_of_sound();
        let reynolds_per_m = atmosphere.density() * velocity / atmosphere.dynamic_viscosity();
        self.parasite_components_at_reynolds_per_m(mach, reynolds_per_m)
    }

    /// The same parasite build-up on an actual Reynolds-number-per-metre
    /// state; see [`Self::cd0_at_reynolds_per_m`].
    pub fn parasite_components_at_reynolds_per_m(
        &self,
        mach: f64,
        reynolds_per_m: f64,
    ) -> impl Iterator<Item = (&str, f64)> + '_ {
        let (index, weight) = self.mach_cell(mach);
        self.parasite.iter().map(move |component| {
            let factor = lerp(component.factor[index], component.factor[index + 1], weight);
            let reynolds = reynolds_per_m * component.reynolds_length_m;
            (
                component.label.as_str(),
                factor * AeroAnalysis::turbulent_cf(reynolds, mach),
            )
        })
    }
}
