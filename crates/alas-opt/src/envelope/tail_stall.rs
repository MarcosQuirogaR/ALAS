// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The horizontal tail's download limit at takeoff rotation: the section
//! maximum lift coefficient of the tail airfoil at the rotation Mach and
//! Reynolds numbers, plus the elevator's own increment to it.
//!
//! Frame and sign conventions: the section and finite-wing coefficients here
//! are magnitudes (positive). The rotation moment balance
//! (`super::rotation`) takes the tail lift as a download (negative `CL_h`,
//! trailing edge of the elevator up), so the cap is applied as
//! `CL_h >= -cap`. The elevator deflection that raises the cap is the same
//! up-deflection that produces the download, so by mirror symmetry of the
//! symmetric tail section the plain-flap increment adds to the clean-section
//! value instead of subtracting from it.
//!
//! Sources (public documents, read on the page):
//! - Clean-section `c_l,max`: NACA 0012 free-transition (smooth) data of
//!   C. L. Ladson, NASA TM-4074 (1988), Figs. 24 (a, b) and 25 (a), digitised
//!   by eye, about +-0.02: `c_l,max` of 1.69, 1.67, 1.60, 1.43, 1.25 at
//!   `M` = 0.15, 0.20, 0.25, 0.30, 0.36 and `R` = 9e6; at `M` = 0.15 it is
//!   1.41, 1.58, 1.66, 1.69, 1.69 at `R` = 2, 4, 6, 9, 12 e6. Above
//!   `R` = 9e6 it does not change with Reynolds number.
//! - NACA 0006 and 0009 anchors: Abbott, von Doenhoff and Stivers, NACA
//!   Report 824 (1945), the table of low-speed data at `R` = 9e6 with a
//!   smooth leading edge (0006 0.92, 0009 1.32; also Fig. 39 (a), `R` = 6e6),
//!   scaled by the NACA 0012 Mach and Reynolds knockdown of Ladson [E].
//!   Between them and 0012 the value is linear in thickness; thicker
//!   symmetric sections follow the smooth 4-digit trend of Report 824,
//!   Fig. 39 (a) (24-series: 1.70, 1.62, 1.50, 1.42, 1.32 at 12, 15, 18, 21,
//!   24 %), as a ratio to its 12 % value [E].
//! - Plain-flap increment `Delta c_l,max = k1 k2 (Delta c_l,max)_base`: J.
//!   Roskam, *Airplane Design* Part VI, Figs. 8.31 (curve D, split and
//!   plain, any airfoil), 8.32 and 8.33 (split and plain curves, reference
//!   angle 60 deg), eq. 8.18 with `k3 = 1` for plain flaps; the figures are
//!   copied from USAF DATCOM and were digitised by eye, about +-0.05.
//!   Raymer, *Aircraft Design: A Conceptual Approach*, Table 12.2 gives the
//!   same base value, 0.9, for plain and split flaps.
//! - The finite-wing combination: Raymer eq. 12.21,
//!   `Delta CL,max = 0.9 Delta c_l,max (S_flapped / S_ref) cos Lambda_HL`,
//!   with the clean part `0.9 c_l,max cos Lambda_c/4`.
//!
//! Validity: the tail operates smooth (free transition). The NACA standard
//! roughness of Report 824 (0.011 in grit over the first 8 % chord) lowers
//! the NACA 0012 value to about 1.2 at `R` = 6e6 (Fig. 39 (a); Ladson's
//! #60 grit gives 1.22); Report 824 and Roskam VI both call it much more
//! severe than a real clean airframe, so it is not the design basis. Rotation
//! Mach numbers above 0.36 and Reynolds numbers below 2e6 are clamped to the
//! tabulated ends. The elevator hinge gap is taken sealed (the tests of
//! the flap data), as is the practice for transport elevators.

use super::chart::{interpolate, outside, ChartEnd};

/// ISA sea-level kinematic viscosity, m^2/s.
pub(super) const SEA_LEVEL_KINEMATIC_VISCOSITY_M2_S: f64 = 1.4607e-5;
/// ISA sea-level speed of sound, m/s.
pub(super) const SEA_LEVEL_SPEED_OF_SOUND_M_S: f64 = 340.294;

/// Smooth NACA 0012 `c_l,max` at `R` = 9e6 against Mach number (Ladson,
/// TM-4074, Fig. 24 (b)).
const MACH: [f64; 5] = [0.15, 0.20, 0.25, 0.30, 0.36];
const CL_MAX_0012_R9E6: [f64; 5] = [1.69, 1.67, 1.60, 1.43, 1.25];
/// Smooth NACA 0012 `c_l,max` at `M` = 0.15 against Reynolds number (Ladson,
/// TM-4074, Fig. 25 (a)).
const REYNOLDS: [f64; 5] = [2.0e6, 4.0e6, 6.0e6, 9.0e6, 12.0e6];
const CL_MAX_0012_M015: [f64; 5] = [1.41, 1.58, 1.66, 1.69, 1.69];

