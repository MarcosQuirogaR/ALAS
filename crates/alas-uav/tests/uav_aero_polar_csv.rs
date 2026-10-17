// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Failed assertions identify broken fixtures in this test target.
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Polar import: CSV and XFLR5/XFOIL-style text, SYNTHETIC numbers only.

use alas_uav::aero::polar_csv::PolarTable;
use alas_uav::aero::section::SectionModel;
use alas_uav::aero::{Flag, Severity};

const CSV: &str = "# synthetic polar, not measured data\n\
alpha_deg,cl,cd,cm,re\n\
-4,-0.10,0.0200,-0.05,100000\n\
0,0.40,0.0120,-0.05,100000\n\
4,0.90,0.0130,-0.05,100000\n\
8,1.30,0.0200,-0.04,100000\n\
12,1.20,0.0600,-0.03,100000\n\
-4,-0.12,0.0150,-0.05,200000\n\
0,0.42,0.0090,-0.05,200000\n\
4,0.95,0.0100,-0.05,200000\n\
8,1.40,0.0150,-0.04,200000\n\
12,1.50,0.0400,-0.03,200000\n";

const XFLR5: &str = "xflr5 v6.47\n\
\n\
 Calculated polar for: SYNTH 1234\n\
\n\
 1 1 Reynolds number fixed          Mach number fixed\n\
\n\
 xtrf =   1.000 (top)        1.000 (bottom)\n\
 Mach =  0.000     Re =     0.200 e 6     Ncrit =   9.000\n\
\n\
  alpha     CL        CD       CDp       Cm    Top Xtr Bot Xtr\n\
 ------- -------- --------- --------- -------- ------- -------\n\
  -2.000   0.0000   0.01000   0.00500  -0.0500  0.6000  1.0000\n\
   0.000   0.2500   0.00900   0.00450  -0.0500  0.5000  1.0000\n\
   2.000   0.5000   0.00950   0.00480  -0.0500  0.4000  1.0000\n\
   4.000   0.7500   0.01100   0.00600  -0.0500  0.3000  1.0000\n";

#[test]
fn csv_with_two_reynolds_numbers_is_grouped_and_converted_to_radians() {
    let t = PolarTable::parse("synthetic", CSV, None).unwrap();
    assert_eq!(t.sets.len(), 2);
    assert_eq!(t.re_range(), (1.0e5, 2.0e5));
    assert!((t.sets[0].alpha_rad[1] - 0.0).abs() < 1e-15);
    assert!((t.sets[0].alpha_rad[4] - 12.0_f64.to_radians()).abs() < 1e-15);
    assert!(t.sets[0].has_cm);
}

#[test]
fn interpolates_in_alpha_and_log_reynolds_without_extrapolating() {
    let t = PolarTable::parse("synthetic", CSV, None).unwrap();
    // Node value.
    let p = t.eval(4.0_f64.to_radians(), 1.0e5);
    assert!((p.cl - 0.90).abs() < 1e-12 && (p.cd - 0.0130).abs() < 1e-12);
    assert!(p.validity.flags.is_empty());
    // Midpoint in alpha between 0 and 4 deg at Re 1e5.
    let p = t.eval(2.0_f64.to_radians(), 1.0e5);
    assert!((p.cl - 0.65).abs() < 1e-12);
    // Geometric-mean Reynolds number: halfway in ln Re.
    let re = (1.0e5_f64 * 2.0e5).sqrt();
    let p = t.eval(0.0, re);
    assert!((p.cl - 0.41).abs() < 1e-12 && (p.cd - 0.0105).abs() < 1e-12);
    // Outside the angle table: clamped and flagged, not extrapolated.
    let p = t.eval(20.0_f64.to_radians(), 1.0e5);
    assert!((p.cl - 1.20).abs() < 1e-12);
    assert!(p
        .validity
        .flags
        .iter()
        .any(|(s, f)| *s == Severity::Warn && matches!(f, Flag::OutsideTable(_))));
    // Reynolds number far outside: nearest set, flagged.
    let p = t.eval(0.0, 1.0e7);
    assert!((p.cl - 0.42).abs() < 1e-12 && p.validity.severity() == Severity::Warn);
}

#[test]
fn xflr5_text_with_a_preamble_reynolds_number_is_read() {
    let t = PolarTable::parse("synth", XFLR5, None).unwrap();
    assert_eq!(t.sets.len(), 1);
    assert!((t.sets[0].re - 2.0e5).abs() < 1e-6);
    assert_eq!(t.sets[0].alpha_rad.len(), 4);
    assert!((t.sets[0].cl[3] - 0.75).abs() < 1e-12);
    assert!((t.sets[0].cm[0] + 0.05).abs() < 1e-12);
}

#[test]
fn section_model_from_a_table_reports_table_flags_not_neuralfoil_ones() {
    let section = SectionModel::from_table(PolarTable::parse("synthetic", CSV, None).unwrap());
    let p = section.eval(2.0_f64.to_radians(), 1.5e5, 0.03).unwrap();
    assert!(p.validity.flags.is_empty(), "{:?}", p.validity);
    assert!(p.cl > 0.6 && p.cl < 0.8);
}

#[test]
fn malformed_tables_are_rejected_with_a_line_number() {
    // No Reynolds number anywhere.
    let e = PolarTable::parse(
        "x",
        "alpha,cl,cd\n0,0.1,0.01\n1,0.2,0.01\n2,0.3,0.01\n",
        None,
    );
    assert!(e.is_err());
    // The same table with a default Reynolds number is fine.
    assert!(PolarTable::parse(
        "x",
        "alpha,cl,cd\n0,0.1,0.01\n1,0.2,0.01\n2,0.3,0.01\n",
        Some(1e5)
    )
    .is_ok());
    // Non-numeric cell: line 3.
    let e = PolarTable::parse(
        "x",
        "alpha,cl,cd\n0,0.1,0.01\n1,abc,0.01\n2,0.3,0.01\n",
        Some(1e5),
    );
    assert!(matches!(
        e,
        Err(alas_uav::aero::AeroError::Csv { line: 3, .. })
    ));
    // Non-positive drag.
    assert!(PolarTable::parse(
        "x",
        "alpha,cl,cd\n0,0.1,0.0\n1,0.2,0.01\n2,0.3,0.01\n",
        Some(1e5)
    )
    .is_err());
    // Duplicate angle.
    assert!(PolarTable::parse(
        "x",
        "alpha,cl,cd\n0,0.1,0.01\n0,0.2,0.01\n2,0.3,0.01\n",
        Some(1e5)
    )
    .is_err());
    // Too few points.
    assert!(PolarTable::parse("x", "alpha,cl,cd\n0,0.1,0.01\n1,0.2,0.01\n", Some(1e5)).is_err());
    // No header.
    assert!(PolarTable::parse("x", "0,0.1,0.01\n1,0.2,0.01\n2,0.3,0.01\n", Some(1e5)).is_err());
    // Semicolon delimiter and unordered rows are accepted and sorted.
    let t = PolarTable::parse(
        "x",
        "alpha;cl;cd\n2;0.3;0.01\n0;0.1;0.01\n1;0.2;0.01\n",
        Some(1e5),
    )
    .unwrap();
    assert!(t.sets[0].alpha_rad.windows(2).all(|w| w[1] > w[0]));
}
