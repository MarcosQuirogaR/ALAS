// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The takeoff-mass limit residuals of each sizing plan, and the off-design
//! route checks of a plan closed on its design mission.
//!
//! The MTOW band is two hard one-sided residuals and nothing else: a closure
//! inside `[T (1 - p), T (1 + p)]` contributes zero to the cost whatever its
//! distance from `T`, so the search is not pulled toward the target and no
//! fuel is added to reach it.

use alas_config::{ConstraintPolicy, MtowPlan, MtowSizing};
use alas_mass::dispatch::DispatchStatus;

use super::super::offdesign::OffDesignFlight;
use super::super::types::ConstraintFamily::Mass;
use super::super::types::{ConstraintResidual, SizedCandidate};

/// The takeoff-mass limit residuals of `plan`: `mtow_ceiling` for the two
/// original ceiling-bound modes, the band pair for `MtowBand`, none for the
/// modes with no ceiling.
///
/// `required_takeoff_mass_kg` is the unclamped mission requirement (closure
/// zero-fuel mass plus takeoff fuel), `closed_takeoff_mass_kg` the closed
/// analysis mass and `ceiling_kg` the plan's upper limit.
pub(crate) fn takeoff_mass_residuals(
    plan: &MtowPlan,
    required_takeoff_mass_kg: f64,
    closed_takeoff_mass_kg: f64,
    ceiling_kg: f64,
    policy: ConstraintPolicy,
) -> Vec<ConstraintResidual> {
    match (plan.mode, plan.upper_bound_kg, plan.lower_bound_kg) {
        (MtowSizing::MtowBand, Some(upper_kg), Some(lower_kg)) => vec![
            ConstraintResidual::scaled(
                "mtow_band_upper",
                Mass,
                required_takeoff_mass_kg,
                upper_kg,
                "kg",
                required_takeoff_mass_kg - upper_kg,
                policy,
            ),
            ConstraintResidual::scaled(
                "mtow_band_lower",
                Mass,
                closed_takeoff_mass_kg,
                lower_kg,
                "kg",
                lower_kg - closed_takeoff_mass_kg,
                policy,
            ),
        ],
        (_, Some(_), _) => vec![ConstraintResidual::scaled(
            "mtow_ceiling",
            Mass,
            required_takeoff_mass_kg,
            ceiling_kg,
            "kg",
            required_takeoff_mass_kg - ceiling_kg,
            policy,
        )],
        _ => Vec::new(),
    }
}

/// The off-design route checks: reserve-inclusive takeoff fuel against the
/// usable tank capacity, route payload against the derived design structural
/// payload, and route takeoff mass against the closed MTOW.
///
/// A route whose dispatch did not converge has no requirement to compare,
/// so `offdesign_tow` is then reported as a failed boolean.
pub(crate) fn offdesign_residuals(
    flight: &OffDesignFlight,
    policy: ConstraintPolicy,
) -> Vec<ConstraintResidual> {
    let mut residuals = Vec::new();
    if !matches!(flight.dispatch.status, DispatchStatus::Converged) {
        residuals.push(ConstraintResidual::direct(
            "offdesign_tow",
            Mass,
            1.0,
            0.0,
            "bool",
            1.0,
            1.0,
            policy,
        ));
        return residuals;
    }
    if flight.usable_capacity_kg.is_finite() {
        residuals.push(ConstraintResidual::scaled(
            "offdesign_fuel_capacity",
            Mass,
            flight.takeoff_fuel_kg,
            flight.usable_capacity_kg,
            "kg",
            flight.takeoff_fuel_kg - flight.usable_capacity_kg,
            policy,
        ));
    }
    residuals.push(ConstraintResidual::scaled(
        "offdesign_payload",
        Mass,
        flight.payload_kg,
        flight.payload_limit_kg,
        "kg",
        flight.payload_kg - flight.payload_limit_kg,
        policy,
    ));
    residuals.push(ConstraintResidual::scaled(
        "offdesign_tow",
        Mass,
        flight.required_takeoff_mass_kg,
        flight.mtow_kg,
        "kg",
        flight.required_takeoff_mass_kg - flight.mtow_kg,
        policy,
    ));
    residuals
}