/// Thickness, `c_l,max` at `R` = 9e6, low Mach number, smooth: NACA Report
/// 824 for 0006 and 0009, Ladson (M = 0.15) for 0012.
const THICKNESS: [f64; 3] = [0.06, 0.09, 0.12];
const CL_MAX_LOW_MACH: [f64; 3] = [0.92, 1.32, 1.69];
/// Thickness ratio and `c_l,max` relative to the 12 % section of the smooth
/// 24-series (Report 824, Fig. 39 (a)).
const THICK_T: [f64; 5] = [0.12, 0.15, 0.18, 0.21, 0.24];
const THICK_RATIO: [f64; 5] = [1.0, 0.953, 0.882, 0.835, 0.776];

/// How a `(mach, reynolds)` condition lies outside the Mach and Reynolds
/// ranges of the section `c_l,max` charts ([`tail_section_cl_max`] then holds
/// the end value of the chart): `(mach, reynolds)` ends, `None` when both
/// are read inside their chart.
#[must_use]
pub(super) fn tail_section_chart_excursion(
    mach: f64,
    reynolds: f64,
) -> Option<(Option<ChartEnd>, Option<ChartEnd>)> {
    let ends = (outside(&MACH, mach), outside(&REYNOLDS, reynolds));
    (ends.0.is_some() || ends.1.is_some()).then_some(ends)
}

/// Maximum section lift coefficient of the tail airfoil (magnitude, smooth
/// flow) at the rotation Mach number `mach` and tail Reynolds number
/// `reynolds`.
///
/// Outside the charts' Mach (0.15 to 0.36) and Reynolds (2e6 to 12e6) ranges
/// the end value is held; [`tail_section_chart_excursion`] reports it.
///
/// Symmetric `naca00NN` sections only carry sourced data; any other name
/// takes the thinnest sourced section (NACA 0006), the lowest tabulated
/// value, so an unrecognised tail section can only under-credit the tail.
#[must_use]
pub(super) fn tail_section_cl_max(airfoil: &str, mach: f64, reynolds: f64) -> f64 {
    let lower = airfoil.trim().to_ascii_lowercase();
    let thickness = lower
        .strip_prefix("naca00")
        .filter(|digits| digits.len() == 2)
        .and_then(|digits| digits.parse::<u32>().ok())
        .map_or(THICKNESS[0], |percent| f64::from(percent) / 100.0);
    let mach = if mach.is_finite() { mach } else { MACH[4] };
    let reynolds = if reynolds.is_finite() {
        reynolds
    } else {
        REYNOLDS[0]
    };
    // Mach and Reynolds knockdown of the 0012 section, relative to its
    // low-Mach, R >= 9e6 value, assumed to carry over to other thicknesses.
    let knockdown = interpolate(&MACH, &CL_MAX_0012_R9E6, mach) / CL_MAX_0012_R9E6[0]
        * interpolate(&REYNOLDS, &CL_MAX_0012_M015, reynolds)
        / CL_MAX_0012_M015[3];
    let low_mach = if thickness <= THICKNESS[2] {
        interpolate(&THICKNESS, &CL_MAX_LOW_MACH, thickness)
    } else {
        CL_MAX_LOW_MACH[2] * interpolate(&THICK_T, &THICK_RATIO, thickness)
    };
    low_mach * knockdown
}

/// Plain-flap base increment `(Delta c_l,max)_base` against airfoil thickness
/// ratio (Roskam VI Fig. 8.31, curve D), for the 25 % reference flap chord
/// and the reference deflection of 60 deg.
const BASE_T: [f64; 12] = [
    0.0, 0.04, 0.06, 0.08, 0.09, 0.10, 0.12, 0.14, 0.16, 0.18, 0.19, 0.20,
];
const BASE_DELTA: [f64; 12] = [
    1.00, 0.97, 0.91, 0.82, 0.80, 0.805, 0.90, 1.08, 1.30, 1.52, 1.57, 1.60,
];
/// Flap-chord factor `k1`, plain flap (Roskam VI Fig. 8.32).
const K1_CF: [f64; 8] = [0.0, 0.02, 0.04, 0.08, 0.12, 0.16, 0.20, 0.30];
const K1: [f64; 8] = [0.0, 0.20, 0.33, 0.56, 0.71, 0.83, 0.92, 1.05];
/// Flap-angle factor `k2`, plain flap (Roskam VI Fig. 8.33).
const K2_DELTA: [f64; 7] = [0.0, 6.0, 12.0, 21.0, 33.0, 45.0, 60.0];
const K2: [f64; 7] = [0.0, 0.2, 0.4, 0.6, 0.8, 0.915, 1.0];

/// Section increment of maximum lift from a plain elevator of chord fraction
/// `chord_fraction` deflected `deflection_deg` (either sign) on a section of
/// thickness ratio `thickness`; `eq. 8.18` of Roskam VI with `k3 = 1`.
#[must_use]
pub(super) fn plain_flap_delta_cl_max(
    thickness: f64,
    chord_fraction: f64,
    deflection_deg: f64,
) -> f64 {
    if !thickness.is_finite() || !chord_fraction.is_finite() || !deflection_deg.is_finite() {
        return 0.0;
    }
    interpolate(&BASE_T, &BASE_DELTA, thickness)
        * interpolate(&K1_CF, &K1, chord_fraction)
        * interpolate(&K2_DELTA, &K2, deflection_deg.abs())
}

