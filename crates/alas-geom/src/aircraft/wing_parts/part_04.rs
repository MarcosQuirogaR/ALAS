// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Native addition, not part of the upstream port documented in the parent
// module, see `2026-09-09-claude-mac-parity-derivation.html` under
// `.agent/reports/` for the derivation this method is grounded in.

impl Wing {
    /// The manufacturer "theoretical" or reference-wing mean aerodynamic
    /// chord: the trapezoid formed by extending the outermost lofted panel's
    /// leading and trailing edges straight to [`Self::xsecs`]'s first
    /// station, discarding every inboard station (crank, side-of-body, or
    /// fuselage carry-through).
    ///
    /// [`Self::mean_aerodynamic_chord`] integrates the wing's actual lofted
    /// panels, including any inboard crank or side-of-body station: the
    /// physically correct chord for lift/moment reference. Manufacturers
    /// commonly state a type-certificate MAC (and the `%MAC` balance datum
    /// derived from it) from the theoretical wing instead, because the
    /// inboard carry-through is not a real tapered aerodynamic surface. The
    /// two are different, both legitimate, quantities; this method exists so
    /// a caller can reproduce the TCDS/WBM convention without silently
    /// replacing the physical one.
    ///
    /// Numerically verified against two real cranked (Yehudi) transport
    /// wings' EASA type-certificate data sheets in this module's tests: it
    /// reproduces the B787-9 certified MAC to 0.14% and demonstrably does
    /// *not* reproduce the A340-300 certified MAC (the A340 preset's crank
    /// station is an area-calibrated fit rather than a source-drawn point,
    /// see the report above). Do not assume this closes every cranked-wing
    /// discrepancy; it is evidence for one specific convention, not a
    /// universal correction.
    ///
    /// Degenerates to [`Self::mean_aerodynamic_chord`] when there is no
    /// inboard station to discard (exactly two cross-sections).
    ///
    /// # Panics
    ///
    /// Never on a [`Wing`] with at least two cross-sections, which every
    /// constructible [`Wing`] in this crate has (there is no way to build one
    /// with fewer through the public API).
    pub fn theoretical_reference_mac(&self) -> f64 {
        let last = self.xsecs.len() - 1;
        let spans = self.sectional_spans_yz();
        let outer_panel_span = spans[last - 1];
        let s_last: f64 = spans.iter().sum();

        let c_last = self.xsecs[last].chord;
        let c_second_last = self.xsecs[last - 1].chord;
        let slope = (c_last - c_second_last) / outer_panel_span;
        // Extrapolate the outer panel's chord law back to s = 0 (this
        // wing's first cross-section), which is usually the aircraft
        // centerline.
        let virtual_root_chord = c_last - s_last * slope;

        let a = s_last * (virtual_root_chord + c_last) / 2.0;
        let ic = s_last
            * (virtual_root_chord.powi(2) + virtual_root_chord * c_last + c_last.powi(2))
            / 3.0;
        ic / a
    }
}

#[cfg(test)]
mod theoretical_reference_mac_tests {
    use super::*;

    fn naca(name: &str) -> Airfoil {
        Airfoil::from_name(name).expect("valid 4-digit NACA name")
    }

