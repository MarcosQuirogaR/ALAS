// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Turning a sized candidate into the typed residual table.
//!
//! Each family is independent: a family whose policy is
//! [`ConstraintPolicy::Off`] contributes no residuals at all, and every
//! other family is evaluated the same way regardless of the others'
//! policies. The mass and balance families are evaluated here; the
//! performance and geometry families are large enough on their own that they
//! live in `mdo::residuals_performance` and `mdo::residuals_geometry`.

use alas_config::{AlasConfig, ConstraintPolicy, ObjectiveConfig, ObjectiveWeights};

use crate::envelope::{
    assess_model_cg_envelope, ModelCgConstraint, ModelCgConstraintAssessment,
    ModelCgLoadingAssessment,
};

use super::residuals_geometry::geometry_residuals;
use super::residuals_performance::performance_residuals;
use super::sizing::SizingOutcome;
use super::types::ConstraintFamily::{Balance, Mass};
use super::types::ConstraintResidual;

/// Every requirement family's residuals for one sized candidate.
pub(crate) fn build(
    outcome: &SizingOutcome,
    config: &AlasConfig,
    weights: &ObjectiveWeights,
    target_num_passengers: i64,
    target_cargo_payload_kg: f64,
) -> Vec<ConstraintResidual> {
    let objective = &config.optimizer.objective;
    let mut residuals = Vec::new();
    residuals.extend(mass_residuals(outcome, objective, config));
    residuals.extend(balance_residuals(
        outcome,
        config,
        objective.balance_constraints,
    ));
    residuals.extend(performance_residuals(
        outcome,
        config,
        objective.performance_constraints,
    ));
    residuals.extend(geometry_residuals(
        outcome,
        config,
        weights,
        objective.geometry_constraints,
        target_num_passengers,
        target_cargo_payload_kg,
    ));
    residuals
}

/// The fuel-capacity, takeoff-mass-ceiling, landing-mass and sizing-closure
/// residuals.
fn mass_residuals(
    outcome: &SizingOutcome,
    objective: &ObjectiveConfig,
    config: &AlasConfig,
) -> Vec<ConstraintResidual> {
    let policy = objective.mass_constraints;
    if policy == ConstraintPolicy::Off {
        return Vec::new();
    }
    let sized = &outcome.sized;
    let mut residuals = Vec::new();

    // The clean-sheet reconciliation currently has an explicit primary box
    // and Torenbeek high-lift/spoiler inventory, while joints, actuators,
    // fairings and other non-box items are not represented by a sourced
    // complete inventory. Keep that limitation binding so a partial wing
    // model cannot become the accepted finalist through a finite penalty.
    if !outcome.structural_inventory_complete {
        residuals.push(ConstraintResidual::direct(
            "structural_inventory_unverified",
            Mass,
            1.0,
            0.0,
            "bool",
            1.0,
            1.0,
            policy,
        ));
    }

    if sized.usable_capacity_kg.is_finite() {
        residuals.push(ConstraintResidual::scaled(
            "fuel_capacity",
            Mass,
            sized.ramp_fuel_kg,
            sized.usable_capacity_kg,
            "kg",
            sized.ramp_fuel_kg - sized.usable_capacity_kg,
            policy,
        ));
    } else {
        residuals.push(ConstraintResidual::direct(
            "fuel_capacity_unavailable",
            Mass,
            1.0,
            0.0,
            "bool",
            1.0,
            1.0,
            policy,
        ));
    }

    let required_takeoff_mass_kg =
        sized.dispatch.zero_fuel_mass_kg + sized.dispatch.plan.takeoff_fuel_kg();
    residuals.push(ConstraintResidual::scaled(
        "mtow_ceiling",
        Mass,
        required_takeoff_mass_kg,
        outcome.mtow_ceiling,
        "kg",
        required_takeoff_mass_kg - outcome.mtow_ceiling,
        policy,
    ));

    let mlw_kg = config.landing_mass_limit_kg(outcome.mtow_ceiling);
    residuals.push(ConstraintResidual::scaled(
        "landing_mass",
        Mass,
        sized.dispatch.destination_landing_mass_kg,
        mlw_kg,
        "kg",
        sized.dispatch.destination_landing_mass_kg - mlw_kg,
        policy,
    ));

    match &sized.dispatch.status {
        alas_mass::dispatch::DispatchStatus::Converged => {}
        alas_mass::dispatch::DispatchStatus::MtowLimited { shortfall_kg } => {
            residuals.push(ConstraintResidual::scaled(
                "dispatch_mtow_limited",
                Mass,
                sized.takeoff_mass_kg + shortfall_kg,
                outcome.mtow_ceiling,
                "kg",
                *shortfall_kg,
                policy,
            ));
        }
        alas_mass::dispatch::DispatchStatus::TankLimited { shortfall_kg } => {
            let capacity = sized.usable_capacity_kg;
            residuals.push(if capacity.is_finite() {
                ConstraintResidual::scaled(
                    "dispatch_tank_limited",
                    Mass,
                    sized.ramp_fuel_kg,
                    capacity,
                    "kg",
                    *shortfall_kg,
                    policy,
                )
            } else {
                ConstraintResidual::direct(
                    "dispatch_tank_limited",
                    Mass,
                    1.0,
                    0.0,
                    "bool",
                    1.0,
                    1.0,
                    policy,
                )
            });
        }
        alas_mass::dispatch::DispatchStatus::NotConverged { last_change_kg } => {
            residuals.push(ConstraintResidual::scaled(
                "dispatch_not_converged",
                Mass,
                *last_change_kg,
                objective.sizing_tolerance_kg,
                "kg",
                last_change_kg.abs(),
                policy,
            ));
        }
        alas_mass::dispatch::DispatchStatus::ModelFailed(_) => {
            residuals.push(ConstraintResidual::direct(
                "dispatch_model_failed",
                Mass,
                1.0,
                0.0,
                "bool",
                1.0,
                1.0,
                policy,
            ));
        }
    }

    let not_closed = if sized.sizing_closed { 0.0 } else { 1.0 };
    residuals.push(ConstraintResidual::direct(
        "sizing_not_closed",
        Mass,
        not_closed,
        0.0,
        "bool",
        not_closed,
        not_closed,
        policy,
    ));

    residuals
}