/// Thickness ratio of a `naca00NN` tail airfoil; `None` for any other name.
#[must_use]
pub(super) fn symmetric_thickness(airfoil: &str) -> Option<f64> {
    airfoil
        .trim()
        .to_ascii_lowercase()
        .strip_prefix("naca00")
        .filter(|digits| digits.len() == 2)
        .and_then(|digits| digits.parse::<u32>().ok())
        .map(|percent| f64::from(percent) / 100.0)
}

/// Fraction of a straight-tapered tail's planform area between the span
/// stations `eta0..eta1` (fractions of the semispan from the root).
#[must_use]
pub(super) fn strip_area_fraction(taper_ratio: f64, eta0: f64, eta1: f64) -> f64 {
    let lambda = if taper_ratio.is_finite() {
        taper_ratio
    } else {
        1.0
    };
    let integral = |eta: f64| eta - (1.0 - lambda) * eta * eta / 2.0;
    let (a, b) = (eta0.clamp(0.0, 1.0), eta1.clamp(0.0, 1.0));
    ((integral(b) - integral(a)) / integral(1.0)).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clean_section_value_follows_the_published_naca_0012_data() {
        // Ladson TM-4074 Fig. 24: low Mach, R >= 9e6.
        assert!((tail_section_cl_max("naca0012", 0.15, 12.0e6) - 1.69).abs() < 1.0e-9);
        assert_eq!(tail_section_chart_excursion(0.15, 12.0e6), None);
        assert_eq!(
            tail_section_chart_excursion(0.40, 1.9e7),
            Some((Some(ChartEnd::Above), Some(ChartEnd::Above)))
        );
        assert_eq!(
            tail_section_chart_excursion(0.20, 1.0e6),
            Some((None, Some(ChartEnd::Below)))
        );
        assert!((tail_section_cl_max("NACA0012", 0.25, 12.0e6) - 1.60).abs() < 1.0e-9);
        // Fig. 25 (a): M = 0.15 against Reynolds number.
        assert!((tail_section_cl_max("naca0012", 0.15, 4.0e6) - 1.58).abs() < 1.0e-9);
        // Mach and Reynolds only ever lower it.
        assert!(tail_section_cl_max("naca0012", 0.30, 12.0e6) < 1.5);
    }

    #[test]
    fn thinner_and_thicker_symmetric_sections_follow_the_sourced_trends() {
        assert!((tail_section_cl_max("naca0009", 0.15, 12.0e6) - 1.32).abs() < 1.0e-9);
        assert!((tail_section_cl_max("naca0006", 0.15, 12.0e6) - 0.92).abs() < 1.0e-9);
        let t10 = tail_section_cl_max("naca0010", 0.15, 12.0e6);
        assert!(1.32 < t10 && t10 < 1.69);
        assert!(tail_section_cl_max("naca0018", 0.15, 12.0e6) < 1.69);
        // An unrecognised section takes the thinnest sourced value.
        assert!((tail_section_cl_max("naca2412", 0.15, 12.0e6) - 0.92).abs() < 1.0e-9);
        assert!((tail_section_cl_max("something", 0.15, 12.0e6) - 0.92).abs() < 1.0e-9);
    }

    #[test]
    fn the_plain_elevator_increment_reproduces_the_chart_readings() {
        // 12 % thick, 30 % chord, 25 deg: 0.90 * 1.05 * k2(25).
        let k2 = 0.6 + (0.8 - 0.6) * (25.0 - 21.0) / (33.0 - 21.0);
        let expected = 0.90 * 1.05 * k2;
        assert!((plain_flap_delta_cl_max(0.12, 0.30, -25.0) - expected).abs() < 1.0e-9);
        // Reference flap (25 % chord, 60 deg) returns the base value.
        assert!((plain_flap_delta_cl_max(0.12, 0.25, 60.0) - 0.90 * 1.0 * 1.0).abs() < 0.02);
        assert_eq!(plain_flap_delta_cl_max(0.12, 0.30, 0.0), 0.0);
        assert_eq!(plain_flap_delta_cl_max(0.12, f64::NAN, 25.0), 0.0);
    }

    #[test]
    fn the_strip_fraction_is_the_planform_area_share() {
        assert!((strip_area_fraction(1.0, 0.2, 0.7) - 0.5).abs() < 1.0e-12);
        assert!((strip_area_fraction(0.3, 0.0, 1.0) - 1.0).abs() < 1.0e-12);
        // A tapered tail has more area inboard.
        assert!(strip_area_fraction(0.3, 0.0, 0.5) > 0.5);
        assert_eq!(strip_area_fraction(0.3, 0.7, 0.2), 0.0);
    }

    #[test]
    fn the_symmetric_thickness_parser_accepts_only_naca00nn() {
        assert_eq!(symmetric_thickness(" NACA0012 "), Some(0.12));
        assert_eq!(symmetric_thickness("naca2412"), None);
        assert_eq!(symmetric_thickness("naca001"), None);
    }
}