    #[test]
    fn matches_mean_aerodynamic_chord_with_no_inboard_station_to_discard() {
        // Only one panel exists, so "the outer panel extended to the root"
        // is the same trapezoid the physical integral already uses.
        let wing = Wing::new(
            "Trapezoid",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 4.0, 0.0, naca("naca0012")),
                WingXSec::new([2.0, 20.0, 0.0], 1.0, 0.0, naca("naca0012")),
            ],
            true,
        );
        assert!(
            (wing.theoretical_reference_mac() - wing.mean_aerodynamic_chord()).abs() < 1e-9,
            "reference={} physical={}",
            wing.theoretical_reference_mac(),
            wing.mean_aerodynamic_chord()
        );
    }

    #[test]
    fn discarding_a_steeper_inboard_crank_shrinks_the_reference_mac() {
        // A steep root-to-kink carry-through panel (chord 14 -> 9 over 8 m)
        // followed by a shallower true taper (9 -> 3 over 22 m) to the tip:
        // the same qualitative shape as the real cranked wings in the tests
        // below, where the fuselage-side chord decreases faster than the
        // exposed outer panel. Extending only the shallower outer panel's law
        // inward gives a virtual root (11.18 m) below the physical 14 m root,
        // so the reference MAC must sit below the physical one. Whether
        // discarding a crank raises or lowers the MAC depends on the relative
        // steepness of the two panels in general (it is not a universal
        // direction); this fixes one concrete, worked-out case.
        let wing = Wing::new(
            "Cranked",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 14.0, 0.0, naca("naca0012")),
                WingXSec::new([1.0, 8.0, 0.0], 9.0, 0.0, naca("naca0012")),
                WingXSec::new([3.0, 30.0, 0.0], 3.0, 0.0, naca("naca0012")),
            ],
            true,
        );
        assert!((wing.mean_aerodynamic_chord() - 8.627_976).abs() < 1e-6);
        assert!((wing.theoretical_reference_mac() - 7.877_622).abs() < 1e-6);
        assert!(wing.theoretical_reference_mac() < wing.mean_aerodynamic_chord());
    }

    /// B787-9 transport planform (root/side-of-body/kink/tip), reconstructed
    /// from `crates/alas-config/src/presets/widebody_parts/part_02.rs`'s
    /// design vector (span 60.12 m, root/break/tip chord 12.60/6.50/1.60 m,
    /// sweep 32.2 deg, `WingConfig::default()`'s side-of-body span fraction
    /// 0.10, kink span fraction 0.353771245388011_8) through
    /// `WingConfig::transport_planform`'s exact station algebra. z is left at
    /// 0 (the real preset has small dihedral offsets that change this by
    /// under 0.01%, immaterial to the convention this test checks).
    #[test]
    fn reproduces_the_boeing_787_9_certified_reference_mac() {
        let wing = Wing::new(
            "B787-9",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 12.6000, 0.0, naca("naca0012")),
                WingXSec::new([1.8930, 3.0060, 0.0], 10.8757, 0.0, naca("naca0012")),
                WingXSec::new([6.6968, 10.6344, 0.0], 6.5000, 0.0, naca("naca0012")),
                WingXSec::new([18.9298, 30.0600, 0.0], 1.6000, 0.0, naca("naca0012")),
            ],
            true,
        );
        // EASA TCDS IMA.115 Issue 30 (2025), certified reference MAC 6.27126 m
        // (golden/aircraft/real_aircraft_parity.json, cite
        // easa_tcds_ima115_i30_2025). The physical (crank-inflated) MAC this
        // preset's model export currently reports is 7.54779 m, +20.36%
        // against the same anchor.
        let certified_mac_m = 6.27126;
        let reference_mac_m = wing.theoretical_reference_mac();
        let relative_error = (reference_mac_m / certified_mac_m - 1.0).abs();
        assert!(
            relative_error < 0.005,
            "reference MAC {reference_mac_m} m should be within 0.5% of the certified {certified_mac_m} m, got {:.3}%",
            relative_error * 100.0
        );
    }

    /// A340-300 transport planform, reconstructed the same way from
    /// `crates/alas-config/src/presets/widebody_parts/part_01.rs`'s design
    /// vector (span 60.30 m, root/break/tip chord 12.00/6.50/1.80 m, sweep
    /// 30 deg, default side-of-body span fraction 0.10, kink span fraction
    /// 0.362094754983253_8).
    ///
    /// This is a documented *negative* result, not a bug in the method under
    /// test: the A340 preset's crank station is fit to reproduce Airbus's
    /// published 361.6 sq m reference wing area (see that file's comment),
    /// not read from a certified drawing, so there is no source-evidenced
    /// station to extrapolate from. The theoretical-wing construction is
    /// therefore not expected to reconcile this preset with the certified
    /// EASA TCDS A.015 MAC (7.27 m), and this test exists so a future change
    /// does not silently assume it does.
    #[test]
    fn does_not_reconcile_the_a340_300_certified_reference_mac() {
        let wing = Wing::new(
            "A340-300",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 12.0000, 0.0, naca("naca0012")),
                WingXSec::new([1.7407, 3.0150, 0.0], 10.4811, 0.0, naca("naca0012")),
                WingXSec::new([6.3030, 10.9172, 0.0], 6.5000, 0.0, naca("naca0012")),
                WingXSec::new([17.4071, 30.1500, 0.0], 1.8000, 0.0, naca("naca0012")),
            ],
            true,
        );
        let certified_mac_m = 7.27;
        let reference_mac_m = wing.theoretical_reference_mac();
        let relative_error = (reference_mac_m / certified_mac_m - 1.0).abs();
        assert!(
            relative_error > 0.05,
            "this preset's crank station is unsourced, so the reference-wing MAC {reference_mac_m} m \
             is not expected to sit within 5% of the certified {certified_mac_m} m; a future change that \
             makes this converge means the underlying preset geometry changed and this test's comment \
             (and the parity report) needs to be revisited, not silently loosened"
        );
    }
}
