// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Airfoil-database batch screening: 2-D surrogate scoring, 3-D trim re-simulation,
//! MSES verification, and multi-criteria ranking.
//!
//! [`runner::run_airfoil_screening`] orchestrates the pure-FLOPS product
//! screening process across:
//! - **Stage 1 (2-D)**: Fast NeuralFoil polar interpolation at design cruise CL.
//! - **Stage 2 (3-D)**: Real vortex-lattice stability & trim solve on the candidate wing.
//! - **Stage 3 (MSES)**: Coupled viscous/inviscid transonic shock & wave drag verification.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod refine;
pub mod runner;
pub mod score;
pub mod types;
pub mod verify_mses;

pub use refine::{
    refine_candidate_3d, refine_candidate_3d_reference_compatibility, ScreeningMassModel,
};
pub use runner::{
    blend_scores, filter_names, run_airfoil_screening, run_airfoil_screening_product,
    run_airfoil_screening_reference_compatibility,
};
pub use score::{cruise_condition, score_candidate};
pub use types::{
    AirfoilCandidateResult, AirfoilScreeningOptions, AirfoilScreeningResult, ScreeningFlowRegime,
    ScreeningObjective, CL_FEASIBILITY_TOL, MIN_NEURALFOIL_ANALYSIS_CONFIDENCE, REFERENCE_AIRFOILS,
    TRANSONIC_MACH_CAVEAT, TRIM_ALPHA_SLACK_DEG, TRIM_CM_RESIDUAL_TOL,
};
pub use verify_mses::verify_candidate_mses;
