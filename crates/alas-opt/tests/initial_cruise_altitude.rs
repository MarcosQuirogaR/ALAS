// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The initial cruise altitude (ICA) requirement of a turbofan.
//!
//! Undeclared, nothing is required: the achievable ICA is a Diagnostic and
//! the engine is sized exactly as before. Declared, the planner's 300 ft/min
//! residual climb at maximum-climb thrust at that level, at the brake-release
//! mass less the integrated take-off and climb fuel, is a thrust row of the
//! engine sizing: the sized engine meets it, and asking for a higher level
//! never asks for less engine.
//!
//! The A320-200 brief with FL330 at MTOW is the requirement case. FL330 is a
//! representative A320 initial level, not a verified published ICA:
//! no primary source (FCOM altitude-capability chart) was in hand, so the test
//! asserts the physical consistency of the binding, not a match to a figure.

// A test asserts on values it built from a brief it controls, so a failed
// unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stderr)]

use alas_config::optimizer::DesignMode;
use alas_config::{presets, AlasConfig, DesignVector};
use alas_opt::{assess_product_candidate, canonicalize_design, CandidateAssessment};
use serde_json::json;

const FOOT_M: f64 = 0.3048;

fn a320_clean_sheet(ica_ft: Option<f64>) -> (AlasConfig, DesignVector) {
    let mut config = AlasConfig::from_value(&json!({ "preset": "A320-200" })).unwrap();
    config.optimizer.design_space.mode = DesignMode::CleanSheet;
    config.requirements.initial_cruise_altitude_m = ica_ft.map(|ft| ft * FOOT_M);
    let preset = presets::registry()
        .iter()
        .find(|p| p.name == "A320-200")
        .unwrap();
    let design = canonicalize_design(&config, preset.design_vector).unwrap();
    (config, design)
}

fn assess(ica_ft: Option<f64>) -> CandidateAssessment {
    let (config, design) = a320_clean_sheet(ica_ft);
    assess_product_candidate(&config, &design).unwrap_or_else(|reason| panic!("{reason}"))
}

fn rating_kn(a: &CandidateAssessment) -> f64 {
    a.resolved
        .sized_engine
        .as_ref()
        .expect("a clean-sheet jet sizes its engine")
        .engine
        .scaling_rating()
        .unwrap()
}

fn find<'a>(a: &'a CandidateAssessment, id: &str) -> Option<&'a alas_opt::ConstraintResidual> {
    a.residuals.iter().find(|row| row.id == id)
}

#[test]
fn a_declared_initial_cruise_altitude_sizes_the_engine_and_an_undeclared_one_only_reports() {
    let free = assess(None);
    assert!(find(&free, "initial_cruise_altitude").is_none());
    let capability =
        find(&free, "initial_cruise_altitude_capability").expect("the achievable ICA is reported");
    assert_eq!(capability.role, alas_opt::mdo::ResidualRole::Diagnostic);
    assert_eq!(capability.normalized_violation, 0.0);
    let achievable_m = capability.actual;
    eprintln!(
        "A320 undeclared: rating {:.2} kN, achievable ICA {:.0} m (FL{:.0})",
        rating_kn(&free),
        achievable_m,
        achievable_m / FOOT_M / 100.0
    );

    let mut previous = rating_kn(&free);
    for ica_ft in [33_000.0, 37_000.0] {
        let sized = assess(Some(ica_ft));
        let row = find(&sized, "initial_cruise_altitude").expect("the declared ICA row");
        eprintln!(
            "A320 ICA FL{:.0}: rating {:.2} kN, residual climb {:.3} m/s, {}",
            ica_ft / 100.0,
            rating_kn(&sized),
            row.actual,
            row.detail.as_deref().unwrap_or("")
        );
        assert_eq!(row.role, alas_opt::mdo::ResidualRole::Constraint);
        // Sized to the requirement: the residual climb holds the criterion.
        assert!(
            row.raw_residual <= 1.0e-5 * row.limit,
            "FL{:.0}: {row:?}",
            ica_ft / 100.0
        );
        let rating = rating_kn(&sized);
        // A requirement adds a row: it never asks for less engine, and a
        // higher level never for less than a lower one.
        assert!(
            rating >= previous * (1.0 - 1.0e-3),
            "FL{:.0}: {rating} kN below {previous} kN",
            ica_ft / 100.0
        );
        // A level the undeclared design already reaches does not bind.
        if ica_ft * FOOT_M <= achievable_m {
            assert!(
                (rating / rating_kn(&free) - 1.0).abs() < 1.0e-3,
                "FL{:.0} is achievable yet moved the rating: {rating} vs {}",
                ica_ft / 100.0,
                rating_kn(&free)
            );
        }
        previous = rating;
    }
}
