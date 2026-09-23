// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Lock/Korn transonic rise and the frozen Python comparison law.

use super::AeroAnalysis;
use alas_geom::aircraft::airplane::Airplane;

impl AeroAnalysis<'_> {
    /// Quarter-chord sweep of `plane`'s main wing, degrees, measured off the
    /// built geometry the same way the mass model measures it.
    ///
    /// The design vector's `sweep_deg` is the inboard leading-edge sweep the
    /// geometry builder lays out, not the quarter-chord sweep the Korn
    /// drag-divergence relation, the swept compressibility correction and the
    /// form factor are written for; for a tapered wing the two differ by
    /// several degrees (A380: 36.4 against 33.5). `fallback` is returned only
    /// when there is no main wing with a root and a tip section.
    pub fn quarter_chord_sweep_deg(plane: &Airplane, fallback: f64) -> f64 {
        plane
            .wings
            .first()
            .filter(|wing| wing.xsecs.len() >= 2)
            .map_or(fallback, |wing| wing.mean_sweep_angle(0.25))
    }

    /// Thickness-to-chord the Korn drag-divergence relation takes, which is a
    /// representative section of the whole wing: the main wing's
    /// exposed-area-weighted t/c, the same basis as the form factor. The root
    /// section is the thickest station on a tapered wing and overstates wave
    /// drag; reference-compatibility analyses keep it for the frozen fixtures.
    pub fn korn_thickness(&self) -> f64 {
        match self.plane.wings.first() {
            Some(wing) if !self.reference_compatibility => Self::area_weighted_thickness(wing),
            _ => self.section_thickness(),
        }
    }

    /// The Korn-equation transonic wave-drag estimate.
    ///
    /// Zero below the configured onset Mach, and zero again above it while
    /// the critical Mach the section, sweep and lift coefficient set has not
    /// been passed. `M_dd` (the Korn equation's own output) is the
    /// drag-divergence Mach, defined by `dCD/dM = 0.1`, not the onset of
    /// wave drag. The Lock/Korn law is `CD_w = C (M - M_crit)^4` with
    /// `M_crit = M_dd - (0.1/(4 C))^(1/3)`. At the default C=20, the offset
    /// is about 0.1077 (Mason, *Configuration Aerodynamics*, transonic-drag
    /// notes; Lock 1985). The frozen Python fixture
    /// instead used `M_dd` as the start; `parity_analysis.rs` compares against
    /// the corrected expectation without widening tolerance.
    pub fn wave_drag(&self, mach: f64, cl: f64, section_thickness: Option<f64>) -> f64 {
        if mach < self.drag.wave_drag_onset_mach {
            return 0.0;
        }
        let thickness = section_thickness.unwrap_or_else(|| self.korn_thickness());
        let cos_sweep = self.sweep_deg.to_radians().cos();
        let kappa = self.drag.korn_technology_factor;
        let mach_dd =
            kappa / cos_sweep - thickness / cos_sweep.powf(2.0) - cl / (10.0 * cos_sweep.powf(3.0));
        // Invert 4*C*(M_dd - M_crit)^3 = 0.1. The explicit frozen route is
        // only used by objective parity fixtures generated with the old law.
        let mach_crit = if self.frozen_wave_drag {
            mach_dd
        } else {
            mach_dd - (0.1 / (4.0 * self.drag.wave_drag_coefficient)).cbrt()
        };
        if mach > mach_crit {
            self.drag.wave_drag_coefficient * (mach - mach_crit).powf(4.0)
        } else {
            0.0
        }
    }
}
