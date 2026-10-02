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

use alas_config::{AlasConfig, ConstraintPolicy, ObjectiveWeights};

use crate::envelope::{ModelCgConstraint, ModelCgConstraintAssessment, ModelCgLoadingAssessment};

use super::residuals_geometry::geometry_residuals;
use super::residuals_layout::layout_residuals;
use super::residuals_performance::performance_residuals;
use super::sizing::SizingOutcome;
use super::types::ConstraintFamily::{Balance, Mass};
use super::types::ConstraintResidual;

mod balance_ledger;
#[cfg(test)]
mod critical_tests;
#[cfg(test)]
mod declared_fuel_tests;
mod mtow_modes;
mod relative_balance;
#[cfg(test)]
mod relative_tests;

pub use relative_balance::reporting_relative_balance;

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
    // Loads use the sized closure mass in every mode, the same binding the
    // pipeline gives the final report and its structural stage; a registered
    // aircraft keeps its declared design gross mass through the overrides
    // `at_closure_mass` writes, except in the two design modes, which design
    // it at the closure (`at_sized_closure_mass`).
    let structural_config = config.at_sized_closure_mass(outcome.sized.takeoff_mass_kg);
    residuals.extend(super::structural_feasibility::residuals(
        &structural_config,
        &outcome.history.dv,
        &outcome.plane,
        outcome.masses.wing,
    ));
    residuals.extend(mass_residuals(outcome, config));
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
    residuals.extend(layout_residuals(
        outcome,
        config,
        objective.geometry_constraints,
    ));
    residuals.extend(super::residuals_buffet::buffet_residuals(
        outcome,
        config,
        objective.performance_constraints,
    ));
    residuals
}

/// The published usable fuel mass of the registered aircraft a reference
/// adaptation redesigns, kg, or `None` in any other mode or when the preset
/// carries no published figure. A clean-sheet study has no such requirement,
/// and the working-default FLOPS capacity is a placeholder, not a source.
fn published_fuel(config: &AlasConfig) -> Option<(Option<f64>, Option<f64>)> {
    if config.optimizer.design_space.mode != alas_config::DesignMode::ReferenceAdaptation {
        return None;
    }
    let reference = &alas_config::presets::get(&config.preset).ok()?.reference;
    let positive = |v: Option<f64>| v.filter(|x| x.is_finite() && *x > 0.0);
    Some((
        positive(reference.usable_fuel_mass_kg),
        positive(reference.usable_fuel_volume_l),
    ))
}

/// Modelled usable tank capacity of the registered preset design, kg, for a
/// reference adaptation; `None` in any other mode or when it cannot be
/// resolved. Resolved once per complete configuration
/// ([`super::nominal_cache`]): geometry, structures, tank declarations and
/// fuel density all move it.
fn nominal_tank_capacity_kg(config: &AlasConfig) -> Option<f64> {
    published_fuel(config)?;
    NOMINAL_TANK_CAPACITY_KG.get_or_resolve(config, || {
        let design = alas_config::presets::get(&config.preset)
            .ok()?
            .design_vector;
        let plane = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), false)
            .ok()?;
        super::tanks::tank_capacity_kg(config, &plane, &design)
    })
}

/// The cache of [`nominal_tank_capacity_kg`].
static NOMINAL_TANK_CAPACITY_KG: super::nominal_cache::NominalCache<f64> =
    super::nominal_cache::NominalCache::new();

