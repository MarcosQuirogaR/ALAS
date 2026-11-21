// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The cruise buffet-onset margin (`mdo::residuals_buffet`): the load factor to
//! buffet onset against the absolute 1.3 g operating margin. The onset comes
//! from the offline MSES tables (`AeroAnalysis::buffet_onset`); the relative
//! reference floor is gone, so a registered wing that reads below 1.3 g is a
//! hard violation, reported rather than hidden.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
mod support;
use support::{assess_view, AssessmentView};

fn assess(preset: &str, design: &DesignVector) -> AssessmentView {
    assess_mode(preset, design, alas_config::MtowSizing::FixedRequirement)
}

fn assess_mode(
    preset: &str,
    design: &DesignVector,
    mode: alas_config::MtowSizing,
) -> AssessmentView {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
        .unwrap_or_else(|error| panic!("{preset}: {error}"));
    config.optimizer.objective.mtow_sizing = mode;
    // The nominal assessments are shared with the other residual suites.
    assess_view(&config, design).unwrap_or_else(|error| panic!("{preset}: {error}"))
}

fn nominal(preset: &str) -> DesignVector {
    alas_config::presets::get(preset)
        .expect("registered preset")
        .design_vector
}

fn load_factor(assessment: &AssessmentView, id: &str) -> Option<(f64, f64)> {
    assessment
        .residuals
        .iter()
        .find(|r| r.id == id)
        .map(|r| (r.actual, r.limit))
}

/// Every jet preset in `mode`: a finite positive reading against the absolute
/// 1.3 g floor at the start of cruise (initial cruise level, top-of-climb
/// mass). Every preset cruising at or above its wave-drag onset Mach carries
/// the row: a section family outside the tables would read an undefined value
/// (infeasible), which fails here. The closed-takeoff-mass reading is the
/// reported diagnostic `buffet_margin_mtow`.
///
/// Pins: the A320 and the B747-400 read above their floor at the start of
/// cruise with their declared takeoff mass (the 747 boundary is the
/// published one of Obert Fig. 28.10, p. 375, within 1 % at Mach 0.85).
fn check_nominal_presets(mode: alas_config::MtowSizing) {
    let mut checked = 0;
    for preset in alas_config::presets::available() {
        let assessment = assess_mode(preset, &nominal(preset), mode);
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset })).unwrap();
        let Some((n, floor)) = load_factor(&assessment, "buffet_margin") else {
            assert!(
                config.requirements.cruise_mach < config.drag_model.wave_drag_onset_mach,
                "{preset}: transonic cruise without a buffet_margin row"
            );
            continue;
        };
        checked += 1;
        assert!(n.is_finite() && n > 0.3 && n < 4.0, "{preset}: n {n:.3}");
        assert_eq!(floor, 1.3, "{preset}");
        let row = |id: &str| assessment.residuals.iter().find(|row| row.id == id);
        let residual = row("buffet_margin").unwrap();
        assert_eq!(residual.role, alas_opt::mdo::ResidualRole::Constraint);
        assert!(
            (residual.normalized_violation > 0.0) == (n < floor),
            "{preset}: n {n:.3}"
        );
        let mtow = row("buffet_margin_mtow").unwrap();
        assert_eq!(mtow.role, alas_opt::mdo::ResidualRole::Diagnostic);
        assert!(
            mtow.actual < n,
            "{preset}: MTOW n {} vs {n:.3}",
            mtow.actual
        );
        if mode == alas_config::MtowSizing::FixedRequirement {
            match preset {
                "A320-200" => assert!(n >= floor, "A320 n {n:.3}"),
                "B747-400" => assert!(n >= floor, "B747 n {n:.3}"),
                _ => {}
            }
        }
    }
    assert!(checked >= 6, "only {checked} jet presets were checked");
}

