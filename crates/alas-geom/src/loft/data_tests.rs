// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Data-file tests that need the geometry crate: every converted preset builds
//! its lofted fuselage without a build warning (the extent rule of the builder)
//! and meets the fairness budget recorded in its file under the Rust sampled
//! metric (`Loft::fairness`): zero violations, G2 at every knot, one maximum of
//! the section area. The budget is the absolute 1 / 2 / 2 inflections with a TV
//! ratio of at most 1.5, raised explicitly (and documented in the file) only for
//! the E195-E2 crown. The builder's extent rule is silent for every file.

use alas_config::{embedded_loft, preset_loft_id, presets, WingShape};

use super::Loft;
use crate::builder::AircraftBuilder;

/// `(preset, loft id)`.
const CONVERTED: [(&str, &str); 12] = [
    ("A220-300", "a220_300"),
    ("A320-200", "a320_200"),
    ("A340-300", "a340_300"),
    ("A380-800", "a380_800"),
    ("A400M", "a400m"),
    ("ATR72-600", "atr72_600"),
    ("AVE", "ave"),
    ("B747-400", "b747_400"),
    ("B787-9", "b787_9"),
    ("C919", "c919"),
    ("DC-10", "dc_10"),
    ("E195-E2", "e195_e2"),
];

fn per_file(preset: &str, id: &str) {
    assert_eq!(preset_loft_id(preset), Some(id));
    let spec = embedded_loft(id).expect("embedded and valid");
    let loft = Loft::from_reference(&spec).expect("builds");
    let fairness = loft.fairness();
    assert!(
        fairness.violations.is_empty(),
        "{id}: {:?}",
        fairness.violations
    );
    assert!(fairness.min_exponent >= 2.0 - 1e-9 && fairness.min_squared_extent >= -1e-9);
    assert_eq!(fairness.area_maxima, 1, "{id}: one maximum of S(x)");
    for line in &fairness.lines {
        assert!(line.knot_jump < 1e-6, "{id}: {} not G2", line.name);
    }

    // the preset builds the loft, at its own length, with no extent warning
    let pr = presets::get(preset).expect("preset");
    assert!(
        pr.geometry.fuselage.loft.is_some(),
        "{preset} references its loft"
    );
    let (plane, report) = AircraftBuilder::new(Some(pr.geometry.clone()))
        .build_reported(Some(&pr.design_vector), true, WingShape::Flight)
        .expect("builds");
    // The final merge sets the B747-400 / DC-10 reference trapezoid against their published areas
    // (-1.6 % / -2.2 %, deliberately reported); only that check may appear.
    let others: Vec<_> = report
        .warnings
        .iter()
        .filter(|w| w.code != "reference_wing_published_area_mismatch")
        .collect();
    assert!(others.is_empty(), "{id}: {:?}", report.warnings);
    assert!(plane.fuselages[0].loft.is_some());
}

#[test]
fn a220_300_file_builds_and_meets_its_fairness_budget() {
    let (p, i) = CONVERTED[0];
    per_file(p, i);
}

#[test]
fn a320_200_file_builds_and_meets_its_fairness_budget() {
    let (p, i) = CONVERTED[1];
    per_file(p, i);
}

#[test]
fn a340_300_file_builds_and_meets_its_fairness_budget() {
    let (p, i) = CONVERTED[2];
    per_file(p, i);
}

#[test]
fn a380_800_file_builds_and_meets_its_fairness_budget() {
    let (p, i) = CONVERTED[3];
    per_file(p, i);
}

#[test]
fn a400m_file_builds_and_meets_its_fairness_budget() {
    let (p, i) = CONVERTED[4];
    per_file(p, i);
}

#[test]
fn atr72_600_file_builds_and_meets_its_fairness_budget() {
    let (p, i) = CONVERTED[5];
    per_file(p, i);
}

#[test]
fn ave_file_builds_and_meets_its_fairness_budget() {
    let (p, i) = CONVERTED[6];
    per_file(p, i);
}

#[test]
fn b747_400_file_builds_and_meets_its_fairness_budget() {
    let (p, i) = CONVERTED[7];
    per_file(p, i);
}

#[test]
fn b787_9_file_builds_and_meets_its_fairness_budget() {
    let (p, i) = CONVERTED[8];
    per_file(p, i);
}

#[test]
fn c919_file_builds_and_meets_its_fairness_budget() {
    let (p, i) = CONVERTED[9];
    per_file(p, i);
}

#[test]
fn dc_10_file_builds_and_meets_its_fairness_budget() {
    let (p, i) = CONVERTED[10];
    per_file(p, i);
}

#[test]
fn e195_e2_file_builds_and_meets_its_fairness_budget() {
    let (p, i) = CONVERTED[11];
    per_file(p, i);
}
