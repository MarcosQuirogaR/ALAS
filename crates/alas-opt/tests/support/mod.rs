// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What the optimizer integration tests share: a preset's registered design,
//! the clean-sheet configuration of an MTOW sizing mode, and the violated
//! hard residual test.

// Each test binary compiles its own copy of this module and uses part of it.
#![allow(dead_code)]
// A test binary's failed expect is the assertion failing.
#![allow(clippy::expect_used)]

use alas_config::design_variables::DesignVector;
use alas_config::{AlasConfig, MtowSizing};
use alas_opt::CandidateAssessment;

/// The registered design vector of `preset`.
pub fn nominal(preset: &str) -> DesignVector {
    alas_config::presets::get(preset)
        .expect("registered preset")
        .design_vector
}

/// The default clean-sheet aircraft sized in `sizing`, with the transport
/// planform preferences disabled.
pub fn clean_sheet(sizing: MtowSizing) -> AlasConfig {
    let mut config = AlasConfig::default();
    let optimizer = &mut config.optimizer;
    optimizer.weights.transport_planform_constraints_enabled = false;
    optimizer.objective.mtow_sizing = sizing;
    config
}

/// Whether `id` appears among the assessment's violated hard residuals.
pub fn violates(assessment: &CandidateAssessment, id: &str) -> bool {
    assessment
        .violated_hard_ids()
        .into_iter()
        .any(|name| name == id)
}
