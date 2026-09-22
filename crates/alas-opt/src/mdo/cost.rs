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

use std::collections::BTreeSet;

use alas_config::{AlasConfig, ConstraintPolicy, ObjectiveKind};

use super::sizing::SizingOutcome;
use super::types::{
    CandidateAssessment, ConstraintResidual, ProductStateProvenance, RelaxationOutcome,
    ResolvedProductState,
};

/// Split the violated hard residuals into those the relaxation policy admits
/// and those that still reject the candidate (clarified ledger D01-D02).
///
/// A violation is admitted only when the policy lists its limit, the miss is
/// inside that limit's own declared tolerance, and the number of distinct
/// discipline groups carrying an admitted miss stays inside the allowed
/// count. Anything else rejects, exactly as it did with the policy off. With
/// the shipped strict policy this returns an empty outcome without inspecting
/// a single residual, so nothing about the default run changes.
///
/// Called only for a candidate that is *not* strictly feasible, so a strict
/// policy rejects it outright.
fn apply_relaxation(config: &AlasConfig, residuals: &[ConstraintResidual]) -> RelaxationOutcome {
    let policy = &config.optimizer.relaxation;
    if !policy.is_active() {
        return RelaxationOutcome {
            relaxed_ids: Vec::new(),
            violated_groups: 0,
            rejected: true,
        };
    }
    let mut relaxed_ids = Vec::new();
    let mut groups = BTreeSet::new();
    let mut rejected = false;
    for residual in residuals {
        if residual.policy != ConstraintPolicy::Hard || residual.normalized_violation <= 0.0 {
            continue;
        }
        match policy.tolerance_for(residual.id) {
            Some(tolerance) if residual.normalized_violation <= tolerance => {
                relaxed_ids.push(residual.id);
                groups.insert(residual.family);
            }
            _ => rejected = true,
        }
    }
    let allowed_groups = usize::try_from(policy.allowed_violated_groups).unwrap_or(0);
    if groups.len() > allowed_groups {
        // Too many disciplines are being missed at once. The candidate is
        // rejected as a whole; the misses stay reported so a reader can see
        // which groups they were.
        rejected = true;
    }
    RelaxationOutcome {
        relaxed_ids,
        violated_groups: groups.len(),
        rejected,
    }
}

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
    // Efficiency is reported per seat actually carried by the detailed load
    // case. A shell that cannot seat the requested brief must not look better
    // merely because the denominator still uses the requested count.
    let passengers = outcome.sized.carried_passengers;

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
    let strictly_feasible = residuals.iter().all(|residual| {
        !(residual.policy == ConstraintPolicy::Hard && residual.normalized_violation > 0.0)
    });
    // A candidate the relaxation policy admits is *admissible*, not feasible:
    // it is ranked, reported and exported as relaxed, and `relaxation`
    // carries which limits and how many groups. With the shipped strict
    // policy `relaxation` is empty and this is exactly `strictly_feasible`.
    let relaxation = if strictly_feasible {
        RelaxationOutcome::strict()
    } else {
        apply_relaxation(config, &residuals)
    };
    let hard_feasible = strictly_feasible || !relaxation.rejected;

    let objective_value = objective_value(kind, &outcome.sized, range_km, passengers);
    let normalization_scale = match kind {
        ObjectiveKind::BlockFuel => BLOCK_FUEL_NORMALIZATION_FRACTION * mtow_ceiling,
        ObjectiveKind::TakeoffMass | ObjectiveKind::OperatingEmptyMass => mtow_ceiling,
        ObjectiveKind::FuelPerSeatKilometre => FUEL_PER_SEAT_KM_NORMALIZATION,
    };
    let normalized_objective = objective_value / normalization_scale.max(1e-9);

    let mut cost = normalized_objective + objective_config.soft_penalty_weight * soft_violation_sum;
    if hard_feasible && !relaxation.relaxed_ids.is_empty() {
        // D03: a fully feasible design ranks ahead of a relaxed one whatever
        // their objectives. The ranking key the search uses is
        // (admissible, aggregate hard violation, cost), and a strictly
        // feasible candidate's aggregate violation is zero while a relaxed
        // one's is not, so the ordering is already lexicographic. The cost
        // term here only keeps the relaxed candidate behind its own feasible
        // neighbours for a caller that compares costs alone.
        cost += objective_config.soft_penalty_weight * hard_violation_sum;
    }
    if !hard_feasible {
        // An infeasible candidate always costs more than a feasible one, and
        // infeasible candidates order by how badly they violate their worst
        // hard constraint (differential_evolution_parts::part_01::scored_point
        // ranks feasibility first).
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
        relaxation,
        hard_violation_sum,
        soft_violation_sum,
        objective_value,
        cost,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::optimizer::relaxation::{ConstraintRelaxation, RelaxableLimit};
    use alas_config::ConstraintPolicy;

    use super::super::types::ConstraintFamily;

    /// A hard residual violated by `normalized_violation` of its own limit.
    fn violated(id: &'static str, family: ConstraintFamily, violation: f64) -> ConstraintResidual {
        ConstraintResidual::direct(
            id,
            family,
            1.0 + violation,
            1.0,
            "-",
            violation,
            violation,
            ConstraintPolicy::Hard,
        )
    }

    fn eligible(id: &str, tolerance_fraction: f64) -> RelaxableLimit {
        RelaxableLimit {
            id: id.to_owned(),
            tolerance_fraction,
            provenance: "test fixture: not an engineering-reviewed tolerance".to_owned(),
        }
    }

    fn config_with(policy: ConstraintRelaxation) -> AlasConfig {
        let mut config = AlasConfig::default();
        config.optimizer.relaxation = policy;
        config
    }

    #[test]
    fn the_strict_shipped_policy_rejects_every_violation() {
        let config = AlasConfig::default();
        let outcome = apply_relaxation(
            &config,
            &[violated("wing_area", ConstraintFamily::Geometry, 0.01)],
        );
        assert!(outcome.rejected);
        assert!(outcome.relaxed_ids.is_empty());
        assert!(!outcome.is_relaxed());
    }

    #[test]
    fn a_reviewed_limit_inside_its_tolerance_is_relaxed_rather_than_rejected() {
        let config = config_with(ConstraintRelaxation {
            enabled: true,
            allowed_violated_groups: 1,
            eligible: vec![eligible("wing_area", 0.05)],
        });
        let outcome = apply_relaxation(
            &config,
            &[violated("wing_area", ConstraintFamily::Geometry, 0.01)],
        );
        assert!(!outcome.rejected);
        assert_eq!(outcome.relaxed_ids, vec!["wing_area"]);
        assert_eq!(outcome.violated_groups, 1);
        assert!(outcome.is_relaxed());
    }

    #[test]
    fn the_same_limit_outside_its_tolerance_is_rejected() {
        let config = config_with(ConstraintRelaxation {
            enabled: true,
            allowed_violated_groups: 1,
            eligible: vec![eligible("wing_area", 0.005)],
        });
        let outcome = apply_relaxation(
            &config,
            &[violated("wing_area", ConstraintFamily::Geometry, 0.01)],
        );
        assert!(outcome.rejected);
    }

    #[test]
    fn several_eligible_misses_inside_one_discipline_count_as_one_group() {
        // D01 in its own words: several eligible exceeded limits within Mass
        // count as one violated group.
        let config = config_with(ConstraintRelaxation {
            enabled: true,
            allowed_violated_groups: 1,
            eligible: vec![
                eligible("fuel_capacity", 0.05),
                eligible("landing_mass", 0.05),
            ],
        });
        let outcome = apply_relaxation(
            &config,
            &[
                violated("fuel_capacity", ConstraintFamily::Mass, 0.01),
                violated("landing_mass", ConstraintFamily::Mass, 0.02),
            ],
        );
        assert!(!outcome.rejected);
        assert_eq!(outcome.violated_groups, 1);
        assert_eq!(outcome.relaxed_ids.len(), 2);
    }

    #[test]
    fn more_violated_groups_than_allowed_rejects_the_whole_candidate() {
        let config = config_with(ConstraintRelaxation {
            enabled: true,
            allowed_violated_groups: 1,
            eligible: vec![eligible("fuel_capacity", 0.05), eligible("wing_area", 0.05)],
        });
        let outcome = apply_relaxation(
            &config,
            &[
                violated("fuel_capacity", ConstraintFamily::Mass, 0.01),
                violated("wing_area", ConstraintFamily::Geometry, 0.01),
            ],
        );
        assert!(outcome.rejected);
        assert_eq!(outcome.violated_groups, 2);
    }

    #[test]
    fn one_ineligible_miss_rejects_even_when_every_other_miss_is_admitted() {
        let config = config_with(ConstraintRelaxation {
            enabled: true,
            allowed_violated_groups: 4,
            eligible: vec![eligible("wing_area", 0.05)],
        });
        let outcome = apply_relaxation(
            &config,
            &[
                violated("wing_area", ConstraintFamily::Geometry, 0.01),
                violated("static_margin_floor", ConstraintFamily::Balance, 0.01),
            ],
        );
        assert!(outcome.rejected);
    }

    #[test]
    fn a_failed_evaluation_is_never_relaxed_even_when_a_document_lists_it() {
        let config = config_with(ConstraintRelaxation {
            enabled: true,
            allowed_violated_groups: 4,
            eligible: vec![eligible("sizing_not_closed", 0.05)],
        });
        let outcome = apply_relaxation(
            &config,
            &[violated("sizing_not_closed", ConstraintFamily::Mass, 0.01)],
        );
        assert!(outcome.rejected);
        assert!(outcome.relaxed_ids.is_empty());
    }

    #[test]
    fn a_soft_residual_is_not_counted_as_a_relaxed_hard_miss() {
        let config = config_with(ConstraintRelaxation {
            enabled: true,
            allowed_violated_groups: 1,
            eligible: vec![eligible("wing_area", 0.05)],
        });
        let soft = ConstraintResidual::direct(
            "wing_area",
            ConstraintFamily::Geometry,
            1.01,
            1.0,
            "-",
            0.01,
            0.01,
            ConstraintPolicy::Soft,
        );
        let outcome = apply_relaxation(&config, &[soft]);
        assert!(!outcome.rejected);
        assert!(outcome.relaxed_ids.is_empty());
        assert!(!outcome.is_relaxed());
    }
}