/// The residuals of `sized`'s plan-dependent mass limits.
pub(crate) fn plan_residuals(
    plan: &MtowPlan,
    sized: &SizedCandidate,
    ceiling_kg: f64,
    policy: ConstraintPolicy,
) -> Vec<ConstraintResidual> {
    let required_takeoff_mass_kg =
        sized.dispatch.zero_fuel_mass_kg + sized.dispatch.plan.takeoff_fuel_kg();
    let mut residuals = takeoff_mass_residuals(
        plan,
        required_takeoff_mass_kg,
        sized.takeoff_mass_kg,
        ceiling_kg,
        policy,
    );
    if let Some(flight) = &sized.mtow.offdesign {
        residuals.extend(offdesign_residuals(flight, policy));
    }
    residuals
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::AlasConfig;

    fn band_plan() -> MtowPlan {
        let mut config = AlasConfig::default();
        config.requirements.mtow_kg = 100_000.0;
        config.optimizer.objective.mtow_sizing = MtowSizing::MtowBand;
        config.optimizer.objective.mtow_band_fraction = 0.05;
        config.mtow_plan()
    }

    fn violation(residuals: &[ConstraintResidual], id: &str) -> f64 {
        residuals
            .iter()
            .find(|residual| residual.id == id)
            .map(|residual| residual.normalized_violation)
            .unwrap_or_else(|| panic!("{id} missing"))
    }

    fn raw(residuals: &[ConstraintResidual], id: &str) -> f64 {
        residuals
            .iter()
            .find(|residual| residual.id == id)
            .map(|residual| residual.raw_residual)
            .unwrap_or_else(|| panic!("{id} missing"))
    }

    #[test]
    fn the_band_residuals_change_sign_at_each_edge() {
        let plan = band_plan();
        let at = |mass_kg: f64| {
            takeoff_mass_residuals(&plan, mass_kg, mass_kg, 105_000.0, ConstraintPolicy::Hard)
        };
        assert!(raw(&at(104_990.0), "mtow_band_upper") < 0.0);
        assert!(raw(&at(105_010.0), "mtow_band_upper") > 0.0);
        assert!(violation(&at(106_000.0), "mtow_band_upper") > 0.0);
        assert!(raw(&at(95_010.0), "mtow_band_lower") < 0.0);
        assert!(raw(&at(94_990.0), "mtow_band_lower") > 0.0);
        assert!(violation(&at(94_000.0), "mtow_band_lower") > 0.0);
        // No ceiling residual beside the band.
        assert!(at(100_000.0)
            .iter()
            .all(|residual| residual.id != "mtow_ceiling"));
    }

    #[test]
    fn two_masses_inside_the_band_cost_the_same_so_nothing_pulls_toward_the_target() {
        let plan = band_plan();
        let total = |mass_kg: f64| -> f64 {
            takeoff_mass_residuals(&plan, mass_kg, mass_kg, 105_000.0, ConstraintPolicy::Hard)
                .iter()
                .map(|residual| residual.normalized_violation)
                .sum()
        };
        assert_eq!(total(96_000.0), 0.0);
        assert_eq!(total(104_000.0), total(96_000.0));
        assert_eq!(total(100_000.0), 0.0);
    }

    #[test]
    fn the_original_modes_keep_their_ceiling_rule() {
        let mut config = AlasConfig::default();
        config.requirements.mtow_kg = 100_000.0;
        for (mode, expected) in [
            (MtowSizing::FixedRequirement, 1),
            (MtowSizing::SizedByMission, 1),
            (MtowSizing::Unconstrained, 0),
            (MtowSizing::PayloadAdjusted, 0),
        ] {
            config.optimizer.objective.mtow_sizing = mode;
            let residuals = takeoff_mass_residuals(
                &config.mtow_plan(),
                101_000.0,
                100_000.0,
                100_000.0,
                ConstraintPolicy::Hard,
            );
            assert_eq!(residuals.len(), expected, "{mode:?}");
            assert!(residuals
                .iter()
                .all(|residual| residual.id == "mtow_ceiling"));
        }
    }
}
