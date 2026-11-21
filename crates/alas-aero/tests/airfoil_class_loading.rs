// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Every preset draws wing sections of the loading class it declares.
//!
//! [`AirfoilClass`] declares the section technology: conventional sections
//! (the NACA 6-series era and the peaky sections of the 1960s) carry little
//! rear loading, supercritical sections are aft-loaded (C. D. Harris, "NASA
//! Supercritical Airfoils", NASA TP-2969, 1990). The camber-line measure of
//! rear loading is the thin-airfoil pitching moment about the quarter chord,
//! the integral the wing-body `Cm_ac` is built from, so a section of the
//! wrong class moves the trim and the forward centre-of-gravity limits with
//! it: the DC-10 drawing the aft-loaded DSMA-523A (`c_m,c/4` -0.149) had a
//! clean wing-body `Cm_ac` near -0.17. The boundary is the least aft-loaded
//! member of the family that defines the supercritical class, SC(2)-0410
//! (design lift 0.4): a conventional preset draws no section at or beyond
//! its moment, a supercritical preset none short of it.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_aero::fourier_lifting_line::thin_airfoil_quarter_chord_moment;
use alas_config::{presets, AirfoilClass};
use alas_geom::airfoil_library::AirfoilLibrary;

fn quarter_chord_moment(name: &str) -> f64 {
    let airfoil =
        AirfoilLibrary::get(name).unwrap_or_else(|| panic!("{name} is not in the airfoil library"));
    thin_airfoil_quarter_chord_moment(&airfoil).unwrap()
}

#[test]
fn every_preset_draws_sections_of_its_declared_loading_class() {
    let boundary = quarter_chord_moment("sc20410");
    let mut checked = 0;
    for name in presets::available() {
        let wing = &presets::get(name).unwrap().geometry.wing;
        for section in [&wing.root_airfoil, &wing.tip_airfoil] {
            let moment = quarter_chord_moment(section);
            match wing.airfoil_class {
                AirfoilClass::Conventional | AirfoilClass::Peaky => assert!(
                    moment > boundary,
                    "{name}: conventional class, but {section} is aft-loaded \
                     (c_m,c/4 {moment:.4}, SC(2)-0410 {boundary:.4})"
                ),
                AirfoilClass::Supercritical | AirfoilClass::SupercriticalAdvanced => assert!(
                    moment <= boundary,
                    "{name}: supercritical class, but {section} carries less rear \
                     loading than SC(2)-0410 (c_m,c/4 {moment:.4} > {boundary:.4})"
                ),
            }
            checked += 1;
        }
    }
    assert!(checked >= 30, "only {checked} sections checked");
}

#[test]
fn the_conventional_surrogate_pair_carries_little_rear_loading() {
    // The documented moments of the Boeing 737 root and outboard stations:
    // little rear loading, of the size the 747 stations of Obert, Aerodynamic
    // Design of Transport Aircraft (2009), Fig. 24.82, digitise to (-0.014 to
    // -0.044).
    let (root, outboard) = AirfoilClass::CONVENTIONAL_TRANSPORT_SECTIONS;
    let (root, outboard) = (quarter_chord_moment(root), quarter_chord_moment(outboard));
    assert!((root - -0.013).abs() < 0.0015, "root c_m,c/4 {root:.4}");
    assert!(
        (outboard - -0.022).abs() < 0.0015,
        "outboard c_m,c/4 {outboard:.4}"
    );
}
