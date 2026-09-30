// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Lock/Korn transonic rise and the frozen Python comparison law.

use super::AeroAnalysis;
use alas_geom::aircraft::airplane::Airplane;

impl AeroAnalysis<'_> {
    /// Quarter-chord sweep of `plane`'s main wing, degrees: the mean of the
    /// per-panel quarter-chord sweeps weighted by panel planform area.
    ///
    /// The design vector's `sweep_deg` is the inboard leading-edge sweep the
    /// geometry builder lays out, not the quarter-chord sweep the Korn
    /// drag-divergence relation, the swept compressibility correction and the
    /// form factor are written for. A single root-to-tip line also under-reads
    /// a cranked planform (A380: 31.4 against the published 33.5). Each
    /// panel's sweep is `atan(dx / dy)` of its quarter-chord line in the
    /// planform (XY) projection, weighted by its projected area
    /// `dy (c_in + c_out) / 2`, so the result is purely geometric and equals
    /// the panel sweep for a single-panel wing.
    ///
    /// Panels steeper than 45 degrees (`|dz| > |dy|`), such as winglets, are
    /// not part of the lifting planform whose sweep these relations take: their
    /// quarter-chord line tends to 90 degrees as `dy` shrinks, so they are left
    /// out of the average rather than allowed to pull it up.
    ///
    /// The root panel starts at the root section, which the geometry builder
    /// places on the aircraft centreline, so the fuselage carry-through is
    /// averaged in with the exposed panels. The result is therefore the sweep
    /// of the full reference planform, not an exposed-wing sweep.
    ///
    /// `fallback` is returned only when there is no main wing with a root and
    /// a tip section, or no retained panel has a positive finite area.
    pub fn quarter_chord_sweep_deg(plane: &Airplane, fallback: f64) -> f64 {
        let Some(wing) = plane.wings.first().filter(|wing| wing.xsecs.len() >= 2) else {
            return fallback;
        };
        let mut weighted = 0.0;
        let mut area = 0.0;
        for pair in wing.xsecs.windows(2) {
            let dy = (pair[1].xyz_le[1] - pair[0].xyz_le[1]).abs();
            let dz = (pair[1].xyz_le[2] - pair[0].xyz_le[2]).abs();
            if dz > dy {
                continue;
            }
            let qc_in = pair[0].xyz_le[0] + 0.25 * pair[0].chord;
            let qc_out = pair[1].xyz_le[0] + 0.25 * pair[1].chord;
            let panel_area = dy * 0.5 * (pair[0].chord + pair[1].chord);
            let sweep = (qc_out - qc_in).atan2(dy).to_degrees();
            if panel_area.is_finite() && panel_area > 0.0 && sweep.is_finite() {
                weighted += panel_area * sweep;
                area += panel_area;
            }
        }
        if area > 0.0 {
            weighted / area
        } else {
            fallback
        }
    }

    /// Thickness-to-chord the Korn drag-divergence relation takes, which is a
    /// representative section of the whole wing: the main wing's
    /// exposed-area-weighted t/c, the same basis as the form factor. The root
    /// section is the thickest station on a tapered wing and overstates wave
    /// drag; reference-compatibility analyses keep it for the frozen fixtures.
    ///
    /// Reads [`Self::wing_thicknesses`]' entry 0 rather than running its own
    /// [`Self::area_weighted_thickness`] pass: `parasite_drag`'s own index-0
    /// term is that exact same quantity, cached there for the same reason.
    pub fn korn_thickness(&self) -> f64 {
        match self.plane.wings.first() {
            Some(_) if !self.reference_compatibility => self.wing_thicknesses()[0],
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

    /// The Korn drag-divergence and critical Mach numbers `(M_dd, M_crit)` at
    /// lift coefficient `cl`, from the same relations [`Self::wave_drag`]
    /// evaluates, for reporting.
    ///
    /// `M_dd = kappa/cos(L) - t/c/cos^2(L) - CL/(10 cos^3(L))` (Korn, as
    /// presented in Mason, *Configuration Aerodynamics*, transonic-drag
    /// notes), with `L` the quarter-chord sweep this analysis was built with,
    /// `kappa` the configured technology factor and `t/c` the Korn section
    /// thickness (`section_thickness`, else [`Self::korn_thickness`]).
    /// `M_crit = M_dd - (0.1/(4C))^(1/3)` inverts `dCD_w/dM = 0.1` on the
    /// Lock law `CD_w = C (M - M_crit)^4`; the frozen-law analysis reports
    /// `M_crit = M_dd`, as [`Self::wave_drag`] does. Unlike `wave_drag` there
    /// is no onset-Mach gate: this is the analytic value, not a drag.
    pub fn korn_mach_numbers(&self, cl: f64, section_thickness: Option<f64>) -> (f64, f64) {
        let thickness = section_thickness.unwrap_or_else(|| self.korn_thickness());
        let cos_sweep = self.sweep_deg.to_radians().cos();
        let mach_dd = self.drag.korn_technology_factor / cos_sweep
            - thickness / cos_sweep.powf(2.0)
            - cl / (10.0 * cos_sweep.powf(3.0));
        let mach_crit = if self.frozen_wave_drag {
            mach_dd
        } else {
            mach_dd - (0.1 / (4.0 * self.drag.wave_drag_coefficient)).cbrt()
        };
        (mach_dd, mach_crit)
    }
}

// A test constructs the geometry it asserts on directly, so a failed expect is
// the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::wing::{Wing, WingXSec};

    fn plane_with(sections: &[([f64; 3], f64)]) -> Airplane {
        let airfoil = Airfoil::from_name("naca2412").expect("a 4-digit NACA name");
        let xsecs = sections
            .iter()
            .map(|&(xyz, chord)| WingXSec::new(xyz, chord, 0.0, airfoil.clone()))
            .collect();
        Airplane {
            name: "sweep probe".to_owned(),
            xyz_ref: [0.0; 3],
            wings: vec![Wing::new("Main Wing", xsecs, true)],
            fuselages: Vec::new(),
            s_ref: 1.0,
            c_ref: 1.0,
            b_ref: 1.0,
        }
    }

    /// Leading-edge x that puts the quarter chord at `qc_x`.
    fn le_for_quarter_chord(qc_x: f64, chord: f64) -> f64 {
        qc_x - 0.25 * chord
    }

    #[test]
    fn a_cranked_a380_like_planform_reads_the_published_quarter_chord_sweep() {
        // Quarter-chord sweeps of 35 and 32 degrees on the inboard and outboard
        // panels of a 79.8 m span, 17 m root chord planform; the published A380
        // quarter-chord sweep is 33.5 degrees.
        let crank_y = 14.0;
        let tip_y = 39.9;
        let root_qc = 0.25 * 17.0;
        let crank_qc = root_qc + crank_y * 35.0_f64.to_radians().tan();
        let tip_qc = crank_qc + (tip_y - crank_y) * 32.0_f64.to_radians().tan();
        let plane = plane_with(&[
            ([0.0, 0.0, 0.0], 17.0),
            ([le_for_quarter_chord(crank_qc, 9.0), crank_y, 1.0], 9.0),
            ([le_for_quarter_chord(tip_qc, 3.0), tip_y, 3.0], 3.0),
        ]);
        let sweep = AeroAnalysis::quarter_chord_sweep_deg(&plane, 0.0);
        assert!((sweep - 33.5).abs() < 0.5, "{sweep}");
        // The area weighting lies between the two panel sweeps.
        assert!(sweep > 32.0 && sweep < 35.0);
    }

    #[test]
    fn a_straight_wing_has_zero_quarter_chord_sweep() {
        let plane = plane_with(&[([0.0, 0.0, 0.0], 3.0), ([0.0, 8.0, 0.0], 3.0)]);
        assert!(AeroAnalysis::quarter_chord_sweep_deg(&plane, 12.0).abs() < 1e-12);
    }

    #[test]
    fn a_tapered_wing_with_an_unswept_quarter_chord_reads_zero() {
        let plane = plane_with(&[
            ([0.0, 0.0, 0.0], 4.0),
            ([le_for_quarter_chord(1.0, 2.0), 10.0, 0.0], 2.0),
        ]);
        assert!(AeroAnalysis::quarter_chord_sweep_deg(&plane, 12.0).abs() < 1e-12);
    }

    #[test]
    fn a_near_vertical_winglet_panel_does_not_move_the_sweep() {
        // A 25 degree quarter-chord wing with and without a 2.5 m winglet that
        // rises 0.4 m outboard for every 2.5 m up and is swept back 1.5 m.
        let tip_qc = 16.0 * 25.0_f64.to_radians().tan();
        let wing = [
            ([0.0, 0.0, 0.0], 6.0),
            (
                [le_for_quarter_chord(0.25 * 6.0 + tip_qc, 2.0), 16.0, 0.0],
                2.0,
            ),
        ];
        let winglet_tip = (
            [
                le_for_quarter_chord(0.25 * 6.0 + tip_qc + 1.5, 1.0),
                16.4,
                2.5,
            ],
            1.0,
        );
        let clean = AeroAnalysis::quarter_chord_sweep_deg(&plane_with(&wing), 0.0);
        let with_winglet = AeroAnalysis::quarter_chord_sweep_deg(
            &plane_with(&[wing[0], wing[1], winglet_tip]),
            0.0,
        );
        assert!((clean - 25.0).abs() < 1e-9, "{clean}");
        assert!((with_winglet - clean).abs() < 1e-12, "{with_winglet}");
        // A wing made only of a vertical panel has no retained panel.
        let fin = plane_with(&[([0.0, 0.0, 0.0], 2.0), ([0.5, 0.1, 3.0], 1.0)]);
        assert_eq!(AeroAnalysis::quarter_chord_sweep_deg(&fin, 12.0), 12.0);
    }

    #[test]
    fn the_korn_mach_numbers_bracket_the_wave_drag_onset() {
        let plane = plane_with(&[
            ([0.0, 0.0, 0.0], 5.0),
            ([le_for_quarter_chord(6.0, 2.0), 15.0, 0.0], 2.0),
        ]);
        let analysis = AeroAnalysis::new(&plane, 30.0, None, None, None);
        let (mach_dd, mach_crit) = analysis.korn_mach_numbers(0.5, None);
        assert!(mach_dd.is_finite() && mach_crit.is_finite());
        assert!(mach_crit < mach_dd);
        // The accessor and the drag law share one critical Mach: no wave drag
        // below it, a positive rise above it.
        assert_eq!(analysis.wave_drag(mach_crit - 1e-6, 0.5, None), 0.0);
        assert!(analysis.wave_drag(mach_crit + 0.02, 0.5, None) > 0.0);
    }

    #[test]
    fn a_wing_without_two_sections_returns_the_fallback() {
        let plane = plane_with(&[([0.0, 0.0, 0.0], 3.0)]);
        assert_eq!(AeroAnalysis::quarter_chord_sweep_deg(&plane, 12.0), 12.0);
    }
}
