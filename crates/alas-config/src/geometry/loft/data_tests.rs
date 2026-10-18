// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! One test per embedded loft data file: schema, validation, the recorded
//! fairness budget and fit record, and the extent rule against the preset
//! (loft widest section and tallest section vs `diameter_m` and `height_m`,
//! at most 0.5 % apart so the builder stays silent).
//!
//! The extents are read off the fitted curves directly: the widest section is
//! `2 max sqrt(A)` and the tallest is `2 max sqrt(E)` (the warped
//! superellipse reaches `zc +- e` at `zeta = +-1` for `|k| < 0.5`), the same
//! quantities the builder's extent check measures (per section, not over the
//! whole body).

// A failed expect is the assertion failing here, not a library invariant.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_math::clamped_bspline::ClampedBSpline;

use super::{embedded_loft, parse_loft_spec, preset_loft_id, LoftSpec};

/// Relative tolerance of the extent rule that keeps the builder silent.
const EXTENT_SILENT: f64 = 0.005;
fn curve(spec: &LoftSpec, which: &str) -> ClampedBSpline {
    let c = match which {
        "A" => &spec.curves.a,
        "E" => &spec.curves.e,
        _ => &spec.curves.zc,
    };
    ClampedBSpline::new(c.knots.clone(), c.coeffs.clone(), usize::from(spec.degree))
        .expect("a validated curve builds")
}

/// `(max width, max height)` of the lofted body, m (largest section, as the builder measures it).
fn extents(spec: &LoftSpec) -> (f64, f64) {
    let (a, e) = (curve(spec, "A"), curve(spec, "E"));
    let (mut width, mut height) = (0.0_f64, 0.0_f64);
    for i in 0..=4000 {
        let u = f64::from(i) / 4000.0;
        width = width.max(2.0 * a.eval(u).max(0.0).sqrt());
        height = height.max(2.0 * e.eval(u).max(0.0).sqrt());
    }
    (width, height)
}

/// Fit-residual gate of the drawing class recorded for the source: `(rms, max)` in metres.
/// Class A/B drawings (vector PDF, clean raster): 6 cm / 15 cm; class C (scan, render): 12 cm / 30 cm.
const GATE_AB: (f64, f64) = (0.06, 0.151);
const GATE_C: (f64, f64) = (0.12, 0.301);

fn check(
    id: &str,
    preset_name: &str,
    json: &str,
    budget: (u32, u32, u32),
    tv_max: f64,
    gate: (f64, f64),
) {
    // schema and validation (parse_loft_spec does both)
    let spec = parse_loft_spec(json).unwrap_or_else(|e| panic!("{id}: {e}"));
    assert_eq!(spec.schema, 1);
    assert_eq!(spec.degree, 5, "{id}: quintic B-splines");
    assert_eq!(spec.preset, preset_name);
    assert_eq!(preset_loft_id(preset_name), Some(id));
    assert_eq!(embedded_loft(id).as_ref(), Some(&spec));
    // the fairness budget and the fit record are stored in the file
    assert!((spec.fairness.tv_ratio_max - tv_max).abs() < 1e-12, "{id}");
    assert_eq!(spec.fairness.inflections.crown, budget.0, "{id}");
    assert_eq!(spec.fairness.inflections.keel, budget.1, "{id}");
    assert_eq!(spec.fairness.inflections.plan, budget.2, "{id}");
    if (budget, tv_max) != ((1, 2, 2), 1.5) {
        assert!(
            spec.provenance
                .iter()
                .any(|g| g.note.contains("FAIRNESS BUDGET RAISED")),
            "{id}: a raised budget must be documented in the file"
        );
    }
    assert!(
        spec.fit.rms_m > 0.0 && spec.fit.rms_m < gate.0,
        "{id}: rms (gate {} m)",
        gate.0
    );
    assert!(
        spec.fit.max_m >= spec.fit.rms_m && spec.fit.max_m < gate.1,
        "{id}: max"
    );
    assert!(!spec.fit.tool_version.is_empty());
    assert!(!spec.provenance.is_empty(), "{id}: provenance");
    // the extent rule against the preset
    let preset = crate::presets::get(preset_name).expect("preset exists");
    let g = &preset.geometry.fuselage;
    let (width, height) = extents(&spec);
    let rel_w = (width - g.diameter_m).abs() / g.diameter_m;
    // the builder's expectation: declared section height plus the full upper-deck rise
    let expected_h = g.effective_height_m() + g.upper_deck_hump().map_or(0.0, |hump| hump.height_m);
    let rel_h = (height - expected_h).abs() / expected_h;
    assert!(
        rel_w <= EXTENT_SILENT,
        "{id}: width {width} vs {}",
        g.diameter_m
    );
    assert!(
        rel_h <= EXTENT_SILENT,
        "{id}: height {height} vs {expected_h}"
    );
    // built at the preset length: no remap
    let length_ratio = preset.design_vector.fuselage_length_m / spec.reference_length_m;
    assert!(
        (length_ratio - 1.0).abs() < 1e-9,
        "{id}: length ratio {length_ratio}"
    );
}

