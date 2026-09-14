// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mission-sized design objectives: block fuel, takeoff mass, operating
//! empty mass or fuel per seat-kilometre, closed by the design mission under
//! the configured fuel policy and ranked feasibility first.
//!
//! [`crate::objective::DesignObjective::evaluate`] delegates here for every
//! product evaluation; only the frozen reference-compatibility replay keeps
//! the legacy weighted penalty so its parity fixture stays meaningful.
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
mod mda;
pub mod mission_model;
pub mod propulsion;
mod range;
mod residuals;
mod residuals_geometry;
mod residuals_performance;
mod sizing;
mod tanks;
mod trim;
mod types;

pub use mission_model::SegmentMissionModel;
pub use types::{
    CandidateAssessment, ConstraintFamily, ConstraintResidual, ExternalPolar,
    PolarConditionTolerance, ProductStateProvenance, ResolvedProductState, SizedCandidate,
};

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;

use crate::objective::DesignObjective;

/// Resolve a deterministic nominal vector for a product design space.
///
/// Clean-sheet passenger studies with cabin-derived fuselage length need the
/// same one-dimensional sizing solve before bounds are handed to the search;
/// otherwise the search would evaluate one body length and return another in
/// its best-design vector. Other modes return the supplied nominal intact.
pub(crate) fn canonical_nominal_design(
    config: &AlasConfig,
    nominal: DesignVector,
) -> Result<DesignVector, String> {
    canonicalize_design(config, nominal)
}

/// Materialize the design vector that the production evaluator actually
/// builds for a candidate.
///
/// In a clean-sheet passenger study the fuselage length is derived from the
/// cabin load case.  Keeping this operation at the public pipeline boundary
/// prevents a no-optimization run (or a downstream export) from publishing
/// the unsized default vector while the evaluator silently builds a different
/// fuselage.  Other design modes preserve the caller's vector verbatim.
pub fn canonicalize_design(
    config: &AlasConfig,
    mut design: DesignVector,
) -> Result<DesignVector, String> {
    if !config.optimizer.design_space.sizes_fuselage_from_cabin()
        || config.requirements.aircraft_type == "cargo"
    {
        return Ok(design);
    }
    let mut materialized = config.clone();
    crate::objective::apply_candidate_payload_load_case(&mut materialized, &design)
        .map_err(|error| format!("cabin load case cannot be materialized: {error}"))?;
    build::size_fuselage_from_cabin(&materialized, &mut design)
        .map_err(|failure| format!("fuselage cannot be sized from cabin: {}", failure.reason))?;
    Ok(design)
}

/// Evaluate a mission-sized objective for candidate design vector `x`,
/// recording the result into `objective`'s history and returning the scalar
/// cost.
///
/// Called from [`crate::objective::DesignObjective::evaluate`]; not meant to
/// be called directly on a kind that is not mission-sized.
pub fn evaluate_mission_sized(objective: &mut DesignObjective, x: &[f64]) -> f64 {
    evaluate_mission_sized_with_assessment(objective, x).0
}

/// [`evaluate_mission_sized`], also returning the residual table the cost
/// was assembled from, for a driver that reads constraints individually.
pub fn evaluate_mission_sized_with_assessment(
    objective: &mut DesignObjective,
    x: &[f64],
) -> (f64, Option<CandidateAssessment>) {
    let weights = objective.config.optimizer.weights.clone();
    if objective.validate_design_space(x).is_err() {
        let cost = weights.failure_cost;
        let dv = DesignVector::from_array(x).unwrap_or_default();
        objective.history.record_mission_sized(
            dv,
            false,
            cost,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            "design_space".to_owned(),
            f64::NAN,
            f64::NAN,
            f64::NAN,
            f64::NAN,
            f64::NAN,
        );
        return (cost, None);
    }
    match sizing::run_candidate_with_fuselage_policy(
        &objective.config,
        x,
        objective.preserve_explicit_fuselage_length,
    ) {
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
            (assessment.cost, Some(assessment))
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
            (cost, None)
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
    assess_with(objective, x, None)
}

/// Re-evaluate a product finalist with the same mission-sized objective that
/// the optimizer uses, using the finalist itself as the nominal design-space
/// reference.
///
/// The optimizer returns a design vector and its history, while the pipeline
/// still needs the closed takeoff mass to build the final report.  Replaying
/// the typed assessment at this boundary keeps that mass, dispatch status and
/// residual table tied to the exact vector that is exported.  Using the
/// finalist as the nominal only avoids rejecting a valid caller-supplied
/// starting vector because it lies outside a different preset's preferred
/// envelope; the configured global design-space validity rules remain active.
pub fn assess_product_candidate(
    config: &AlasConfig,
    design: &DesignVector,
) -> Result<CandidateAssessment, String> {
    if !config.mass_model.mass_architecture.is_production() {
        return Err(
            "the product candidate assessor requires pure_flops_transport_v1; select the explicit reference-compatibility comparison path for legacy masses"
                .to_owned(),
        );
    }
    let objective = DesignObjective::new_with_nominal(config.clone(), *design);
    assess_candidate(&objective, &design.to_array())
}

/// [`assess_candidate`] with the cruise drag polar supplied by an external
/// aerodynamic solver instead of the native trim: the sizing loop then keeps
/// that polar fixed and closes mass, fuel and takeoff mass around it.
///
/// # Errors
///
/// As [`assess_candidate`]; an invalid polar is reported as `trim_solve`.
pub fn assess_candidate_with_polar(
    objective: &DesignObjective,
    x: &[f64],
    polar: &ExternalPolar,
) -> Result<CandidateAssessment, String> {
    assess_with(objective, x, Some(polar))
}

fn assess_with(
    objective: &DesignObjective,
    x: &[f64],
    polar: Option<&ExternalPolar>,
) -> Result<CandidateAssessment, String> {
    objective
        .validate_design_space(x)
        .map_err(|_| "design_space".to_owned())?;
    let weights = objective.config.optimizer.weights.clone();
    match sizing::run_candidate_with_polar_and_fuselage_policy(
        &objective.config,
        x,
        polar,
        objective.preserve_explicit_fuselage_length,
    ) {
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
