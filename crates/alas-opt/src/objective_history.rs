// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Final history bookkeeping for scalar objective evaluations.

use alas_config::{design_variables::DesignVector, ObjectiveWeights};

use crate::history::OptimizationHistory;

/// Record a completed objective evaluation and derive its physical-diagnostic labels.
///
/// Keeping this bookkeeping outside the numerical evaluator keeps the latter
/// below the repository's production-source size limit without changing the
/// order or contents of the history vectors.
#[allow(clippy::too_many_arguments)]
pub(crate) fn record_objective_result(
    history: &mut OptimizationHistory,
    dv: DesignVector,
    cost: f64,
    ld: f64,
    span_m: f64,
    alpha_deg: f64,
    area_m2: f64,
    trim_ih_deg: f64,
    sm_floor_violation: bool,
    cg_envelope_violation: bool,
    shortfall_pct: f64,
    wing_area_limit_violation: bool,
    wing_loading_violation: bool,
    transport_constraints_active: bool,
    geometric_body_alpha_deg: f64,
    weights: &ObjectiveWeights,
) -> f64 {
    let mut reasons = Vec::new();
    if sm_floor_violation {
        reasons.push("static_margin");
    }
    if cg_envelope_violation {
        reasons.push("cg_envelope");
    }
    if shortfall_pct > 0.0 {
        reasons.push("payload_shortfall");
    }
    if wing_area_limit_violation {
        reasons.push("wing_area_limit");
    }
    if wing_loading_violation {
        reasons.push("wing_loading");
    }
    if transport_constraints_active
        && (geometric_body_alpha_deg < weights.geometric_body_alpha_min_deg
            || geometric_body_alpha_deg > weights.geometric_body_alpha_max_deg)
    {
        reasons.push("body_alpha_window");
    }

    let is_valid = reasons.is_empty();
    history.record(
        dv,
        is_valid,
        cost,
        ld,
        span_m,
        alpha_deg,
        area_m2,
        trim_ih_deg,
        reasons.join("+"),
    );
    cost
}
