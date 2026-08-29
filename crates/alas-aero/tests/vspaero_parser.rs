// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Deterministic characterization of VSPAERO's native batch-file boundary.

use alas_aero::vspaero::{parse_polar, ReferenceLengthUnit, VspaeroError, VspaeroModel};

const SETUP: &str = include_str!("fixtures/vspaero/tr1208.vspaero");
const POLAR: &str = include_str!("fixtures/vspaero/tr1208.polar");

#[test]
fn tr1208_native_polar_preserves_coefficients_and_reference_units() {
    let polar = parse_polar(
        POLAR,
        SETUP,
        ReferenceLengthUnit::Inch,
        VspaeroModel::ALAS_VLM,
    )
    .unwrap_or_else(|error| panic!("parse TR1208 VSPAERO fixture: {error}"));

    assert_eq!(polar.points.len(), 3);
    assert!((polar.reference.area_m2 - 2121.68 * 0.0254_f64.powi(2)).abs() < 1e-12);
    assert!((polar.reference.chord_m - 16.672 * 0.0254).abs() < 1e-12);
    assert!((polar.reference.span_m - 127.26 * 0.0254).abs() < 1e-12);
    assert_eq!(polar.reference.moment_reference_m, [0.0; 3]);
    assert_eq!(polar.points[0].span_efficiency, None);
    assert_eq!(polar.points[1].alpha_deg, 1.0);
    assert!((polar.points[1].lift_coefficient - 0.049329865452).abs() < 1e-14);
    assert!((polar.points[1].induced_drag_coefficient - 0.000106538687).abs() < 1e-14);
    assert!((polar.points[1].pitching_moment_coefficient + 0.098313969204).abs() < 1e-14);
}

#[test]
fn missing_native_columns_are_rejected_instead_of_defaulted() {
    let Err(error) = parse_polar(
        "Beta Mach AoA CLtot CDtot CMytot\n0 0 0 0 0 0\n",
        SETUP,
        ReferenceLengthUnit::Inch,
        VspaeroModel::ALAS_VLM,
    ) else {
        panic!("a polar without induced drag and reference Reynolds columns is incomplete");
    };
    assert!(matches!(error, VspaeroError::MissingColumn(_)));
}

#[test]
fn setup_without_a_moment_origin_is_rejected() {
    let Err(error) = parse_polar(
        POLAR,
        "Sref = 1\nCref = 1\nBref = 1\n",
        ReferenceLengthUnit::Meter,
        VspaeroModel::ALAS_VLM,
    ) else {
        panic!("moment normalization needs an explicit origin");
    };
    assert_eq!(error, VspaeroError::MissingSetupValue("X_cg"));
}
