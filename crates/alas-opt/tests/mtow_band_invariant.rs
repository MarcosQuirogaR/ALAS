// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The MTOW band mode is a relaxation of the fixed requirement.
//!
//! The band takes the takeoff mass `m` as a design variable in
//! `[T (1 - p), T (1 + p)]` and sizes everything that depends on it at `m`
//! through the fixed-requirement path. The invariant this file pins:
//!
//! > if the fixed requirement at `MTOW = m0` (with `m0` in the band) is
//! > feasible for a design vector `x`, then `(x, m0)` is feasible in band
//! > mode with the same constraint values.
//!
//! It is checked point-wise on several registered aircraft at several masses
//! inside the band: both modes evaluate the same `x` at the same `m0`, and
//! feasibility, the closed takeoff mass, the objective value and every
//! constraint and preference residual agree to the last bit. The band adds
//! only diagnostics (the closure position), which do not rank candidates.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::design_variables::DesignVector;
use alas_config::{AlasConfig, DesignMode, MtowSizing};
use alas_opt::mdo::ResidualRole;
use alas_opt::{assess_product_candidate, CandidateAssessment};

/// Registered aircraft spanning a turboprop, two narrow-bodies, a regional
/// jet and a long-range wide-body.
const PRESETS: [&str; 5] = ["ATR72-600", "A220-300", "A320-200", "E195-E2", "A340-300"];

/// Chosen masses as fractions of the target: both band edges, the target and
/// two interior points (default band fraction 10 %).
const FRACTIONS: [f64; 5] = [0.90, 0.95, 1.00, 1.05, 1.10];

fn preset_config(preset: &str, sizing: MtowSizing) -> (AlasConfig, DesignVector) {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": preset })).unwrap();
    config.optimizer.design_space.mode = DesignMode::ReferenceAdaptation;
    config.optimizer.objective.mtow_sizing = sizing;
    let design = alas_config::presets::get(preset).unwrap().design_vector;
    (config, design)
}

/// `(id, role, actual, limit, raw residual, normalised violation)` of every
/// residual that ranks candidates (constraints and preferences). Values are
/// compared by bit pattern: an unevaluable constraint (the jet take-off field
/// with no positive one-engine gradient) reports `NaN` limits in both modes,
/// and `NaN != NaN` would fail the comparison of two identical tables.
fn ranking_residuals(
    assessment: &CandidateAssessment,
) -> Vec<(&'static str, ResidualRole, u64, u64, u64, u64)> {
    assessment
        .residuals
        .iter()
        .filter(|residual| residual.role != ResidualRole::Diagnostic)
        .map(|residual| {
            (
                residual.id,
                residual.role,
                residual.actual.to_bits(),
                residual.limit.to_bits(),
                residual.raw_residual.to_bits(),
                residual.normalized_violation.to_bits(),
            )
        })
        .collect()
}

#[test]
fn a_fixed_requirement_point_inside_the_band_has_the_same_values_in_band_mode() {
    let mut feasible_points = 0;
    let mut checked = 0;
    for preset in PRESETS {
        let (band, nominal) = preset_config(preset, MtowSizing::MtowBand);
        let target_kg = band.mtow_band_target_kg();
        assert_eq!(band.optimizer.objective.mtow_band_fraction, 0.10);
        for fraction in FRACTIONS {
            let m0 = fraction * target_kg;
            let (lower_kg, upper_kg) = band.mtow_band_limits_kg();
            assert!(m0 >= lower_kg - 1e-6 && m0 <= upper_kg + 1e-6, "{preset}");

            // Fixed requirement at m0.
            let (mut fixed, design) = preset_config(preset, MtowSizing::FixedRequirement);
            fixed.requirements.mtow_kg = m0;
            let Ok(fixed_assessment) = assess_product_candidate(&fixed, &design) else {
                // A candidate the fixed path cannot evaluate at all (a
                // failed build or trim) must fail in the band the same way.
                let mut carried = nominal;
                carried.mtow_kg = Some(m0);
                assert!(
                    assess_product_candidate(&band, &carried).is_err(),
                    "{preset} {fraction}: the band evaluated a point the fixed path rejects"
                );
                continue;
            };

            // Band mode at (x, m0): the same design vector plus the mass.
            let mut carried = nominal;
            carried.mtow_kg = Some(m0);
            let band_assessment = assess_product_candidate(&band, &carried)
                .unwrap_or_else(|reason| panic!("{preset} {fraction}: {reason}"));

            assert_eq!(
                band_assessment.hard_feasible, fixed_assessment.hard_feasible,
                "{preset} at {fraction} T: feasibility"
            );
            assert_eq!(
                band_assessment.violated_hard_ids(),
                fixed_assessment.violated_hard_ids(),
                "{preset} at {fraction} T"
            );
            assert_eq!(
                ranking_residuals(&band_assessment),
                ranking_residuals(&fixed_assessment),
                "{preset} at {fraction} T: residual table"
            );
            assert_eq!(
                band_assessment.sized.takeoff_mass_kg, fixed_assessment.sized.takeoff_mass_kg,
                "{preset} at {fraction} T: closed takeoff mass"
            );
            assert_eq!(
                band_assessment.sized.design_gross_mass_kg, m0,
                "{preset} at {fraction} T: the structure is sized at m0"
            );
            assert_eq!(
                band_assessment.objective_value, fixed_assessment.objective_value,
                "{preset} at {fraction} T: objective value"
            );
            // The band adds the closure position as a diagnostic only.
            assert!(band_assessment
                .residuals
                .iter()
                .any(|residual| residual.id == "mtow_band_closure"
                    && residual.role == ResidualRole::Diagnostic));
            checked += 1;
            feasible_points += usize::from(fixed_assessment.hard_feasible);
        }
    }
    assert!(checked >= 20, "only {checked} points were evaluable");
    // The statement is vacuous if no point is feasible; the registered
    // aircraft at their own mass (fraction 1.0) are expected to be among
    // them, and the lighter points are allowed to be infeasible.
    assert!(
        feasible_points >= 1,
        "no feasible fixed-requirement point was exercised"
    );
}

#[test]
fn a_lighter_chosen_mass_sizes_a_lighter_structure() {
    // The structure follows m: the same design at 0.9 T has a smaller
    // design gross mass and a smaller operating empty mass than at 1.1 T.
    let (band, nominal) = preset_config("A320-200", MtowSizing::MtowBand);
    let target_kg = band.mtow_band_target_kg();
    let at = |fraction: f64| {
        let mut design = nominal;
        design.mtow_kg = Some(fraction * target_kg);
        assess_product_candidate(&band, &design).unwrap()
    };
    let (light, heavy) = (at(0.9), at(1.1));
    assert!(light.sized.design_gross_mass_kg < heavy.sized.design_gross_mass_kg);
    assert!(light.sized.operating_empty_mass_kg < heavy.sized.operating_empty_mass_kg);
}

#[test]
fn a_mass_outside_the_band_is_rejected_at_the_evaluator_boundary() {
    let (band, nominal) = preset_config("A320-200", MtowSizing::MtowBand);
    let (_, upper_kg) = band.mtow_band_limits_kg();
    let mut design = nominal;
    design.mtow_kg = Some(upper_kg * 1.01);
    assert_eq!(
        assess_product_candidate(&band, &design).unwrap_err(),
        "design_space"
    );
}
