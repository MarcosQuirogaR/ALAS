// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Assembling the residual table into the scalar cost the search minimises.
//!
//! The configured mission quantity is normalized by a fixed reference scale
//! rather than by a value observed during the run, because dividing by "the
//! first evaluated candidate" is not deterministic once worker threads
//! evaluate candidates in parallel. Masses are normalized by the takeoff-mass
//! ceiling, since every mission-sized candidate is bounded by it; block fuel
//! is normalized by three tenths of the ceiling, a generous upper bound on
//! trip fuel fraction for a long-range transport; fuel per seat-kilometre is
//! normalized by 1e-3 kg/(seat km), the order of magnitude of a modern
//! narrowbody's block fuel efficiency.

use alas_config::{AlasConfig, ConstraintPolicy, ObjectiveKind};

use super::sizing::SizingOutcome;
use super::types::{CandidateAssessment, ConstraintResidual};

/// Fraction of the takeoff-mass ceiling used to normalize a block-fuel
/// objective.
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
        ObjectiveKind::LegacyLiftToDrag => f64::NAN, // never reached: the legacy kind never delegates here
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
    residuals: Vec<ConstraintResidual>,
) -> CandidateAssessment {
    let objective_config = &config.optimizer.objective;
    let mtow_ceiling = outcome.mtow_ceiling;
    let kind = objective_config.kind;
    let range_km = outcome.sized.design_range_m / 1_000.0;
    let passengers = config.requirements.num_passengers;

    let hard_violation_sum: f64 = residuals
        .iter()
        .filter(|residual| residual.policy == ConstraintPolicy::Hard)
        .map(|residual| residual.normalized_violation)
        .sum();
    let soft_violation_sum: f64 = residuals
        .iter()
        .filter(|residual| residual.policy == ConstraintPolicy::Soft)
        .map(|residual| residual.normalized_violation)
        .sum();
    let hard_feasible = residuals.iter().all(|residual| {
        !(residual.policy == ConstraintPolicy::Hard && residual.normalized_violation > 0.0)
    });

    let objective_value = objective_value(kind, &outcome.sized, range_km, passengers);
    let normalization_scale = match kind {
        ObjectiveKind::LegacyLiftToDrag => 1.0,
        ObjectiveKind::BlockFuel => BLOCK_FUEL_NORMALIZATION_FRACTION * mtow_ceiling,
        ObjectiveKind::TakeoffMass | ObjectiveKind::OperatingEmptyMass => mtow_ceiling,
        ObjectiveKind::FuelPerSeatKilometre => FUEL_PER_SEAT_KM_NORMALIZATION,
    };
    let normalized_objective = objective_value / normalization_scale.max(1e-9);

    let mut cost = normalized_objective + objective_config.soft_penalty_weight * soft_violation_sum;
    if !hard_feasible {
        // An infeasible candidate always costs more than a feasible one, and
        // infeasible candidates order by how badly they violate their worst
        // hard constraint (differential_evolution_parts::part_01::scored_point
        // ranks feasibility first).
        cost += 1.0 + hard_violation_sum;
    }

    CandidateAssessment {
        sized: outcome.sized,
        residuals,
        hard_feasible,
        hard_violation_sum,
        soft_violation_sum,
        objective_value,
        cost,
    }
}
