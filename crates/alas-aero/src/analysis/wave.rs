// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Lock/Korn transonic rise and the frozen Python comparison law.

use super::AeroAnalysis;

impl AeroAnalysis<'_> {
    /// The Korn-equation transonic wave-drag estimate.
    ///
    /// Zero below the configured onset Mach, and zero again above it while
    /// the critical Mach the section, sweep and lift coefficient set has not
    /// been passed. `M_dd` (the Korn equation's own output) is the
    /// drag-divergence Mach, defined by `dCD/dM = 0.1`, not the onset of
    /// wave drag. The Lock/Korn law is `CD_w = C (M - M_crit)^4` with
    /// `M_crit = M_dd - (0.1/(4 C))^(1/3)`. At the default C=20, the offset
    /// is about 0.1077 (Mason, *Configuration Aerodynamics*, transonic-drag
    /// notes; Lock 1985; physics review v1.2, A3). The frozen Python fixture
    /// instead used `M_dd` as the start; `parity_analysis.rs` compares against
    /// the corrected expectation without widening tolerance.
    pub fn wave_drag(&self, mach: f64, cl: f64, section_thickness: Option<f64>) -> f64 {
        if mach < self.drag.wave_drag_onset_mach {
            return 0.0;
        }
        let thickness = section_thickness.unwrap_or_else(|| self.section_thickness());
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
