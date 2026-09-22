// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Aircraft design space optimization: objective evaluation, envelope checks,
//! sampling, and differential evolution.
//!
//! [`objective::DesignObjective`] evaluates candidate design vectors against
//! multi-disciplinary physics (aerodynamic efficiency, longitudinal stability,
//! cabin sizing, structural geometry realism, and model-derived CG limits).
//!
//! [`envelope::assess_model_cg_envelope`] evaluates the hard stability floor
//! and each gear-reaction constraint across OEW, MZFW, and MTOW. The separate
//! [`envelope::check_cg_envelope`] path preserves the frozen Python fixture.
//!
//! [`sampling::sample_design`] generates random valid design vectors.
//!
//! [`differential_evolution::DesignOptimizer`] searches the design space using
//! a global evolutionary strategy.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod cancellation;
pub mod differential_evolution;
pub mod envelope;
pub mod evaluator;
pub mod gradient;
pub mod history;
pub mod mdo;
mod mesh_correction;
pub mod objective;
mod python_rng;
pub mod sampling;
mod search;
mod search_methods;
pub mod transport_planform;

pub use cancellation::{
    watch_for, CancelEvent, CancelEventKind, CancelPhase, CancelScope, CancelSnapshot, CancelWatch,
    StopReason,
};
pub use differential_evolution::{
    DeliveredAcceptance, DesignOptimizer, NoFeasibleDesign, OptimizationError, OptimizationResult,
    ParetoCandidate, SearchDiagnostics, CANCELLED, REPORTING_FIDELITY_FALLBACK,
    REPORTING_FIDELITY_REJECTED,
};
pub use envelope::{
    assess_model_cg_envelope, check_cg_envelope, AftCgLimitGovernance, CgEnvelopeResult,
    ModelCgConstraint, ModelCgConstraintAssessment, ModelCgEnvelopeAssessment,
    ModelCgEnvelopeError, ModelCgLoadingAssessment, ModelCgLoadingState,
    StaticMarginPreferenceAssessment,
};
pub use evaluator::{ObjectiveEvaluation, ObjectiveEvaluator};
pub use gradient::{
    run_sqp, solve_qp, ConstrainedEvaluator, ConstrainedPoint, QpSolution, SqpOutcome, SqpSettings,
};
pub use history::OptimizationHistory;
pub use mdo::{
    assess_candidate, assess_candidate_with_polar, assess_product_candidate, canonicalize_design,
    evaluate_mission_sized, evaluate_mission_sized_with_assessment, CandidateAssessment,
    ConstraintFamily, ConstraintResidual, ExternalPolar, PolarConditionTolerance,
    ProductStateProvenance, ResolvedProductState, SegmentMissionModel, SizedCandidate,
};
pub use objective::{
    wing_fuel_volume_m3, wing_fuel_volume_m3_reference_compatibility, DesignObjective,
};
pub use sampling::{draw_one, error_issues, sample_design, widened_bounds, Rng};
pub use transport_planform::{
    assess_transport_planform, transport_planform_penalty, TransportPlanformAssessment,
};