/// The five hard model constraints and the lower/upper direction each is
/// evaluated in, matching `crate::envelope::assess_loading_constraints`
/// (`static_stability_floor` and `configured_forward_cg_range` are lower
/// bounds; the gear-strength pair is an upper bound; the minimum nose-gear
/// load is a lower bound).
const BALANCE_CONSTRAINTS: [(&str, ModelCgConstraint, bool); 5] = [
    (
        "static_margin_floor",
        ModelCgConstraint::StaticStabilityFloor,
        true,
    ),
    (
        "forward_cg_range",
        ModelCgConstraint::ConfiguredForwardCgRange,
        true,
    ),
    (
        "nose_gear_strength",
        ModelCgConstraint::NoseGearStrength,
        false,
    ),
    (
        "main_gear_strength",
        ModelCgConstraint::MainGearStrength,
        false,
    ),
    (
        "min_nose_gear_load",
        ModelCgConstraint::MinimumNoseGearLoad,
        true,
    ),
];

/// The centre-of-gravity envelope and landing-gear reaction residuals.
fn balance_residuals(
    outcome: &SizingOutcome,
    config: &AlasConfig,
    policy: ConstraintPolicy,
) -> Vec<ConstraintResidual> {
    if policy == ConstraintPolicy::Off {
        return Vec::new();
    }
    let assessment = assess_model_cg_envelope(
        &outcome.plane,
        &outcome.masses,
        &outcome.coords,
        outcome.cg_x,
        outcome.x_np,
        outcome.mac,
        config,
    );
    let Ok(assessment) = assessment else {
        return vec![ConstraintResidual::direct(
            "cg_model_error",
            Balance,
            1.0,
            0.0,
            "bool",
            1.0,
            1.0,
            policy,
        )];
    };
    BALANCE_CONSTRAINTS
        .iter()
        .filter_map(|&(id, constraint, is_lower_bound)| {
            worst_by_constraint(&assessment.loading_states, constraint).map(|worst| {
                let raw_residual = if is_lower_bound {
                    worst.limit - worst.actual
                } else {
                    worst.actual - worst.limit
                };
                ConstraintResidual::direct(
                    id,
                    Balance,
                    worst.actual,
                    worst.limit,
                    constraint.unit(),
                    raw_residual,
                    worst.normalized_exceedance,
                    policy,
                )
            })
        })
        .collect()
}

/// The per-constraint assessment with the largest normalized exceedance
/// across every loading state, matching
/// `ModelCgEnvelopeAssessment::worst_hard_exceedance`'s own convention but
/// keeping the assessment (actual, limit) that produced it rather than only
/// the number.
fn worst_by_constraint(
    loading_states: &[ModelCgLoadingAssessment],
    constraint: ModelCgConstraint,
) -> Option<ModelCgConstraintAssessment> {
    loading_states
        .iter()
        .flat_map(|state| state.constraints.iter())
        .filter(|candidate| candidate.constraint == constraint)
        .copied()
        .max_by(|a, b| a.normalized_exceedance.total_cmp(&b.normalized_exceedance))
}
