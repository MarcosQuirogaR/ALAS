// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Aircraft design space optimization: objective evaluation, envelope checks,
//! and differential evolution.
//!
//! [`objective::DesignObjective`] evaluates candidate design vectors against
//! multi-disciplinary physics (aerodynamic efficiency, longitudinal stability,
//! cabin sizing, structural geometry realism, and model-derived CG limits).
//!
//! [`envelope::assess_model_cg_envelope`] evaluates the hard stability floor
//! and each gear-reaction constraint across OEW, MZFW, and MTOW. The separate
//! [`envelope::check_cg_envelope`] path preserves the reference fixture.
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
pub mod history;
pub mod mdo;
#[path = "objective_model.rs"]
pub mod objective;
mod search;
mod search_methods;
pub mod transport_planform;

pub use cancellation::{
    watch_for, CancelEvent, CancelEventKind, CancelPhase, CancelScope, CancelSnapshot, CancelWatch,
    StopReason,
};
pub use differential_evolution::{
    BaselineComparison, PreGateReasons, ReportingBaseline, SizingWorkSummary, StageRejections,
    StageSummary, WorkDistribution, SEARCH_SCOPE,
};
pub use differential_evolution::{
    DeliveredAcceptance, DesignOptimizer, DiagnosticSearchOutcome, NoFeasibleDesign,
    OptimizationError, OptimizationResult, ParetoCandidate, RestorationDiagnostics,
    SearchDiagnostics, CANCELLED, REPORTING_FIDELITY_FALLBACK, REPORTING_FIDELITY_REJECTED,
};
pub use envelope::{
    assess_model_cg_envelope, assess_model_cg_envelope_with_ledger, check_cg_envelope,
    AftCgLimitGovernance, CgEnvelopeResult, LedgerLoadingBasis, ModelCgConstraint,
    ModelCgConstraintAssessment, ModelCgEnvelopeAssessment, ModelCgEnvelopeError,
    ModelCgLoadingAssessment, ModelCgLoadingState, StaticMarginPreferenceAssessment,
};
pub use evaluator::{ObjectiveEvaluation, ObjectiveEvaluator};
pub use history::OptimizationHistory;
pub use mdo::{
    assess_candidate, assess_candidate_with_polar_cancellable, assess_product_candidate,
    assess_product_candidate_cancellable, canonicalize_design, evaluate_mission_sized,
    evaluate_mission_sized_with_assessment, reporting_relative_balance, resolve_tail_sizing,
    CandidateAssessment, ConstraintFamily, ConstraintResidual, ExternalPolar,
    PolarConditionTolerance, ProductStateProvenance, ResolvedProductState, SegmentMissionModel,
    SizedCandidate,
};
pub use objective::{
    wing_fuel_volume_m3, wing_fuel_volume_m3_reference_compatibility, DesignObjective,
};
pub use search::fidelity_pairs::{compare_fidelities, CandidateScore, FidelityPair};
pub use search::screening::ScreeningFidelity;
pub use search::work_cap::{work_cap, WORK_CAP_MULTIPLE};
pub use search_methods::product_de::{
    planned_refinement_budget, verification_reserve, VerificationReserve, MAX_VERIFIED_CANDIDATES,
    MIN_PLANNED_EVALUATIONS, THROUGHPUT_SAFETY_FACTOR, VERIFICATION_RESERVE_EVALUATIONS,
    VERIFICATION_RESERVE_TIME_FRACTION,
};
pub use transport_planform::{assess_transport_planform, TransportPlanformAssessment};