macro_rules! data_file_test {
    ($test:ident, $id:literal, $preset:literal, $budget:expr, $tv:expr, $gate:expr) => {
        #[test]
        fn $test() {
            check(
                $id,
                $preset,
                include_str!(concat!("../../../data/lofts/", $id, ".json")),
                $budget,
                $tv,
                $gate,
            );
        }
    };
}

// (crown, keel, plan) inflection budget and TV-ratio limit recorded in the file:
// the absolute 1 / 2 / 2 and 1.5 everywhere except the E195-E2 (documented in
// its provenance: the drawn radome / windshield corner at x = 3.5-3.9 m).
data_file_test!(
    a220_300_file,
    "a220_300",
    "A220-300",
    (1, 2, 2),
    1.5,
    GATE_AB
);
data_file_test!(
    a320_200_file,
    "a320_200",
    "A320-200",
    (1, 2, 2),
    1.5,
    GATE_AB
);
data_file_test!(
    a340_300_file,
    "a340_300",
    "A340-300",
    (1, 2, 2),
    1.5,
    GATE_AB
);
// Tail-cone refit 2026-10-12 (geo/tailcone-2026-10-12): the aft body of the A380-800 (x >= 57.73 m)
// and the A400M (x >= 29.1 m) was refitted to the calibrated side drawings with the forward
// coefficients frozen. Recorded fit (old -> new): A380-800 rms 2.72 -> 2.34 cm, max 7.85 -> 7.85 cm
// over the drawn samples (the 0.8 m APU-cone step, x 70.3-72.3 m, is not representable and is
// excluded); A400M rms 4.59 -> 4.37 cm, max 25.76 -> 25.76 cm (forward, frozen). Fairness (crown / keel
// / plan inflections, TV): A380-800 0/2/2 -> 1/2/2, TV 1.37 -> 1.39; A400M 1/1/0 -> 1/1/0, TV 1.03.
// Built overlay fus_aft side over: A380-800 1.071 -> 0.257 m2, A400M 0.078 -> 0.000 m2.
data_file_test!(
    a380_800_file,
    "a380_800",
    "A380-800",
    (1, 2, 2),
    1.5,
    GATE_AB
);
data_file_test!(a400m_file, "a400m", "A400M", (1, 2, 2), 1.5, GATE_C);
data_file_test!(
    atr72_600_file,
    "atr72_600",
    "ATR72-600",
    (1, 2, 2),
    1.5,
    GATE_C
);
data_file_test!(ave_file, "ave", "AVE", (1, 2, 2), 1.5, GATE_AB);
data_file_test!(
    b747_400_file,
    "b747_400",
    "B747-400",
    (1, 2, 2),
    1.5,
    GATE_AB
);
data_file_test!(b787_9_file, "b787_9", "B787-9", (1, 2, 2), 1.5, GATE_AB);
data_file_test!(c919_file, "c919", "C919", (1, 2, 2), 1.5, GATE_AB);
data_file_test!(dc_10_file, "dc_10", "DC-10", (1, 2, 2), 1.5, GATE_C);
data_file_test!(e195_e2_file, "e195_e2", "E195-E2", (2, 2, 2), 1.7, GATE_AB);
