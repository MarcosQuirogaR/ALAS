// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Exact component Reynolds evaluation on the candidate's Mach factors.

use super::{grid::lerp, DragTableError, TrimmedDragTable};
use alas_aero::analysis::{DragAreaItem, SkinFriction};
use alas_atmo::Atmosphere;

/// One parasite component with its skin-friction-free factor per Mach node.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub(super) struct ParasiteComponent {
    pub(super) label: String,
    pub(super) reynolds_length_m: f64,
    /// The component's skin-friction law, as the build-up took it.
    pub(super) friction: SkinFriction,
    /// `FF Q Swet/S * component_scale * viscous_margin` at each Mach node.
    pub(super) factor: Vec<f64>,
}

/// One drag-area item (upsweep, base), Reynolds-independent.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub(super) struct DragAreaColumn {
    #[serde(deserialize_with = "alas_config::static_text::text")]
    label: alas_config::static_text::Text,
    /// `D/q / S * viscous_margin` at each Mach node.
    cd: Vec<f64>,
}

impl DragAreaColumn {
    /// Empty columns labelled like `items`.
    pub(super) fn columns(items: &[DragAreaItem]) -> Vec<Self> {
        items
            .iter()
            .map(|item| Self {
                label: item.name,
                cd: Vec::new(),
            })
            .collect()
    }

    /// Empty every column, returning the entries it held, column by column.
    pub(super) fn take_all(columns: &mut [Self]) -> Vec<Vec<f64>> {
        columns
            .iter_mut()
            .map(|column| std::mem::take(&mut column.cd))
            .collect()
    }

    /// Append the Mach node `index` of entries [`Self::take_all`] returned.
    pub(super) fn push_previous(columns: &mut [Self], previous: &[Vec<f64>], index: usize) {
        for (column, entries) in columns.iter_mut().zip(previous) {
            column.cd.push(entries[index]);
        }
    }

    /// Append one Mach node: each item's coefficient times the margin.
    pub(super) fn push_all(
        columns: &mut [Self],
        items: &[DragAreaItem],
        margin: f64,
    ) -> Result<(), DragTableError> {
        if items.len() != columns.len() {
            return Err(DragTableError::NonPhysical("parasite component count"));
        }
        for (column, item) in columns.iter_mut().zip(items) {
            let cd = item.cd * margin;
            if !cd.is_finite() || cd < 0.0 {
                return Err(DragTableError::NonPhysical("parasite drag area"));
            }
            column.cd.push(cd);
        }
        Ok(())
    }
}

impl TrimmedDragTable {
    /// Zero-lift parasite drag `CD0(M, h)`.
    pub fn cd0(&self, mach: f64, altitude_m: f64) -> f64 {
        self.parasite_components(mach, altitude_m)
            .map(|(_, cd)| cd)
            .sum()
    }

    /// Lift-dependent viscous drag `kappa CD0 CL^2` for the zero-lift
    /// parasite drag `cd0` (AERL-04 / DRG-01): zero while the configured
    /// `lift_dependent_viscous_fraction` is zero. `cd0` is the whole
    /// aircraft profile drag, upsweep column included: Jenkinson, *Civil Jet
    /// Aircraft Design* (1999), pp. 179-180, makes the term proportional to
    /// the profile drag of the complete aircraft, its factor fitted on
    /// complete jet transports.
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
        let lift_drag = self.induced_at(cl, mach).0 + self.viscous_lift_cd_at(cl, mach, altitude_m);
        cl * cl / (std::f64::consts::PI * self.aspect_ratio * lift_drag)
    }

    /// Clean total drag `CD0 + CDv + CDup(alpha) + CDi_trim + CDw` on an
    /// actual Reynolds-number-per-metre state, the sum [`Self::cd`] takes
    /// on the standard atmosphere.
    pub fn cd_at_reynolds_per_m(&self, cl: f64, mach: f64, reynolds_per_m: f64) -> f64 {
        let cd0 = self.cd0_at_reynolds_per_m(mach, reynolds_per_m);
        cd0 + self.viscous_lift_cd(cl, cd0)
            + self.upsweep_attitude_cd(cl, mach)
            + self.induced_at(cl, mach).0
            + self.wave_cd(cl, mach)
    }

    /// Drag-area column `index` at `mach`, the viscous margin included.
    pub(super) fn drag_area_cd(&self, index: usize, mach: f64) -> f64 {
        let (cell, weight) = self.mach_cell(mach);
        let column = &self.drag_areas[index].cd;
        lerp(column[cell], column[cell + 1], weight)
    }

    /// Induced drag of a configuration whose flaps add `flap_lift_cl` at constant angle
    /// of attack: the lattice's (flaps undeflected) trimmed induced drag at `cl`, with
    /// its span efficiency raised by the flap span-loading effect of
    /// [`FlapSpanLoading`] (Obert 2009, Fig. 27.39, printed p. 363: the apparent Oswald
    /// factor of wing-engined transports rises 0.0026 per degree of flap), the flap
    /// deflection taken in proportion to the lift the flaps add. Unchanged for a clean
    /// configuration or a table without the effect.
    pub(super) fn flapped_induced_cd(&self, cl: f64, mach: f64, flap_lift_cl: f64) -> f64 {
        let base = self.induced_at(cl, mach).0;
        match self.flap_span_loading {
            Some(loading)
                if flap_lift_cl > 0.0
                    && loading.reference_lift_shift > 0.0
                    && base > 0.0
                    && cl > 0.0 =>
            {
                let e = cl * cl / (std::f64::consts::PI * self.aspect_ratio * base);
                let gain = loading.delta_e * flap_lift_cl / loading.reference_lift_shift;
                base * e / (e + gain)
            }
            _ => base,
        }
    }

    /// [`Self::cd`] without the lift-dependent viscous term: the polar a
    /// high-lift configuration adds its own increment to. The term is a
    /// clean-configuration one (Torenbeek 2013, p. 96). The upsweep is taken
    /// at the attitude of a configuration whose devices add `flap_lift_cl` at
    /// constant angle of attack ([`super::high_lift_lift_shift`]).
    pub fn cd_without_lift_viscous(
        &self,
        cl: f64,
        mach: f64,
        altitude_m: f64,
        flap_lift_cl: f64,
    ) -> f64 {
        self.cd0(mach, altitude_m)
            + self.upsweep_attitude_cd_high_lift(cl, mach, flap_lift_cl)
            + self.flapped_induced_cd(cl, mach, flap_lift_cl)
            + self.wave_cd(cl, mach)
    }

    /// [`Self::cd_at_reynolds_per_m`] without the lift-dependent viscous
    /// term, the base of a high-lift configuration (take-off, landing).
    pub fn cd_without_lift_viscous_at_reynolds_per_m(
        &self,
        cl: f64,
        mach: f64,
        reynolds_per_m: f64,
        flap_lift_cl: f64,
    ) -> f64 {
        self.cd0_at_reynolds_per_m(mach, reynolds_per_m)
            + self.upsweep_attitude_cd_high_lift(cl, mach, flap_lift_cl)
            + self.flapped_induced_cd(cl, mach, flap_lift_cl)
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
        let components = self.parasite.iter().map(move |component| {
            let factor = lerp(component.factor[index], component.factor[index + 1], weight);
            let reynolds = reynolds_per_m * component.reynolds_length_m;
            (
                component.label.as_str(),
                factor * component.friction.cf(reynolds, mach),
            )
        });
        let drag_areas = self
            .drag_areas
            .iter()
            .map(move |item| (item.label, lerp(item.cd[index], item.cd[index + 1], weight)));
        components.chain(drag_areas)
    }
}
