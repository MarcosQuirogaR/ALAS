// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The cruise buffet-onset margin (`mdo::residuals_buffet`): every
//! registered aircraft's own design passes it, and a wing that carries its
//! cruise weight at a higher lift coefficient, or with less sweep, is
//! rejected by name.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_opt::{assess_product_candidate, CandidateAssessment};

fn assess(preset: &str, design: &DesignVector) -> CandidateAssessment {
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
        .unwrap_or_else(|error| panic!("{preset}: {error}"));
    assess_product_candidate(&config, design).unwrap_or_else(|error| panic!("{preset}: {error}"))
}

fn nominal(preset: &str) -> DesignVector {
    alas_config::presets::get(preset)
        .expect("registered preset")
        .design_vector
}

fn load_factor(assessment: &CandidateAssessment, id: &str) -> Option<(f64, f64)> {
    assessment
        .residuals
        .iter()
        .find(|r| r.id == id)
        .map(|r| (r.actual, r.limit))
}

#[test]
fn every_registered_jet_design_passes_its_buffet_margin() {
    let mut checked = 0;
    for preset in alas_config::presets::available() {
        let assessment = assess(preset, &nominal(preset));
        // Below the wave-drag onset Mach (the turboprop) no residual exists.
        let Some((n, floor)) = load_factor(&assessment, "buffet_margin") else {
            continue;
        };
        checked += 1;
        if preset == "DC-10" {
            // Documented exception. DC-10 cruises beyond model Mdd (0.800 vs
            // 0.82): 1970s aft-loaded DSMA airfoil class has no sourced Korn
            // technology factor; kappa held at the conventional 0.87 lower
            // bound (Mason ch. 7; Malone & Mason 1995). Finding, not tuning.
            // The reference-adaptation floor is the registered wing's own
            // model reading, so n meets it, but that reading is below the
            // 0.9 g plausibility bound. This flips when the model is fixed.
            assert!(n.is_finite() && floor.is_finite() && floor > 0.0);
            assert!(n >= floor - 1e-9, "{preset}: {n:.3} against {floor:.3}");
            assert!(
                floor < 0.9,
                "DC-10 buffet floor {floor:.3} g now meets the 0.9 g bound: remove this exception"
            );
            continue;
        }
        assert!(
            n >= floor - 1e-9 && floor > 0.9 && floor <= 1.3,
            "{preset}: load factor to buffet {n:.3} g against floor {floor:.3} g"
        );
        let (absolute, limit) = load_factor(&assessment, "buffet_margin_absolute")
            .expect("the absolute reading is reported beside the floor");
        assert_eq!((absolute, limit), (n, 1.3), "{preset}");
        assert!(
            !assessment.violated_hard_ids().contains(&"buffet_margin"),
            "{preset}"
        );
    }
    assert!(checked >= 6, "only {checked} jet presets were checked");
}

#[test]
fn a_smaller_or_less_swept_wing_loses_its_buffet_margin() {
    let preset = "B787-9";
    let base = nominal(preset);
    let (n0, _) = load_factor(&assess(preset, &base), "buffet_margin").unwrap();

    // Same span, 10 % less chord: aspect ratio up 11 %, cruise CL up by the
    // area ratio, buffet-onset CL unchanged.
    let mut smaller = base;
    smaller.root_chord_m *= 0.9;
    smaller.break_chord_m *= 0.9;
    smaller.tip_chord_m *= 0.9;
    let assessment = assess(preset, &smaller);
    let (n, _) = load_factor(&assessment, "buffet_margin").unwrap();
    assert!(n < n0, "{n} vs {n0}");
    assert!(assessment.violated_hard_ids().contains(&"buffet_margin"));

    // Less sweep lowers the drag-divergence boundary at the cruise Mach.
    let mut unswept = base;
    unswept.sweep_deg -= 3.0;
    let assessment = assess(preset, &unswept);
    let (n, _) = load_factor(&assessment, "buffet_margin").unwrap();
    assert!(n < n0, "{n} vs {n0}");
    assert!(assessment.violated_hard_ids().contains(&"buffet_margin"));
}
