// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Turning a sized candidate into the typed residual table.
//!
//! Every requirement family is evaluated with hard constraints. The mass and balance families are evaluated here; the
//! performance and geometry families are large enough on their own that they
//! live in `mdo::residuals_performance` and `mdo::residuals_geometry`.

use crate::mdo::ResidualRole;

use alas_config::{AlasConfig, ObjectiveWeights};

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
mod declared_fuel;
#[cfg(test)]
mod declared_fuel_tests;
pub(super) mod gear_placement;
mod mtow_modes;
mod public_planning;
mod relative_balance;
#[cfg(test)]
mod relative_tests;

#[cfg(test)]
use declared_fuel::NOMINAL_TANK_CAPACITY_KG;
use declared_fuel::{nominal_tank_capacity_kg, published_fuel};
pub use relative_balance::reporting_relative_balance;

/// Every requirement family's residuals for one sized candidate.
pub(crate) fn build(
    outcome: &SizingOutcome,
    config: &AlasConfig,
    weights: &ObjectiveWeights,
    target_num_passengers: i64,
    target_cargo_payload_kg: f64,
) -> Vec<ConstraintResidual> {
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
    residuals.extend(balance_residuals(outcome, config, ResidualRole::Constraint));
    residuals.extend(public_planning::residuals(outcome, config));
    residuals.extend(performance_residuals(
        outcome,
        config,
        ResidualRole::Constraint,
    ));
    residuals.extend(geometry_residuals(
        outcome,
        config,
        weights,
        ResidualRole::Constraint,
        target_num_passengers,
        target_cargo_payload_kg,
    ));
    residuals.extend(layout_residuals(outcome, config, ResidualRole::Constraint));
    residuals.extend(super::residuals_buffet::buffet_residuals(
        outcome,
        config,
        ResidualRole::Constraint,
    ));
    residuals
}

