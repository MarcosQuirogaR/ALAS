// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/data/airfoil_data.py.
// Reference: alas @ rust-port-baseline.

//! Reference airfoil coordinate data.
//!
//! This module holds *reference* airfoil geometries (raw section
//! coordinates). These are physical reference data, not user-tunable design
//! parameters: the user shapes airfoils through the morphing/bump
//! parameters in the design vector (see `alas-geom::airfoil_library`, not
//! yet translated), not by editing these numbers.
//!
//! Coordinates follow the Selig convention: a single closed loop starting at
//! the trailing edge, running forward over the upper surface to the leading
//! edge, then aft over the lower surface back to the trailing edge: raw,
//! not normalized into any particular winding, matching what
//! `alas-geom::selig::get` returns for the same reason.
//!
//! Unlike [`crate::selig`]'s 1,665-entry corpus, this registry holds one
//! hardcoded section (205 points, 3.4 KB as source text), so it is a plain
//! `const` array rather than an embedded data file: at this size a `const`
//! reads like ordinary Rust data and needs no parser.
//!
//! Unlike [`crate::selig::get`], lookup here is exact-match, not
//! case-folded: the reference is a plain Python `dict`, `NAMED_COORDINATES`,
//! keyed by the literal strings it was written with, and this reproduces
//! that rather than the corpus's folded index.

use std::sync::OnceLock;