/// The fuel-capacity, takeoff-mass-ceiling, landing-mass and sizing-closure
/// residuals.
fn mass_residuals(outcome: &SizingOutcome, config: &AlasConfig) -> Vec<ConstraintResidual> {
    let objective = &config.optimizer.objective;
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

    // A reference adaptation must keep the tanks the registered aircraft's
    // mission needs. The route check above does not test it: a short route
    // leaves most of the volume empty. Where the plan closes on a design
    // mission (design range at design payload), the usable capacity must
    // hold that mission's reserve-inclusive takeoff fuel
    // (`SizedCandidate::design_mission_fuel_kg`); the taxi-out fuel on top of
    // it is the route check's, which then flies the same mission. Otherwise
    // the fallback is the modelled capacity of the preset design vector:
    // comparing the model with itself cancels its density and volume bias
    // (the published kg are converted at each type's own density, the model
    // at one global density), which a comparison with the published figure
    // would carry.
    if let (Some(nominal_kg), true) = (
        nominal_tank_capacity_kg(config),
        sized.usable_capacity_kg.is_finite(),
    ) {
        let required_kg =
            if outcome.plan.design_mission.is_some() && sized.design_mission_fuel_kg.is_finite() {
                sized.design_mission_fuel_kg
            } else {
                nominal_kg
            };
        residuals.push(ConstraintResidual::scaled(
            "fuel_capacity_declared",
            Mass,
            sized.usable_capacity_kg,
            required_kg,
            "kg",
            required_kg - sized.usable_capacity_kg,
            policy,
        ));
        // Published figures, for context only; never ranked.
        if let Some(published) = published_fuel(config) {
            if let Some(kg) = published.0 {
                residuals.push(ConstraintResidual::scaled(
                    "fuel_capacity_published",
                    Mass,
                    sized.usable_capacity_kg,
                    kg,
                    "kg",
                    kg - sized.usable_capacity_kg,
                    ConstraintPolicy::Diagnostic,
                ));
            }
            if let Some(litres) = published.1 {
                let modelled_l = sized.usable_capacity_kg
                    / config.mass_model.fuel_density_kg_m3.max(1e-9)
                    * 1_000.0;
                residuals.push(ConstraintResidual::scaled(
                    "fuel_volume_published",
                    Mass,
                    modelled_l,
                    litres,
                    "L",
                    litres - modelled_l,
                    ConstraintPolicy::Diagnostic,
                ));
            }
        }
    }

    // The takeoff-mass limits follow the plan (`alas_config::MtowPlan`):
    // `mtow_ceiling` for the two original ceiling-bound modes, the hard band
    // pair for `MtowBand`, and nothing for `Unconstrained` and
    // `PayloadAdjusted`, which declare no ceiling (a residual against the
    // seed would reject every closure above it under the default hard
    // policy, defeating the mode). A design-mission closure adds the
    // off-design route checks.
    residuals.extend(mtow_modes::plan_residuals(
        &outcome.plan,
        sized,
        outcome.mtow_ceiling,
        policy,
    ));

    // The landing-mass limit is the sizing basis's own `design_landing_mass_kg`
    // (`alas_config::MassSizingBasis`, computed once in `mdo::sizing` and
    // carried on `SizedCandidate` as the single reported value): the declared
    // `WLDG` in every mode for a fixed aircraft, unaffected by `MtowSizing`,
    // and the configured fraction of the closed takeoff mass for a coupled
    // clean-sheet design, consistent with `mdo::mda::converge`'s own per-pass
    // recomputation there.
    let mlw_kg = sized.design_landing_mass_kg;
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
        // MDA returns CandidateFailure("cancelled") before constructing a
        // SizedCandidate; cancellation is never a physical residual.
        alas_mass::dispatch::DispatchStatus::Cancelled => return residuals,
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
const BALANCE_CONSTRAINTS: [(&str, ModelCgConstraint, bool); 9] = [
    (
        "static_margin_floor",
        ModelCgConstraint::StaticStabilityFloor,
        true,
    ),
    (
        "forward_cg_range",
        ModelCgConstraint::PhysicalForwardCgLimit,
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
    (
        "max_nose_gear_load",
        ModelCgConstraint::MaximumNoseGearLoadFraction,
        false,
    ),
    ("tip_back", ModelCgConstraint::TipBack, true),
    ("tail_scrape", ModelCgConstraint::TailScrape, true),
    (
        "minimum_usable_cg_range",
        ModelCgConstraint::MinimumUsableCgRange,
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
    let assessment = relative_balance::assess(outcome, config);
    let Ok(assessment) = assessment else {
        tracing::debug!(error = ?assessment.err(), "candidate item-level CG assessment unavailable");
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
    // Hard, preset-anchored companions of the two diagnostic residuals below.
    let mut residuals = relative_balance::residuals(&assessment, config, policy);
    residuals.extend(
        BALANCE_CONSTRAINTS
            .iter()
            .filter_map(|&(id, constraint, is_lower_bound)| {
                worst_by_constraint(&assessment.loading_states, constraint).map(|worst| {
                    let raw_residual = if is_lower_bound {
                        worst.limit - worst.actual
                    } else {
                        worst.actual - worst.limit
                    };
                    // Diagnostic constraints (`ModelCgConstraint::is_diagnostic`:
                    // the configured CG range, and tail scrape until the aft
                    // fuselage contour is validated) are reported and visible to
                    // the relaxation review but never reject a candidate, whatever the
                    // Balance family policy -- unless the family is `Off`, which
                    // already short-circuits above.
                    let residual_policy = if constraint.is_diagnostic() {
                        ConstraintPolicy::Diagnostic
                    } else {
                        policy
                    };
                    ConstraintResidual::direct(
                        id,
                        Balance,
                        worst.actual,
                        worst.limit,
                        constraint.unit(),
                        raw_residual,
                        worst.normalized_exceedance,
                        residual_policy,
                    )
                })
            }),
    );
    residuals
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
