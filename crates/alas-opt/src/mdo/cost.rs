// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Assembling the residual table into the scalar cost the search minimises.
//!
//! The configured mission quantity is normalized by a fixed reference scale
//! rather than by a value observed during the run, because dividing by "the
//! first evaluated candidate" is not deterministic once worker threads
//! evaluate candidates in parallel. Masses are normalized by the plan's mass
//! scale (`alas_config::MtowPlan::normalisation_kg`: the declared MTOW, or
//! the band target in the MTOW band mode), called the ceiling below; block fuel
//! is normalized by three tenths of the ceiling, a generous upper bound on
//! trip fuel fraction for a long-range transport; fuel per seat-kilometre is
//! normalized by 1e-3 kg/(seat km), the order of magnitude of a modern
//! narrowbody's block fuel efficiency.

use crate::mdo::ResidualRole;

use alas_config::{AlasConfig, ObjectiveKind, TailSizing};

use super::sizing::SizingOutcome;
use super::types::{
    CandidateAssessment, ConstraintResidual, ProductStateProvenance, ResolvedProductState,
};

/// Fraction of the takeoff-mass ceiling used to normalize a block-fuel objective.
const BLOCK_FUEL_NORMALIZATION_FRACTION: f64 = 0.3;

/// Reference fuel-per-seat-kilometre scale, kg/(seat km).
const FUEL_PER_SEAT_KM_NORMALIZATION: f64 = 1.0e-3;

/// The configured mission quantity, before normalization.
fn objective_value(
    kind: ObjectiveKind,
    sized: &super::types::SizedCandidate,
    range_km: f64,
    passengers: i64,
) -> f64 {
    match kind {
        ObjectiveKind::BlockFuel => sized.block_fuel_kg,
        ObjectiveKind::TakeoffMass => sized.takeoff_mass_kg,
        ObjectiveKind::OperatingEmptyMass => sized.operating_empty_mass_kg,
        ObjectiveKind::FuelPerSeatKilometre => {
            sized.block_fuel_kg / (passengers.max(1) as f64 * range_km.max(1e-9))
        }
    }
}

/// Assemble the residual table and scalar cost for `outcome`.
pub(crate) fn assemble(
    outcome: SizingOutcome,
    config: &AlasConfig,
    mut residuals: Vec<ConstraintResidual>,
) -> CandidateAssessment {
    let objective_config = &config.optimizer.objective;
    // The mass scale of the plan: the declared MTOW, or the band target.
    let mtow_ceiling = outcome.plan.normalisation_kg;
    let kind = objective_config.kind;
    let range_km = outcome.sized.design_range_m / 1_000.0;
    // Efficiency is reported per seat actually carried by the detailed load
    // case. A shell that cannot seat the requested brief must not look better
    // merely because the denominator still uses the requested count.
    let passengers = outcome.sized.carried_passengers;

    let objective_value = objective_value(kind, &outcome.sized, range_km, passengers);
    // NaN comparisons are false and f64::max suppresses NaN. An unavailable
    // physical measurement must therefore have an explicit hard rejection,
    // including measurements used only for preferences or diagnostics.
    if !objective_value.is_finite()
        || objective_config.validate().is_err()
        || objective_value < 0.0
        || !mtow_ceiling.is_finite()
        || mtow_ceiling <= 0.0
        || residuals.iter().any(|residual| {
            ![
                residual.actual,
                residual.limit,
                residual.raw_residual,
                residual.normalized_violation,
            ]
            .iter()
            .all(|value| value.is_finite())
                || residual.normalized_violation < 0.0
        })
    {
        residuals.push(ConstraintResidual::direct(
            "candidate_state_unavailable",
            super::types::ConstraintFamily::Mass,
            1.0,
            0.0,
            "bool",
            1.0,
            1.0,
            ResidualRole::Constraint,
        ));
    }

    let hard_violation_sum: f64 = residuals
        .iter()
        .filter(|residual| residual.role == ResidualRole::Constraint)
        .map(|residual| residual.normalized_violation)
        .sum();
    let soft_violation_sum: f64 = residuals
        .iter()
        .filter(|residual| residual.role == ResidualRole::Preference)
        .map(|residual| residual.normalized_violation)
        .sum();
    let strictly_feasible = residuals.iter().all(|residual| {
        residual.role != ResidualRole::Constraint
            || (residual.normalized_violation.is_finite() && residual.normalized_violation <= 0.0)
    });
    let hard_feasible = strictly_feasible;

    let normalization_scale = match kind {
        ObjectiveKind::BlockFuel => BLOCK_FUEL_NORMALIZATION_FRACTION * mtow_ceiling,
        ObjectiveKind::TakeoffMass | ObjectiveKind::OperatingEmptyMass => mtow_ceiling,
        ObjectiveKind::FuelPerSeatKilometre => FUEL_PER_SEAT_KM_NORMALIZATION,
    };
    let normalized_objective = objective_value / normalization_scale.max(1e-9);

    let mut cost = normalized_objective + objective_config.preference_weight * soft_violation_sum;

    if !hard_feasible {
        // This finite cost surcharge is not the feasibility guarantee:
        // scored-point selection ranks feasible candidates first. Infeasible
        // candidates carry the SUM of normalized hard violations, not their
        // worst individual constraint; the early epsilon comparison can also
        // use cost when comparing two infeasible candidates.
        cost += 1.0 + hard_violation_sum;
    }

    // Captured before `outcome.sized` is moved: this is the same state
    // `mdo::residuals::balance_residuals` just evaluated the hard CG and
    // gear constraints on, so a report bound to this candidate can quote it
    // instead of rebuilding a second, independently placed aircraft.
    let resolved = ResolvedProductState {
        // The vector the evaluator built on, after any cabin-derived
        // coordinate solve replaced the caller's literal. See
        // `ResolvedProductState::design`.
        design: outcome.history.dv,
        tail_sizing: TailSizing {
            tail_scale: outcome.history.dv.tail_scale,
            vstab_scale_ratio: super::tail_sizing::fin_scale_ratio(
                &outcome.plane,
                &config.geometry.empennage,
                &outcome.history.dv,
            ),
        },
        main_gear_placement: config.landing_gear.derived_main_gear,
        masses: outcome.masses,
        coords: outcome.coords,
        cg_x_m: outcome.cg_x,
        x_neutral_point_m: outcome.x_np,
        mac_m: outcome.mac,
        takeoff_mass_kg: outcome.sized.takeoff_mass_kg,
        provenance: ProductStateProvenance::MissionSizedClosure,
    };

    CandidateAssessment {
        sized: outcome.sized,
        resolved,
        residuals,
        hard_feasible,
        hard_violation_sum,
        soft_violation_sum,
        objective_value,
        cost,
    }
}