/// Supercritical SC(2)-0714 section (root/break section of the reference
/// wing), `COORDS_SC2_0714` in the Python source, in file order.
#[rustfmt::skip]
const COORDS_SC2_0714: &[(f64, f64)] = &[
    (1.0, -0.0095),
    (0.99, -0.0063),
    (0.98, -0.0032),
    (0.97, -0.0002),
    (0.96, 0.0027),
    (0.95, 0.0056),
    (0.94, 0.0084),
    (0.93, 0.0111),
    (0.92, 0.0137),
    (0.91, 0.0162),
    (0.9, 0.0187),
    (0.89, 0.0211),
    (0.88, 0.0234),
    (0.87, 0.0256),
    (0.86, 0.0278),
    (0.85, 0.0299),
    (0.84, 0.0319),
    (0.83, 0.0339),
    (0.82, 0.0358),
    (0.81, 0.0376),
    (0.8, 0.0394),
    (0.79, 0.0411),
    (0.78, 0.0427),
    (0.77, 0.0443),
    (0.76, 0.0458),
    (0.75, 0.0473),
    (0.74, 0.0487),
    (0.73, 0.05),
    (0.72, 0.0513),
    (0.71, 0.0525),
    (0.7, 0.0537),
    (0.69, 0.0548),
    (0.68, 0.0559),
    (0.67, 0.0569),
    (0.66, 0.0579),
    (0.65, 0.0588),
    (0.64, 0.0597),
    (0.63, 0.0605),
    (0.62, 0.0613),
    (0.61, 0.0621),
    (0.6, 0.0628),
    (0.59, 0.0635),
    (0.58, 0.0641),
    (0.57, 0.0647),
    (0.56, 0.0653),
    (0.55, 0.0658),
    (0.54, 0.0663),
    (0.53, 0.0668),
    (0.52, 0.0672),
    (0.51, 0.0676),
    (0.5, 0.068),
    (0.49, 0.0683),
    (0.48, 0.0686),
    (0.47, 0.0689),
    (0.46, 0.0691),
    (0.45, 0.0693),
    (0.44, 0.0695),
    (0.43, 0.0696),
    (0.42, 0.0697),
    (0.41, 0.0698),
    (0.4, 0.0699),
    (0.39, 0.0699),
    (0.38, 0.0699),
    (0.37, 0.0699),
    (0.36, 0.0698),
    (0.35, 0.0697),
    (0.34, 0.0696),
    (0.33, 0.0694),
    (0.32, 0.0692),
    (0.31, 0.0689),
    (0.3, 0.0686),
    (0.29, 0.0683),
    (0.28, 0.0679),
    (0.27, 0.0675),
    (0.26, 0.067),
    (0.25, 0.0665),
    (0.24, 0.066),
    (0.23, 0.0654),
    (0.22, 0.0648),
    (0.21, 0.0641),
    (0.2, 0.0633),
    (0.19, 0.0625),
    (0.18, 0.0616),
    (0.17, 0.0607),
    (0.16, 0.0597),
    (0.15, 0.0586),
    (0.14, 0.0574),
    (0.13, 0.0562),
    (0.12, 0.0549),
    (0.11, 0.0535),
    (0.1, 0.0519),
    (0.09, 0.0502),
    (0.08, 0.0484),
    (0.07, 0.0463),
    (0.06, 0.044),
    (0.05, 0.0414),
    (0.04, 0.0383),
    (0.03, 0.0346),
    (0.02, 0.0296),
    (0.01, 0.0224),
    (0.005, 0.01658),
    (0.002, 0.01077),
    (0.0, 0.0),
    (0.002, -0.01077),
    (0.005, -0.01658),
    (0.01, -0.0224),
    (0.02, -0.0296),
    (0.03, -0.0345),
    (0.04, -0.0382),
    (0.05, -0.0413),
    (0.06, -0.0439),
    (0.07, -0.0462),
    (0.08, -0.0483),
    (0.09, -0.0501),
    (0.1, -0.0518),
    (0.11, -0.0534),
    (0.12, -0.0549),
    (0.13, -0.0562),
    (0.14, -0.0574),
    (0.15, -0.0586),
    (0.16, -0.0597),
    (0.17, -0.0607),
    (0.18, -0.0616),
    (0.19, -0.0625),
    (0.2, -0.0633),
    (0.21, -0.0641),
    (0.22, -0.0648),
    (0.23, -0.0655),
    (0.24, -0.0661),
    (0.25, -0.0667),
    (0.26, -0.0672),
    (0.27, -0.0677),
    (0.28, -0.0681),
    (0.29, -0.0685),
    (0.3, -0.0688),
    (0.31, -0.0691),
    (0.32, -0.0693),
    (0.33, -0.0695),
    (0.34, -0.0696),
    (0.35, -0.0697),
    (0.36, -0.0697),
    (0.37, -0.0697),
    (0.38, -0.0696),
    (0.39, -0.0695),
    (0.4, -0.0693),
    (0.41, -0.0691),
    (0.42, -0.0688),
    (0.43, -0.0685),
    (0.44, -0.0681),
    (0.45, -0.0677),
    (0.46, -0.0672),
    (0.47, -0.0667),
    (0.48, -0.0661),
    (0.49, -0.0654),
    (0.5, -0.0646),
    (0.51, -0.0637),
    (0.52, -0.0627),
    (0.53, -0.0616),
    (0.54, -0.0604),
    (0.55, -0.0591),
    (0.56, -0.0577),
    (0.57, -0.0562),
    (0.58, -0.0546),
    (0.59, -0.0529),
    (0.6, -0.0511),
    (0.61, -0.0492),
    (0.62, -0.0473),
    (0.63, -0.0453),
    (0.64, -0.0433),
    (0.65, -0.0412),
    (0.66, -0.0391),
    (0.67, -0.037),
    (0.68, -0.0348),
    (0.69, -0.0326),
    (0.7, -0.0304),
    (0.71, -0.0282),
    (0.72, -0.026),
    (0.73, -0.0238),
    (0.74, -0.0216),
    (0.75, -0.0194),
    (0.76, -0.0173),
    (0.77, -0.0152),
    (0.78, -0.0132),
    (0.79, -0.0113),
    (0.8, -0.0095),
    (0.81, -0.0079),
    (0.82, -0.0064),
    (0.83, -0.005),
    (0.84, -0.0038),
    (0.85, -0.0028),
    (0.86, -0.002),
    (0.87, -0.0014),
    (0.88, -0.001),
    (0.89, -0.0008),
    (0.9, -0.0009),
    (0.91, -0.0012),
    (0.92, -0.0017),
    (0.93, -0.0025),
    (0.94, -0.0036),
    (0.95, -0.005),
    (0.96, -0.0067),
    (0.97, -0.0087),
    (0.98, -0.011),
    (0.99, -0.0136),
    (1.0, -0.0165),
];

