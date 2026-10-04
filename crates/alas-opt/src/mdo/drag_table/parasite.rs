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
