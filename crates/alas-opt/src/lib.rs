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

pub mod differential_evolution;
pub mod envelope;
pub mod evaluator;
pub mod history;
mod mesh_correction;
pub mod objective;
mod python_rng;
pub mod sampling;
mod search_methods;
pub mod transport_planform;

pub use differential_evolution::{DesignOptimizer, OptimizationResult, ParetoCandidate};
pub use envelope::{
    assess_model_cg_envelope, check_cg_envelope, CgEnvelopeResult, ModelCgConstraint,
    ModelCgConstraintAssessment, ModelCgEnvelopeAssessment, ModelCgEnvelopeError,
    ModelCgLoadingAssessment, ModelCgLoadingState, StaticMarginPreferenceAssessment,
};
pub use evaluator::{ObjectiveEvaluation, ObjectiveEvaluator};
pub use history::OptimizationHistory;
pub use objective::{wing_fuel_volume_m3, DesignObjective};
pub use sampling::{draw_one, error_issues, sample_design, widened_bounds, Rng};
pub use transport_planform::{
    assess_transport_planform, transport_planform_penalty, TransportPlanformAssessment,
};
