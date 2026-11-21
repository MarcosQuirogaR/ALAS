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
const CONVERTED: [(&str, &str); 13] = [
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
    ("LJ60", "lj60"),
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

/// Built tail cone of every preset, aft of the geometric start of the cone: the
/// section area, perimeter, half-width and half-height decay monotonically (re-growth below
/// the numerical floor of the constraint, 1e-6 of the cabin value) and each of the crown, keel
/// and plan lines has at most one curvature sign change, the one S-bend that joins the cabin to
/// the closure.
#[test]
fn tail_cones_do_not_compress_and_expand() {
    const MAX_REVERSAL: f64 = 1e-6;
    for (preset, _) in CONVERTED {
        let pr = presets::get(preset).expect("preset");
        let (plane, _) = AircraftBuilder::new(Some(pr.geometry.clone()))
            .build_reported(Some(&pr.design_vector), true, WingShape::Flight)
            .expect("builds");
        let r = plane.fuselages[0]
            .loft
            .as_ref()
            .expect("loft")
            .tail_cone_report();
        assert!(r.is_smooth(MAX_REVERSAL, 1), "{preset}: {r:?}");
    }
}

/// The lofted outer section reproduces the drawn outer contour (S1): half-widths read from the
/// digitised figures (the digitised section traces, depth below the outer crown at the
/// middle of the cabin cylinder) against `Fuselage::skin_chord_m`. Tolerance per body: 3x the
/// recorded fit rms of the family, A320 0.03 m, A220 0.06 m (its figure ends at the floor), E195-E2 0.07 m.
#[test]
fn the_lofted_section_follows_the_drawn_outer_contour() {
    // (preset, tolerance m, [(depth below crown m, drawn half-width m)])
    type Drawn = [(f64, f64); 5];
    let cases: [(&str, f64, Drawn); 3] = [
        (
            "A320-200",
            0.03,
            [
                (0.4, 1.192),
                (0.8, 1.590),
                (1.2, 1.817),
                (1.6, 1.939),
                (2.0, 1.974),
            ],
        ),
        (
            "A220-300",
            0.06,
            [
                (0.4, 1.160),
                (0.8, 1.508),
                (1.2, 1.678),
                (1.6, 1.749),
                (2.0, 1.719),
            ],
        ),
        (
            "E195-E2",
            0.07,
            [
                (0.4, 1.013),
                (0.8, 1.323),
                (1.2, 1.471),
                (1.6, 1.503),
                (2.0, 1.439),
            ],
        ),
    ];
    for (preset, tolerance, points) in cases {
        let pr = presets::get(preset).expect("preset");
        let (plane, _) = AircraftBuilder::new(Some(pr.geometry.clone()))
            .build_reported(Some(&pr.design_vector), true, WingShape::Flight)
            .expect("builds");
        let fuselage = &plane.fuselages[0];
        let loft = fuselage.loft.as_ref().expect("loft");
        // The A220-300 loft no longer has a constant cabin cylinder (its drawn-outline fits);
        // the drawn contour is then read at the middle of the body.
        let (a, b) = loft.cabin_cylinder().unwrap_or_else(|| loft.x_range());
        let x = 0.5 * (a + b);
        let crown = loft.crown_z(x);
        for (depth, drawn) in points {
            let half = 0.5 * fuselage.skin_chord_m(x, crown - depth).expect("skin chord");
            assert!(
                (half - drawn).abs() <= tolerance,
                "{preset}: depth {depth} m: model {half:.3} m, drawn {drawn:.3} m"
            );
        }
    }
}

/// Study probe of the tail-cone report of every built preset (run with `--ignored --nocapture`).
#[test]
#[ignore = "probe"]
// Study probe: the report is printed on purpose.
#[allow(clippy::print_stdout)]
fn tail_cone_probe() {
    for (preset, _) in CONVERTED {
        let pr = presets::get(preset).expect("preset");
        let (plane, _) = AircraftBuilder::new(Some(pr.geometry.clone()))
            .build_reported(Some(&pr.design_vector), true, WingShape::Flight)
            .expect("builds");
        let loft = plane.fuselages[0].loft.as_ref().expect("loft");
        println!("{preset}: {:?}", loft.tail_cone_report());
        println!(
            "{preset} (aft of the cabin end {:.2} m): {:?}",
            loft.cabin_end(),
            loft.tail_cone_report_from(loft.cabin_end())
        );
        if let Ok(dir) = std::env::var("TAIL_DUMP") {
            let r = loft.tail_cone_report();
            let rows: Vec<String> = (0..=400)
                .map(|i| {
                    let x = r.x_start_m + r.length_m * f64::from(i) / 400.0;
                    let s = loft.section(x);
                    format!(
                        "[{x},{},{},{},{},{}]",
                        s.a,
                        s.zc + s.e,
                        s.zc - s.e,
                        loft.s_of_x(x),
                        s.perimeter()
                    )
                })
                .collect();
            std::fs::write(
                format!("{dir}/{preset}.json"),
                format!("[{}]", rows.join(",")),
            )
            .expect("write");
        }
    }
}
