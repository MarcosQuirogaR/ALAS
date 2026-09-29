// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Geometry-only thickness readings shared by `AeroAnalysis::parasite_drag`,
//! `drag_components` and `wave_drag`.
//!
//! Split out of `analysis.rs` to keep that file under its production-line
//! budget: these are one cohesive concern (what "the wing's thickness" means
//! at three different granularities) that both `analysis.rs`'s own
//! `parasite_drag` and `analysis::wave`'s `korn_thickness` read, and every
//! item below stays reachable from both through `pub(super)`.

use alas_geom::aircraft::spacing::linspace;
use alas_geom::aircraft::wing::{Wing, WingXSec};

use super::{AeroAnalysis, MAX_THICKNESS_SAMPLES, SECTION_THICKNESS_FALLBACK};

impl AeroAnalysis<'_> {
    /// The real maximum thickness-to-chord of the morphed root section, or
    /// [`SECTION_THICKNESS_FALLBACK`] where there is no section to read.
    ///
    /// Cached: see `AeroAnalysis::section_thickness_cache`'s own doc for why
    /// that is sound here.
    pub fn section_thickness(&self) -> f64 {
        *self.section_thickness_cache.get_or_init(|| {
            self.plane
                .wings
                .first()
                .map_or(SECTION_THICKNESS_FALLBACK, Self::wing_section_thickness)
        })
    }

    /// [`Self::parasite_drag`]'s own per-wing thickness, one entry per
    /// `self.plane.wings`, computed once and cached: see
    /// `AeroAnalysis::wing_thickness_cache`'s own doc.
    pub(super) fn wing_thicknesses(&self) -> &[f64] {
        self.wing_thickness_cache.get_or_init(|| {
            self.plane
                .wings
                .iter()
                .enumerate()
                .map(|(index, wing)| {
                    if index == 0 {
                        Self::area_weighted_thickness(wing)
                    } else {
                        Self::wing_section_thickness(wing)
                    }
                })
                .collect()
        })
    }

    /// The maximum thickness-to-chord of a surface's root section.
    ///
    /// Parasite drag is accumulated surface by surface. A tail can therefore
    /// not inherit the main wing's section thickness merely because the
    /// latter is the first surface in the airplane. Empty surfaces retain the
    /// same observable fallback as [`Self::section_thickness`].
    pub(super) fn wing_section_thickness(wing: &Wing) -> f64 {
        wing.xsecs
            .first()
            .map_or(SECTION_THICKNESS_FALLBACK, |xsec| {
                xsec.airfoil
                    .max_thickness(&linspace(0.0, 1.0, MAX_THICKNESS_SAMPLES))
            })
    }

    /// The exposed-area-weighted thickness-to-chord across every panel of
    /// `wing`, Raymer eq. 12.30's own convention for the form-factor `t/c`.
    ///
    /// The root section's maximum thickness (what [`Self::wing_section_thickness`]
    /// returns) is the thickest station on a tapered wing, so using it for
    /// the whole surface's form factor biases the factor high: physics
    /// review v1.2, section 3.1 (root t/c 0.15 against an area-weighted
    /// ~0.12 costs about +8% wing profile drag on the reviewed case). Each
    /// panel between consecutive cross-sections contributes the mean of its
    /// two end thicknesses, weighted by that panel's own trapezoidal
    /// planform area (span in the YZ plane, so dihedral is respected, times
    /// the mean chord) -- the same panel decomposition
    /// [`Wing::mean_aerodynamic_chord`] and [`Wing::aerodynamic_center`] use
    /// internally, reproduced here from public cross-section fields since
    /// that panel area is not itself exposed across the crate boundary.
    /// Falls back to [`Self::wing_section_thickness`] for a wing with fewer
    /// than two cross-sections, where no panel exists to weight.
    pub(super) fn area_weighted_thickness(wing: &Wing) -> f64 {
        // Each section's thickness is sampled once; interior sections bound
        // two panels and would otherwise be sampled twice.
        let samples = linspace(0.0, 1.0, MAX_THICKNESS_SAMPLES);
        let thicknesses: Vec<f64> = wing
            .xsecs
            .iter()
            .map(|xsec: &WingXSec| xsec.airfoil.max_thickness(&samples))
            .collect();
        let mut area_sum = 0.0;
        let mut weighted_sum = 0.0;
        for (pair, t) in wing.xsecs.windows(2).zip(thicknesses.windows(2)) {
            let dy = pair[1].xyz_le[1] - pair[0].xyz_le[1];
            let dz = pair[1].xyz_le[2] - pair[0].xyz_le[2];
            let span_m = (dy * dy + dz * dz).sqrt();
            let panel_area = span_m * (pair[0].chord + pair[1].chord) / 2.0;
            let panel_thickness = (t[0] + t[1]) / 2.0;
            area_sum += panel_area;
            weighted_sum += panel_area * panel_thickness;
        }
        if area_sum > 0.0 {
            weighted_sum / area_sum
        } else {
            Self::wing_section_thickness(wing)
        }
    }
}
