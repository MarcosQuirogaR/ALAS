// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Audit SIZ-03: a coupled clean-sheet design sized by the mission (legacy
//! `SizedByMission` / `Unconstrained`) reads one landing mass in the gear
//! design, the dispatch limit and the `landing_mass` residual.
//!
//! Probe before the fix (default clean-sheet vector, declared MTOW
//! 358 670 kg, closure 280 656 kg, `mlw_fraction_mtow` 0.757): the label and
//! the residual limit were `0.757 x declared` = 271 663 kg, while the gear
//! and the dispatch limit used `0.757 x closure` = 212 4xx kg, a 28 %
//! mismatch; under `Unconstrained` the limit (212 574 kg) was below the
//! 235 662 kg the mission lands at, so the residual was violated by
//! construction.

// Assertions on values the test built itself: a failed expect is the
// assertion failing.
#![allow(clippy::expect_used)]

use alas_config::design_variables::DesignVector;
use alas_config::{AlasConfig, DesignMode, MtowSizing};
use alas_opt::assess_candidate;
use alas_opt::objective::DesignObjective;

fn assess(sizing: MtowSizing, declared_scale: f64) -> (AlasConfig, alas_opt::CandidateAssessment) {
    let mut config = AlasConfig::default();
    config.optimizer.design_space.mode = DesignMode::CleanSheet;
    config.optimizer.objective.mtow_sizing = sizing;
    config
        .optimizer
        .weights
        .transport_planform_constraints_enabled = false;
    config.requirements.mtow_kg *= declared_scale;
    let assessment = assess_candidate(
        &DesignObjective::new(config.clone()),
        &DesignVector::default().to_array(),
    )
    .unwrap_or_else(|reason| panic!("{sizing:?}: {reason}"));
    (config, assessment)
}

fn residual(assessment: &alas_opt::CandidateAssessment) -> (f64, f64) {
    let residual = assessment
        .residuals
        .iter()
        .find(|residual| residual.id == "landing_mass")
        .expect("landing_mass residual");
    (residual.limit, residual.actual)
}

#[test]
fn the_landing_limit_is_the_gear_design_landing_mass_and_not_a_fraction_of_the_declared_mtow() {
    for sizing in [MtowSizing::SizedByMission, MtowSizing::Unconstrained] {
        let (config, assessment) = assess(sizing, 1.0);
        let (limit_kg, landed_kg) = residual(&assessment);
        let sized = &assessment.sized;
        assert_eq!(sized.sizing_basis, "coupled");
        // One basis: the label the gear is designed at is the residual limit.
        assert!(
            (sized.design_landing_mass_kg - limit_kg).abs() < 1.0e-6,
            "{sizing:?}: design {} vs limit {limit_kg}",
            sized.design_landing_mass_kg
        );
        // It is the closure fraction raised to what the mission lands at,
        // so the residual is satisfiable on this short sector.
        let fraction_kg = config.mass_model.mlw_fraction_mtow * sized.takeoff_mass_kg;
        assert!(limit_kg >= fraction_kg - 1.0e-6, "{sizing:?}");
        assert!(
            landed_kg <= limit_kg + config.optimizer.objective.sizing_tolerance_kg,
            "{sizing:?}: {landed_kg} vs {limit_kg}"
        );
        // The declared MTOW is not a landing-mass basis (SIZ-03).
        let declared_fraction_kg =
            config.mass_model.mlw_fraction_mtow * config.requirements.mtow_kg;
        assert!(
            (limit_kg - declared_fraction_kg).abs() > 1.0,
            "{sizing:?}: limit {limit_kg} coincides with the declared-MTOW fraction"
        );
    }
}

#[test]
fn raising_the_declared_cap_does_not_move_the_landing_limit_of_an_unconstrained_closure() {
    let (_, base) = assess(MtowSizing::Unconstrained, 1.0);
    let (_, raised) = assess(MtowSizing::Unconstrained, 1.2);
    let (limit_base, _) = residual(&base);
    let (limit_raised, _) = residual(&raised);
    assert!(
        (limit_base - limit_raised).abs() < 1.0e-3 * limit_base,
        "{limit_base} vs {limit_raised}"
    );
}
