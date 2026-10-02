// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mission-sized design objectives: block fuel, takeoff mass, operating
//! empty mass or fuel per seat-kilometre, closed by the design mission under
//! the configured fuel policy and ranked feasibility first.
//!
//! [`crate::objective::DesignObjective::evaluate`] delegates here for every
//! evaluation.
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
pub mod drag_table;
mod engine;
mod mda;
pub mod mission_model;
mod mtow_modes;
mod nominal_cache;
mod offdesign;
pub mod propulsion;
mod range;
mod residuals;
mod residuals_buffet;
mod residuals_geometry;
mod residuals_layout;
mod residuals_performance;
mod sizing;
pub mod structural_feasibility;
mod tail_sizing;
mod tanks;
mod trim;
mod types;

pub use mission_model::SegmentMissionModel;
pub use mtow_modes::MtowPlanOutcome;
pub use offdesign::OffDesignFlight;
pub use residuals::reporting_relative_balance;
pub(crate) use residuals_layout::TE_ANGLE_LIMIT_DEG;
pub use sizing::candidate_model::{
    baseline_closure_config, baseline_fuel_artifacts, candidate_mission_model,
};
pub use sizing::planned_mission::{plan_for_mission, solve_planned_dispatch, PlannedTrips};
pub use tanks::{usable_fuel_capacity, UsableCapacityBasis, UsableFuelCapacity};
pub use types::{
    CandidateAssessment, CandidateDrag, CandidateFuelArtifacts, ConstraintFamily,
    ConstraintResidual, DeckKey, ExternalPolar, PolarConditionTolerance, ProductStateProvenance,
    ResolvedProductState, SizedCandidate, SizingControls, SizingWork,
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

/// The design vector and tail sizing the evaluator builds a candidate with,
/// without evaluating it: the candidate payload load case, the cabin-derived
/// fuselage unless `preserve_explicit_fuselage_length`, and a reference
/// adaptation's tail sizing, through the very function the evaluator's
/// geometry build calls. An external aerodynamic model can then draw the
/// tail the coupled assessment will size, mass and trim.
///
/// # Errors
///
/// The candidate failure reason when those steps reject the vector.
pub fn resolve_tail_sizing(
    config: &AlasConfig,
    design: &DesignVector,
    preserve_explicit_fuselage_length: bool,
) -> Result<(DesignVector, alas_config::TailSizing), String> {
    let (_, resolved, sizing) = build::resolve_before_build(
        config,
        &design.to_array(),
        preserve_explicit_fuselage_length,
    )
    .map_err(|failure| failure.reason.to_owned())?;
    Ok((resolved, sizing))
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
    let outcome = sizing::run_candidate_cancellable(
        &objective.config,
        x,
        None,
        objective.preserve_explicit_fuselage_length,
        objective.cancellation.clone(),
        objective.sizing_controls,
    );
    match outcome {
        Ok(outcome) => {
            let history = outcome.history;
            let work = outcome.sized.work;
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
            objective.history.record_sizing_work(work);
            (assessment.cost, Some(assessment))
        }
        Err(failure) => {
            let cost = weights.failure_cost;
            // `CandidateFailure` carries only the reason, not the design
            // vector (see its doc comment); rebuild it from `x` for the
            // history entry, falling back to the default vector exactly as
            // the weighted-penalty path does when `x` itself does not parse.

            let dv = DesignVector::from_array(x).unwrap_or_default();
            // A candidate that never reached a sized takeoff mass has no
            // finite objective value or L/D; the kernels in
            // `differential_evolution::scored_point_at` already
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
    assess_with(objective, x, None, objective.sizing_controls)
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
    assess_product_candidate_cancellable(config, design, None)
}

/// Product finalist replay with cancellation inside the coupled mission solve.
pub fn assess_product_candidate_cancellable(
    config: &AlasConfig,
    design: &DesignVector,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> Result<CandidateAssessment, String> {
    if !config.mass_model.mass_architecture.is_production() {
        return Err(
            "the product candidate assessor requires pure_flops_transport_v1; select the explicit reference-compatibility comparison path for reference-compatible masses"
                .to_owned(),
        );
    }
    let mut objective = DesignObjective::new_with_nominal(config.clone(), *design);
    let token = crate::cancellation::EvaluationCancellation::new();
    objective.cancellation = Some(token.clone());
    crate::cancellation::forward_evaluation_cancellation(cancel, &token, || {
        assess_candidate(&objective, &design.to_array())
    })
}

/// [`assess_candidate`] with the cruise drag polar supplied by an external
/// aerodynamic solver instead of the native trim: the sizing loop then keeps
/// that polar fixed and closes mass, fuel and takeoff mass around it, with
/// deep coupled-solve cancellation.
///
/// # Errors
///
/// As [`assess_candidate`]; an invalid polar is reported as `trim_solve`.
pub fn assess_candidate_with_polar_cancellable(
    objective: &DesignObjective,
    x: &[f64],
    polar: &ExternalPolar,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> Result<CandidateAssessment, String> {
    let mut objective = objective.clone();
    let token = crate::cancellation::EvaluationCancellation::new();
    objective.cancellation = Some(token.clone());
    crate::cancellation::forward_evaluation_cancellation(cancel, &token, || {
        assess_with(&objective, x, Some(polar), SizingControls::default())
    })
}

/// [`assess_candidate`] under caller [`SizingControls`]: a work budget, a
/// warm-start takeoff mass and the starting integration step count.
///
/// # Errors
///
/// As [`assess_candidate`], plus `sizing_budget_exhausted`
/// ([`mission_model::SIZING_BUDGET_EXHAUSTED`]) when the budget is spent
/// before the closure ends.
pub fn assess_candidate_with_controls(
    objective: &DesignObjective,
    x: &[f64],
    controls: SizingControls,
) -> Result<CandidateAssessment, String> {
    assess_with(objective, x, None, controls)
}

/// [`assess_product_candidate`] under caller [`SizingControls`].
///
/// # Errors
///
/// As [`assess_candidate_with_controls`].
pub fn assess_product_candidate_with_controls(
    config: &AlasConfig,
    design: &DesignVector,
    controls: SizingControls,
) -> Result<CandidateAssessment, String> {
    if !config.mass_model.mass_architecture.is_production() {
        return Err(
            "the product candidate assessor requires pure_flops_transport_v1; select the explicit reference-compatibility comparison path for reference-compatible masses"
                .to_owned(),
        );
    }
    let objective = DesignObjective::new_with_nominal(config.clone(), *design);
    assess_with(&objective, &design.to_array(), None, controls)
}

fn assess_with(
    objective: &DesignObjective,
    x: &[f64],
    polar: Option<&ExternalPolar>,
    controls: SizingControls,
) -> Result<CandidateAssessment, String> {
    objective
        .validate_design_space(x)
        .map_err(|_| "design_space".to_owned())?;
    let weights = objective.config.optimizer.weights.clone();
    match sizing::run_candidate_cancellable(
        &objective.config,
        x,
        polar,
        objective.preserve_explicit_fuselage_length,
        objective.cancellation.clone(),
        controls,
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

/// Build the airplane of `dv` under `config` with the empennage scales
/// `sizing` written into both first (`TailSizing::apply_to`), the replay a
/// report or export of a sized candidate builds its geometry with so it draws
/// the tail the candidate was sized with.
///
/// # Errors
///
/// The geometry build failure, as a description.
pub fn rebuild_airplane(
    config: &mut AlasConfig,
    dv: &mut DesignVector,
    sizing: &alas_config::TailSizing,
) -> Result<alas_geom::aircraft::airplane::Airplane, String> {
    build::rebuild_airplane(config, dv, sizing)
        .map_err(|failure| format!("geometry build failed: {}", failure.reason))
}

#[cfg(test)]
mod cancellation_tests {
    #[test]
    fn pre_cancelled_product_finalist_reports_cancelled() {
        let config = alas_config::AlasConfig::default();
        let design = alas_config::DesignVector::default();
        let flag = std::sync::atomic::AtomicBool::new(true);
        assert!(
            matches!(super::assess_product_candidate_cancellable(&config, &design, Some(&flag)), Err(reason) if reason == "cancelled")
        );
    }
}

#[cfg(test)]
mod tail_sizing_tests {
    /// The pre-build resolution and the full assessment size the same tail
    /// for a moved reference wing, and that tail is not the vector's own.
    #[test]
    fn the_resolved_tail_sizing_is_the_one_the_full_assessment_resolves() {
        let config = alas_config::AlasConfig::from_value(&serde_json::json!({
            "preset": "A320-200",
            "optimizer": {"design_space": {"mode": "reference_adaptation"}}
        }))
        .expect("valid configuration");
        let mut moved = alas_config::presets::get("A320-200")
            .expect("registered preset")
            .design_vector;
        moved.span_m *= 0.95;
        for chord in [
            &mut moved.root_chord_m,
            &mut moved.break_chord_m,
            &mut moved.tip_chord_m,
        ] {
            *chord *= 1.07;
        }
        let preserve = crate::objective::DesignObjective::new_with_nominal(config.clone(), moved)
            .preserve_explicit_fuselage_length;
        let (design, sizing) =
            super::resolve_tail_sizing(&config, &moved, preserve).expect("resolvable");
        let assessed = super::assess_product_candidate(&config, &moved).expect("assessable");
        let reference = assessed.resolved.tail_sizing;
        assert_eq!(design, assessed.resolved.design);
        assert_eq!(sizing.tail_scale, reference.tail_scale);
        // The assessment re-measures the fin ratio on the built fin
        // (`tail_sizing::fin_scale_ratio`), a round trip through the
        // geometry, so the two agree to round-off rather than bit for bit.
        assert!((sizing.vstab_scale_ratio / reference.vstab_scale_ratio - 1.0).abs() < 1e-12);
        assert_ne!(sizing.tail_scale, moved.tail_scale);
        assert_ne!(sizing.vstab_scale_ratio, 1.0);
    }
}
