// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Candidate-level screening diagnostics with physical constraint margins.

use alas_opt::{CandidateScore, ConstraintResidual, FidelityPair};
use serde_json::{json, Value};

/// Assess the original reference with its fixed gear stations, independently
/// of the candidate ground-layout derivation used by the search sample.
pub(super) fn fixed_nominal_assessment(
    config: &alas_config::AlasConfig,
    pairs: &[FidelityPair],
) -> Value {
    let Some(pair) = pairs.first() else {
        return Value::Null;
    };
    let assessment = alas_config::DesignVector::from_array(&pair.design)
        .map_err(|error| error.to_string())
        .and_then(|design| {
            alas_opt::mdo::assess_product_candidate_with_controls(
                config,
                &design,
                alas_opt::mdo::SizingControls::default(),
            )
        });
    match assessment {
        Ok(assessment) => {
            let loading = assessment.sized.takeoff_loading.map(|loading| {
                json!({
                    "status": loading.status.as_str(),
                    "zero_fuel_mass_kg": loading.zero_fuel_mass_kg,
                    "mtow_fuel_budget_kg": loading.mtow_fuel_budget_kg,
                    "usable_capacity_kg": loading.usable_capacity_kg,
                    "carried_usable_fuel_kg": loading.carried_usable_fuel_kg,
                    "takeoff_mass_kg": loading.takeoff_mass_kg,
                    "mtow_margin_kg": loading.mtow_margin_kg,
                })
            });
            json!({
                "design": pair.design,
                "gear_placement": "fixed_reference",
                "takeoff_loading": loading,
                "full": {
                    "feasible": assessment.is_strictly_feasible(),
                    "analysed": true,
                    "cost": assessment.cost,
                    "objective_value": assessment.objective_value,
                    "takeoff_mass_kg": assessment.sized.takeoff_mass_kg,
                    "hard_violation": assessment.hard_violation_sum,
                    "failure_reason": Value::Null,
                    "residuals": assessment.residuals.iter().map(residual).collect::<Vec<_>>(),
                },
            })
        }
        Err(error) => json!({
            "design": pair.design,
            "gear_placement": "fixed_reference",
            "full": {"feasible": false, "analysed": false, "failure_reason": error},
        }),
    }
}

/// The first sampled point is the product search's materialized nominal,
/// including its allowed candidate ground-layout derivation.
pub(super) fn nominal_assessment(pairs: &[FidelityPair]) -> Value {
    pairs.first().map_or(
        Value::Null,
        |pair| json!({"design": pair.design, "full": score(&pair.full, true)}),
    )
}

/// Complete sampled vectors and both models' feasibility diagnostics.
pub(super) fn candidates(pairs: &[FidelityPair]) -> Value {
    json!(pairs
        .iter()
        .map(|pair| json!({
            "design": pair.design,
            "screening": score(&pair.screening, false),
            "full": score(&pair.full, false),
        }))
        .collect::<Vec<_>>())
}

fn score(score: &CandidateScore, all_residuals: bool) -> Value {
    json!({
        "feasible": score.feasible,
        "analysed": score.analysed,
        "cost": score.cost,
        "objective_value": score.objective_value,
        "takeoff_mass_kg": score.takeoff_mass_kg,
        "hard_violation": score.hard_violation,
        "failure_reason": score.failure_reason,
        "residuals": score.residuals.iter()
            .filter(|residual| all_residuals || residual.violated())
            .map(residual)
            .collect::<Vec<_>>(),
    })
}

fn residual(residual: &ConstraintResidual) -> Value {
    json!({
        "id": residual.id,
        "actual": residual.actual,
        "limit": residual.limit,
        "raw_residual": residual.raw_residual,
        "unit": residual.unit,
        "role": format!("{:?}", residual.role),
        "detail": residual.detail,
        "violated": residual.violated(),
        "normalized_violation": residual.normalized_violation,
    })
}
