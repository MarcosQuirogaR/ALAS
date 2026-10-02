// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Lock/Korn transonic rise and the frozen Python comparison law.

use super::AeroAnalysis;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::WingXSec;

/// Largest difference in quarter-chord slope `dx/dy` between two adjacent
/// panels that are still read as subdivisions of one straight planform
/// panel: 1e-6, about 6e-5 degrees of sweep. The builder lofts a panel by
/// linear interpolation, so its subdivisions agree to rounding (about
/// 1e-12), while a planform crank changes the slope by more than 1e-2.
const SAME_PANEL_SLOPE_TOLERANCE: f64 = 1e-6;

impl AeroAnalysis<'_> {
    /// Quarter-chord sweep of `plane`'s main wing, degrees: the sweep of the
    /// quarter-chord line of the outboard trapezoidal panel, which is the
    /// reference-trapezoid sweep published for a transport wing.
    ///
    /// The design vector's `sweep_deg` is the leading-edge sweep the geometry
    /// builder lays out, not the quarter-chord sweep the Korn drag-divergence
    /// relation, the swept compressibility correction and the form factor are
    /// written for. Korn's relation is a 2-D section law carried to the wing
    /// by simple sweep theory (Mason, *Configuration Aerodynamics*, ch. 7,
    /// "the Korn equation"), so its sweep is that of the isobars of the swept
    /// outer wing, which a manufacturer publishes as the quarter-chord sweep
    /// of the reference trapezoid. An inboard trailing-edge extension
    /// (Yehudi) adds root chord without sweeping those isobars: the reference
    /// trapezoid is the outboard panel carried to the centreline, and the
    /// presets are drawn so that the outboard (kink-to-tip) panel reproduces
    /// each published value (A380 33.5, B787-9 32.2, A340 30.0, DC-10 35.0
    /// degrees; `alas-config` test `preset_sweep_conventions`). An
    /// area-weighted mean over every panel instead lets the low-sweep Yehudi
    /// panel pull the value 0.6 to 2.3 degrees below those published values.
    ///
    /// The outboard panel is found from the tip inward: consecutive panels
    /// (the builder subdivides each planform panel) whose quarter-chord line
    /// keeps the outermost panel's slope `dx/dy` in the planform (XY)
    /// projection, to [`SAME_PANEL_SLOPE_TOLERANCE`]. The sweep is
    /// `atan(dx / dy)` of that line. A single-panel wing returns its own
    /// sweep; a curved planform with no straight outboard run returns the
    /// outermost panel's.
    ///
    /// Panels steeper than 45 degrees (`|dz| > |dy|`), such as winglets, are
    /// not part of the lifting planform whose sweep these relations take:
    /// their quarter-chord line tends to 90 degrees as `dy` shrinks, so they
    /// are skipped.
    ///
    /// `fallback` is returned only when there is no main wing with a root and
    /// a tip section, or no retained panel has a positive finite span.
    pub fn quarter_chord_sweep_deg(plane: &Airplane, fallback: f64) -> f64 {
        let Some(wing) = plane.wings.first().filter(|wing| wing.xsecs.len() >= 2) else {
            return fallback;
        };
        let quarter_chord = |xsec: &WingXSec| [xsec.xyz_le[0] + 0.25 * xsec.chord, xsec.xyz_le[1]];
        // Lifting panels as (inboard quarter chord, outboard quarter chord,
        // slope), tip first.
        let mut panels = wing.xsecs.windows(2).rev().filter_map(|pair| {
            let dy = (pair[1].xyz_le[1] - pair[0].xyz_le[1]).abs();
            let dz = (pair[1].xyz_le[2] - pair[0].xyz_le[2]).abs();
            let (inboard, outboard) = (quarter_chord(&pair[0]), quarter_chord(&pair[1]));
            let slope = (outboard[0] - inboard[0]) / dy;
            (dz <= dy && dy > 0.0 && slope.is_finite()).then_some((inboard, outboard, slope))
        });
        let Some((mut inboard, outboard, tip_slope)) = panels.next() else {
            return fallback;
        };
        for (panel_inboard, _, slope) in panels {
            if (slope - tip_slope).abs() > SAME_PANEL_SLOPE_TOLERANCE {
                break;
            }
            inboard = panel_inboard;
        }
        (outboard[0] - inboard[0])
            .atan2((outboard[1] - inboard[1]).abs())
            .to_degrees()
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

    /// The Korn technology factor `kappa_A` of the analysed wing: the one its
    /// declared section class fixes, `geometry.wing.airfoil_class` (0.87
    /// conventional, 0.95 supercritical; Mason, *Configuration
    /// Aerodynamics*, ch. 7, the Korn equation; Malone and Mason 1995).
    pub fn korn_technology_factor(&self) -> f64 {
        self.geometry.wing.airfoil_class.korn_technology_factor()
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
        let (_, mach_crit) = self.korn_mach_numbers(cl, section_thickness);
        if mach > mach_crit {
            self.drag.wave_drag_coefficient * (mach - mach_crit).powf(4.0)
        } else {
            0.0
        }
    }

    /// The Korn drag-divergence and critical Mach numbers `(M_dd, M_crit)` at
    /// lift coefficient `cl`, the relations [`Self::wave_drag`] evaluates.
    ///
    /// `M_dd = kappa/cos(L) - t/c/cos^2(L) - CL/(10 cos^3(L))` (Korn, as
    /// presented in Mason, *Configuration Aerodynamics*, transonic-drag
    /// notes): the 2-D Korn relation `M_dd + cl/10 + t/c = kappa` applied to
    /// the flow normal to the sweep line, where simple sweep theory gives
    /// `M_n = M cos L`, `(t/c)_n = (t/c)/cos L` and `cl_n = CL/cos^2 L`.
    /// `L` is the quarter-chord sweep this analysis was built with, `kappa`
    /// [`Self::korn_technology_factor`] and `t/c` the Korn section thickness
    /// (`section_thickness`, else [`Self::korn_thickness`]).
    /// `M_crit = M_dd - (0.1/(4C))^(1/3)` inverts `dCD_w/dM = 0.1` on the
    /// Lock law `CD_w = C (M - M_crit)^4`; the frozen-law analysis, used only
    /// by objective parity fixtures generated with the old law, reports
    /// `M_crit = M_dd`. There is no onset-Mach gate here: this is the
    /// analytic value, not a drag.
    pub fn korn_mach_numbers(&self, cl: f64, section_thickness: Option<f64>) -> (f64, f64) {
        let thickness = section_thickness.unwrap_or_else(|| self.korn_thickness());
        let cos_sweep = self.sweep_deg.to_radians().cos();
        let mach_dd = self.korn_technology_factor() / cos_sweep
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
    fn a_yehudi_planform_reads_the_reference_trapezoid_sweep() {
        // One 34 degree leading edge from root to tip, with the inboard
        // trailing edge extended so that the inboard panel's quarter-chord
        // line is less swept than the outboard one (the AVE/777 layout). The
        // reference trapezoid is the outboard panel carried to the
        // centreline, so its quarter-chord sweep is the outboard panel's.
        let tan_le = 34.0_f64.to_radians().tan();
        let (kink_y, tip_y) = (12.5, 35.0);
        let (root_c, kink_c, tip_c) = (16.0, 8.0, 2.1);
        let outboard_c4 = (tan_le - 0.25 * (kink_c - tip_c) / (tip_y - kink_y))
            .atan()
            .to_degrees();
        let inboard_c4 = (tan_le - 0.25 * (root_c - kink_c) / kink_y)
            .atan()
            .to_degrees();
        // The builder subdivides each planform panel; two subdivisions of the
        // outboard panel must read as one panel.
        let mid_y = 0.5 * (kink_y + tip_y);
        let mid_c = 0.5 * (kink_c + tip_c);
        let plane = plane_with(&[
            ([0.0, 0.0, 0.0], root_c),
            ([kink_y * tan_le, kink_y, 0.5], kink_c),
            ([mid_y * tan_le, mid_y, 1.2], mid_c),
            ([tip_y * tan_le, tip_y, 2.0], tip_c),
        ]);
        let sweep = AeroAnalysis::quarter_chord_sweep_deg(&plane, 0.0);
        assert!(
            (sweep - outboard_c4).abs() < 1e-9,
            "{sweep} vs {outboard_c4}"
        );
        assert!(inboard_c4 < outboard_c4 - 3.0);
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

    /// `M_dd` of a straight test wing analysed at `sweep_deg` with the
    /// declared section class `class`.
    fn korn_mach_dd(
        class: alas_config::AirfoilClass,
        sweep_deg: f64,
        thickness: f64,
        cl: f64,
    ) -> f64 {
        let plane = plane_with(&[([0.0, 0.0, 0.0], 3.0), ([0.0, 8.0, 0.0], 3.0)]);
        let mut geometry = alas_config::GeometryConfig::default();
        geometry.wing.airfoil_class = class;
        AeroAnalysis::new(&plane, sweep_deg, Some(geometry), None, None)
            .korn_mach_numbers(cl, Some(thickness))
            .0
    }

    #[test]
    fn the_korn_relation_is_the_two_dimensional_law_on_the_sweep_normal_flow() {
        use alas_config::AirfoilClass::{Conventional, Supercritical};
        // Unswept, it is Korn's 2-D relation M_dd + cl/10 + t/c = kappa
        // (Mason, Configuration Aerodynamics, ch. 7): a 10 % supercritical
        // section at cl 0.5 diverges at 0.95 - 0.10 - 0.05 = 0.80, a 12 %
        // NACA 6-series section at cl 0.4 at 0.87 - 0.12 - 0.04 = 0.71.
        assert!((korn_mach_dd(Supercritical, 0.0, 0.10, 0.5) - 0.80).abs() < 1e-12);
        assert!((korn_mach_dd(Conventional, 0.0, 0.12, 0.4) - 0.71).abs() < 1e-12);
        // Swept, simple sweep theory applies the same law to the flow normal
        // to the sweep line: M_n = M cos L, (t/c)_n = (t/c)/cos L and
        // cl_n = CL/cos^2 L.
        for sweep_deg in [10.0_f64, 25.0, 35.0] {
            let cos = sweep_deg.to_radians().cos();
            let (thickness, cl) = (0.11, 0.55);
            let normal = 0.95 - thickness / cos - cl / (cos * cos) / 10.0;
            let swept = korn_mach_dd(Supercritical, sweep_deg, thickness, cl);
            assert!((swept * cos - normal).abs() < 1e-12, "{sweep_deg}");
        }
    }

    #[test]
    fn korn_divergence_rises_with_technology_and_sweep_and_falls_with_thickness_and_lift() {
        use alas_config::AirfoilClass::{Conventional, Supercritical};
        // Over the transport domain (c/4 sweep 0-40 deg, t/c 0.08-0.14,
        // CL 0.3-0.7) every term moves M_dd one way. Sweep raises it while
        // kappa cos^2 L > 2 (t/c) cos L + 0.3 CL, which holds throughout
        // (0.557 > 0.424 at the corner L = 40 deg, t/c 0.14, CL 0.7).
        for sweep in [0.0, 20.0, 40.0] {
            for thickness in [0.08, 0.11, 0.14] {
                for cl in [0.3, 0.5, 0.7] {
                    let base = korn_mach_dd(Supercritical, sweep, thickness, cl);
                    assert!(korn_mach_dd(Conventional, sweep, thickness, cl) < base);
                    assert!(korn_mach_dd(Supercritical, sweep + 0.5, thickness, cl) > base);
                    assert!(korn_mach_dd(Supercritical, sweep, thickness + 0.005, cl) < base);
                    assert!(korn_mach_dd(Supercritical, sweep, thickness, cl + 0.05) < base);
                }
            }
        }
        // The class shifts M_dd by the kappa difference over cos L.
        let cos = 30.0_f64.to_radians().cos();
        let shift = korn_mach_dd(Supercritical, 30.0, 0.11, 0.5)
            - korn_mach_dd(Conventional, 30.0, 0.11, 0.5);
        assert!((shift - (0.95 - 0.87) / cos).abs() < 1e-12);
    }

    /// Saving and reloading a configuration must not change its physics: a
    /// DC-10 (declared conventional) redeclared supercritical keeps
    /// kappa_A = 0.95, its drag-divergence Mach and its wave drag through a
    /// JSON round trip, though the preset reapplies its own class on load.
    #[test]
    fn a_saved_airfoil_class_that_differs_from_its_preset_survives_reload() {
        use alas_config::{AirfoilClass, AlasConfig};
        let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": "DC-10" }))
            .expect("the DC-10 preset loads");
        assert_eq!(
            config.geometry.wing.airfoil_class,
            AirfoilClass::Conventional
        );
        config.geometry.wing.airfoil_class = AirfoilClass::Supercritical;
        let saved = serde_json::to_value(&config).expect("the configuration serializes");
        let reloaded = AlasConfig::from_value(&saved).expect("the saved configuration loads");
        assert_eq!(
            reloaded.geometry.wing.airfoil_class,
            AirfoilClass::Supercritical
        );

        let tan_le = 35.0_f64.to_radians().tan();
        let plane = plane_with(&[([0.0, 0.0, 0.0], 10.0), ([20.0 * tan_le, 20.0, 0.0], 3.0)]);
        let physics = |config: &AlasConfig| {
            let analysis =
                AeroAnalysis::new(&plane, 35.0, Some(config.geometry.clone()), None, None);
            let (mach_dd, _) = analysis.korn_mach_numbers(0.5, Some(0.11));
            let wave = analysis.wave_drag(0.86, 0.5, Some(0.11));
            assert!(mach_dd.is_finite() && wave.is_finite() && wave >= 0.0);
            (analysis.korn_technology_factor(), mach_dd, wave)
        };
        let before = physics(&config);
        assert_eq!(before.0, 0.95);
        assert_eq!(physics(&reloaded), before);
    }
}