/// A published aircraft's own boundary is compared, not enforced: in the
/// baseline sandbox the start-of-cruise row is diagnostic and carries the
/// published CL_b, the model accuracy and the buffet-limited level.
#[test]
fn the_baseline_sandbox_reports_the_buffet_row_without_enforcing_it() {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": "B747-400" })).unwrap();
    config.optimizer.design_space.mode = alas_config::DesignMode::BaselineSandbox;
    let assessment = assess_view(&config, &nominal("B747-400")).unwrap();
    let row = assessment
        .residuals
        .iter()
        .find(|r| r.id == "buffet_margin")
        .unwrap();
    assert_eq!(row.role, alas_opt::mdo::ResidualRole::Diagnostic);
    let detail = row.detail.clone().unwrap_or_default();
    assert!(detail.contains("published CL_b 0.720"), "{detail}");
    assert!(detail.contains("within about 10 %"), "{detail}");
    assert!(
        detail.contains("level at which this mass holds"),
        "{detail}"
    );
    assert!(!assessment.violated_hard_ids().contains(&"buffet_margin"));
}

#[test]
fn mission_sized_registered_jet_designs_meet_their_buffet_floor() {
    check_nominal_presets(alas_config::MtowSizing::SizedByMission);
}

#[test]
fn hard_mtow_registered_jet_designs_meet_their_buffet_floor() {
    check_nominal_presets(alas_config::MtowSizing::FixedRequirement);
}

#[test]
// Read on the closed-takeoff-mass diagnostic: at the start of cruise the
// mission re-selects its initial level for a different wing, which moves q.
fn a_smaller_or_less_swept_wing_loses_its_buffet_margin() {
    // The A380-800 box is complete in the tables; the B787-9 reads through
    // partial cells, whose switching makes its margin non-monotone in chord.
    let preset = "A380-800";
    let base = nominal(preset);
    let (n0, _) = load_factor(&assess(preset, &base), "buffet_margin_mtow").unwrap();

    // Same span, 10 % less chord: aspect ratio up 11 %, cruise CL up by the
    // area ratio, buffet-onset CL unchanged.
    let mut smaller = base;
    smaller.root_chord_m *= 0.9;
    smaller.break_chord_m *= 0.9;
    smaller.tip_chord_m *= 0.9;
    let assessment = assess(preset, &smaller);
    let (n, _) = load_factor(&assessment, "buffet_margin_mtow").unwrap();
    assert!(n < n0, "{n} vs {n0}");

    // Less sweep lowers the drag-divergence boundary at the cruise Mach.
    let mut unswept = base;
    unswept.sweep_deg -= 3.0;
    let assessment = assess(preset, &unswept);
    let (n, _) = load_factor(&assessment, "buffet_margin_mtow").unwrap();
    assert!(n < n0, "{n} vs {n0}");
}

/// Reporting probe (run with `--ignored --nocapture`): n at the start of
/// cruise, n at the closed takeoff mass, and the row role, per preset and mode.
#[test]
#[ignore = "reporting probe"]
fn print_buffet_rows_per_mode() {
    for mode in [
        alas_config::DesignMode::BaselineSandbox,
        alas_config::DesignMode::ReferenceAdaptation,
        alas_config::DesignMode::CleanSheet,
    ] {
        for preset in alas_config::presets::available() {
            let mut config =
                AlasConfig::from_value(&serde_json::json!({ "preset": preset })).unwrap();
            config.optimizer.design_space.mode = mode;
            let Ok(a) = assess_view(&config, &nominal(preset)) else {
                println!("ROW {mode:?} {preset} assess failed");
                continue;
            };
            let find = |id: &str| a.residuals.iter().find(|r| r.id == id);
            println!(
                "ROW {mode:?} {preset} toc_n {:?} role {:?} mtow_n {:?}",
                find("buffet_margin").map(|r| (r.actual * 1000.0).round() / 1000.0),
                find("buffet_margin").map(|r| r.role),
                find("buffet_margin_mtow").map(|r| (r.actual * 1000.0).round() / 1000.0),
            );
            if let Some(r) = find("buffet_margin") {
                println!("DETAIL {mode:?} {preset} {:?}", r.detail);
            }
        }
    }
}