/// Registry of built-in reference sections, addressable by name:
/// `NAMED_COORDINATES` in the Python source. `(name, coordinates)` pairs in
/// declaration order.
const NAMED_COORDINATES: &[(&str, &[(f64, f64)])] = &[("SC2-0714", COORDS_SC2_0714)];

/// A name-sorted view of [`NAMED_COORDINATES`], built once.
///
/// The registry itself is declared in source order (currently one entry) to
/// mirror the Python dict literal; this is only for [`names`], which mirrors
/// `selig::stems`'s sorted-listing contract.
fn sorted_names() -> &'static [&'static str] {
    static NAMES: OnceLock<Vec<&'static str>> = OnceLock::new();
    NAMES
        .get_or_init(|| {
            let mut names: Vec<&'static str> =
                NAMED_COORDINATES.iter().map(|&(name, _)| name).collect();
            names.sort_unstable();
            names
        })
        .as_slice()
}

/// Look up a built-in reference section by exact name (`NAMED_COORDINATES`'s
/// Python dict lookup is exact-match, not case-folded, unlike
/// `selig::get`).
///
/// Returns its `(x, y)` coordinate pairs in file order, not normalized
/// into any particular winding, which is `AirfoilLibrary.normalize_coordinates`'s
/// job in a later module, `alas-geom::airfoil_library`.
pub fn get(name: &str) -> Option<&'static [(f64, f64)]> {
    NAMED_COORDINATES
        .iter()
        .find(|&&(entry_name, _)| entry_name == name)
        .map(|&(_, coordinates)| coordinates)
}

/// Every name the registry has, sorted.
///
/// Mirrors `selig::stems`'s contract: part of what
/// `AirfoilLibrary.get_available_airfoils` needs, merging this list with the
/// Selig corpus's.
pub fn names() -> &'static [&'static str] {
    sorted_names()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_resolves_the_registered_supercritical_section() {
        let coords = get("SC2-0714").expect("SC2-0714 is in the registry");
        assert_eq!(coords.len(), 205);
        assert_eq!(coords[0], (1.0, -0.0095));
        assert_eq!(coords[coords.len() - 1], (1.0, -0.0165));
    }

    #[test]
    fn get_is_exact_case_not_folded() {
        // Unlike selig::get, this is a plain Python dict lookup: neither
        // case variant nor an unregistered name resolves.
        assert!(get("sc2-0714").is_none());
        assert!(get("SC2-0714 ").is_none());
        assert!(get("not-a-real-section").is_none());
    }

    #[test]
    fn names_are_sorted_and_include_the_registered_section() {
        let all = names();
        let mut sorted = all.to_vec();
        sorted.sort_unstable();
        assert_eq!(all, sorted);
        assert!(all.contains(&"SC2-0714"));
    }

    #[test]
    fn the_section_is_a_closed_selig_loop() {
        // Starts and ends at the trailing edge (x == 1.0) and passes through
        // the leading edge (x == 0.0) somewhere in between, per the Selig
        // convention this module's doc comment describes.
        let coords = get("SC2-0714").expect("SC2-0714 is in the registry");
        assert_eq!(coords.first().expect("non-empty").0, 1.0);
        assert_eq!(coords.last().expect("non-empty").0, 1.0);
        assert!(coords.iter().any(|&(x, _)| x == 0.0));
    }
}