/// The fuel-capacity, takeoff-mass-ceiling, landing-mass and sizing-closure
/// residuals.
fn mass_residuals(outcome: &SizingOutcome, config: &AlasConfig) -> Vec<ConstraintResidual> {
    let role = ResidualRole::Constraint;
    let sized = &outcome.sized;
    let mut residuals = Vec::new();

    if let Some(loading) = sized.takeoff_loading {
        residuals.push(ConstraintResidual::scaled(
            "takeoff_mtow_margin",
            Mass,
            loading.takeoff_mass_kg,
            outcome.mtow_ceiling,
            "kg",
            -loading.mtow_margin_kg,
            ResidualRole::Diagnostic,
        ));
        let mut volume = ConstraintResidual::direct(
            "takeoff_volume_limited",
            Mass,
            f64::from(loading.status == alas_mass::loading::MtowFuelLoadingStatus::VolumeLimited),
            0.0,
            "bool",
            0.0,
            0.0,
            ResidualRole::Diagnostic,
        );
        volume.detail = Some(loading.status.as_str().to_owned());
        residuals.push(volume);
    }

    // The clean-sheet reconciliation currently has an explicit primary box
    // and Torenbeek high-lift/spoiler inventory, while joints, actuators,
    // fairings and other non-box items are not represented by a sourced
    // complete inventory. Keep that limitation binding so a partial wing
    // model cannot become the accepted finalist through a finite penalty.
    // A box exceeding the reference complete wing leaves no nonnegative
    // secondary inventory. Keep this veto in reference and sandbox modes too:
    // the discrepancy must be resolved before delivery, without fitting the
    // physical sizing model to the empirical estimate.
    if !outcome.structural_inventory_complete {
        residuals.push(ConstraintResidual::direct(
            "structural_inventory_unverified",
            Mass,
            1.0,
            0.0,
            "bool",
            1.0,
            1.0,
            role,
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
            role,
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
            role,
        ));
    }

    // A reference adaptation must keep the tanks the registered aircraft's
    // mission needs. The route check above does not test it: a short route
    // leaves most of the volume empty. Where the plan closes on a design
    // mission (design range at design payload), the usable capacity must
    // hold that mission's reserve-inclusive takeoff fuel
    // (`SizedCandidate::design_mission_fuel_kg`); the taxi-out fuel on top of
    // it is the route check's, which then flies the same mission. Otherwise
    // the fallback is the product tank inventory of the preset design vector
    // held against the candidate's own: both come from one resolver
    // (`alas_mass::tanks::resolve_product_layout`) at one fuel density, and
    // the candidate's is the capacity its dispatch is bounded by.
    let capacity_kg = sized.usable_capacity_kg;
    if let (Some(nominal_kg), true) = (nominal_tank_capacity_kg(config), capacity_kg.is_finite()) {
        let required_kg =
            if outcome.plan.design_mission.is_some() && sized.design_mission_fuel_kg.is_finite() {
                sized.design_mission_fuel_kg
            } else {
                nominal_kg
            };
        residuals.push(ConstraintResidual::scaled(
            "fuel_capacity_declared",
            Mass,
            capacity_kg,
            required_kg,
            "kg",
            required_kg - capacity_kg,
            role,
        ));
        // Published figures, for context only; never ranked.
        if let Some(published) = published_fuel(config) {
            if let Some(kg) = published.0 {
                residuals.push(ConstraintResidual::scaled(
                    "fuel_capacity_published",
                    Mass,
                    capacity_kg,
                    kg,
                    "kg",
                    kg - capacity_kg,
                    ResidualRole::Diagnostic,
                ));
            }
            if let Some(litres) = published.1 {
                let modelled_l = capacity_kg
                    / alas_mass::tanks::inventory_density_kg_m3(config).max(1e-9)
                    * 1_000.0;
                residuals.push(ConstraintResidual::scaled(
                    "fuel_volume_published",
                    Mass,
                    modelled_l,
                    litres,
                    "L",
                    litres - modelled_l,
                    ResidualRole::Diagnostic,
                ));
            }
        }
    }

    // The takeoff-mass limits follow the plan (`alas_config::MtowPlan`):
    // `mtow_ceiling` for the two original ceiling-bound modes, the hard band
    // pair for `MtowBand`, and nothing for `Unconstrained` and
    // `PayloadAdjusted`, which declare no ceiling (a residual against the
    // seed would reject every closure above it under the default hard
    // role, defeating the mode). A design-mission closure adds the
    // off-design route checks.
    residuals.extend(mtow_modes::plan_residuals(
        &outcome.plan,
        sized,
        outcome.mtow_ceiling,
        role,
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
        role,
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
                role,
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
                    role,
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
                    role,
                )
            });
        }
        alas_mass::dispatch::DispatchStatus::NotConverged { last_change_kg } => {
            residuals.push(ConstraintResidual::scaled(
                "dispatch_not_converged",
                Mass,
                *last_change_kg,
                config.optimizer.objective.sizing_tolerance_kg,
                "kg",
                last_change_kg.abs(),
                role,
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
                role,
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
        role,
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
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    let (assessment, flown) = match relative_balance::assess_states(outcome, config) {
        Ok(assessments) => assessments,
        Err(error) => {
            tracing::debug!(error = %error, "candidate item-level CG assessment unavailable");
            return vec![ConstraintResidual::direct(
                "cg_model_error",
                Balance,
                1.0,
                0.0,
                "bool",
                1.0,
                1.0,
                role,
            )
            .with_detail(error)];
        }
    };
    // Hard, preset-anchored companions of the two diagnostic residuals below.
    // They compare design loadings, as the reporting guard does.
    let mut residuals = relative_balance::residuals(&assessment, config, role);
    // The absolute constraints hold in every state the reporting verdict
    // gates: the design loading and the dispatched route's takeoff and
    // landing.
    let states: Vec<(&str, ModelCgLoadingAssessment)> = assessment
        .loading_states
        .into_iter()
        .map(|state| ("design", state))
        .chain(
            flown
                .into_iter()
                .flat_map(|flown| flown.loading_states)
                .map(|state| ("flown", state)),
        )
        .collect();
    residuals.extend(
        BALANCE_CONSTRAINTS
            .iter()
            .filter_map(|&(id, constraint, is_lower_bound)| {
                let (loading, state, worst) = states
                    .iter()
                    .flat_map(|(loading, state)| {
                        state
                            .constraints
                            .iter()
                            .filter(move |candidate| candidate.constraint == constraint)
                            .map(move |candidate| (*loading, state.state, *candidate))
                    })
                    .max_by(|a, b| {
                        a.2.normalized_exceedance
                            .total_cmp(&b.2.normalized_exceedance)
                    })?;
                let raw_residual = if is_lower_bound {
                    worst.limit - worst.actual
                } else {
                    worst.actual - worst.limit
                };
                // Unvalidated CG-range and tail-scrape measurements are diagnostics.
                let residual_policy = if constraint.is_diagnostic() {
                    ResidualRole::Diagnostic
                } else {
                    role
                };
                // The governing loading and state, so a reader can match the
                // residual to the report's own state.
                Some(
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
                    .with_detail(format!("{loading} {}", state.label())),
                )
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
