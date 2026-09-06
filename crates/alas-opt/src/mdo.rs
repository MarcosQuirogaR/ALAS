// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mission-sized design objectives: block fuel, takeoff mass, operating
//! empty mass or fuel per seat-kilometre, closed by the design mission under
//! the configured fuel policy and ranked feasibility first.
//!
//! [`crate::objective::DesignObjective::evaluate`] delegates here for every
//! [`alas_config::ObjectiveKind`] that
//! [`alas_config::ObjectiveKind::is_mission_sized`] reports true, except the
//! frozen reference-compatibility replay, which keeps the legacy weighted
//! penalty so its parity fixture stays meaningful.
//!
//! The evaluation is a pipeline of four stages, one module each:
//! [`build`] evaluates the geometry, mass and trimmed aerodynamic operating
//! point that do not depend on the takeoff mass; [`sizing`] closes the
//! takeoff-mass fixed point analytically against that trimmed drag polar;
//! [`residuals`] turns the sized candidate into a typed table, one entry per
//! requirement, instead of folding every requirement into a single weighted
//! penalty; and [`cost`] assembles that table into the scalar the search
//! minimises, ranking feasibility ahead of the objective value.

mod build;
mod cost;
mod engine;
mod range;
mod residuals;
mod residuals_geometry;
mod residuals_performance;
mod sizing;
mod tanks;
mod types;

pub use types::{CandidateAssessment, ConstraintFamily, ConstraintResidual, SizedCandidate};

use alas_config::design_variables::DesignVector;

use crate::objective::DesignObjective;

/// Evaluate a mission-sized objective for candidate design vector `x`,
/// recording the result into `objective`'s history and returning the scalar
/// cost.
///
/// Called from [`crate::objective::DesignObjective::evaluate`]; not meant to
/// be called directly on a kind that is not mission-sized.
pub fn evaluate_mission_sized(objective: &mut DesignObjective, x: &[f64]) -> f64 {
    let weights = objective.config.optimizer.weights.clone();
    match sizing::run_candidate(&objective.config, x) {
        Ok(outcome) => {
            let history = outcome.history;
            let residuals = residuals::build(
                &outcome,
                &objective.config,
                &weights,
                objective.target_num_passengers,
                objective.target_cargo_payload_kg,
            );
            let assessment = cost::assemble(outcome, &objective.config, residuals);
            let reason = assessment.violated_hard_ids().join("+");
            objective.history.record_mission_sized(
                history.dv,
                assessment.hard_feasible,
                assessment.cost,
                assessment.sized.lift_to_drag,
                history.span_m,
                history.alpha_deg,
                history.area_m2,
                history.trim_ih_deg,
                reason,
                assessment.objective_value,
                assessment.sized.takeoff_mass_kg,
                assessment.sized.block_fuel_kg,
                assessment.hard_violation_sum,
                assessment.soft_violation_sum,
            );
            assessment.cost
        }
        Err(failure) => {
            let cost = weights.failure_cost;
            // `CandidateFailure` carries only the reason, not the design
            // vector (see its doc comment); rebuild it from `x` for the
            // history entry, falling back to the default vector exactly as
            // the legacy path does when `x` itself does not parse.
            let dv = DesignVector::from_array(x).unwrap_or_default();
            // A candidate that never reached a sized takeoff mass has no
            // finite objective value or L/D; the kernels in
            // `differential_evolution_parts::part_01::scored_point` already
            // rank a non-finite or nonpositive L/D below every physical
            // miss, which is why this stays `0.0` rather than a placeholder
            // positive number.
            objective.history.record_mission_sized(
                dv,
                false,
                cost,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
                failure.reason,
                f64::NAN,
                f64::NAN,
                f64::NAN,
                f64::NAN,
                f64::NAN,
            );
            cost
        }
    }
}

/// Assess candidate design vector `x` without recording history, for a
/// caller (the pipeline's finalist report) that wants the residual table
/// alone.
///
/// # Errors
///
/// The evaluation-failure reason (`geometry_build`, `mass_coordinates`,
/// `payload_layout` or `trim_solve`) when the candidate could not be built,
/// sized or trimmed at all.
pub fn assess_candidate(
    objective: &DesignObjective,
    x: &[f64],
) -> Result<CandidateAssessment, String> {
    let weights = objective.config.optimizer.weights.clone();
    match sizing::run_candidate(&objective.config, x) {
        Ok(outcome) => {
            let residuals = residuals::build(
                &outcome,
                &objective.config,
                &weights,
                objective.target_num_passengers,
                objective.target_cargo_payload_kg,
            );
            Ok(cost::assemble(outcome, &objective.config, residuals))
        }
        Err(failure) => Err(failure.reason.to_owned()),
    }
}
