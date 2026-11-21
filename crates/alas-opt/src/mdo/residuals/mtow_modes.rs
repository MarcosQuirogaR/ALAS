// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The takeoff-mass limit residuals of each sizing plan, and the off-design
//! route checks of a plan closed on its design mission.
//!
//! The MTOW band mode is the fixed requirement at the chosen mass `m`: it
//! carries the same `mtow_ceiling` residual as `FixedRequirement`, evaluated
//! against `m`, which is `requirements.mtow_kg` once the candidate is
//! resolved (`AlasConfig::at_band_mass`). The closed takeoff mass is reported
//! beside it as a diagnostic, not a constraint.

use crate::mdo::ResidualRole;

use alas_config::{AlasConfig, MtowPlan, MtowSizing};
use alas_mass::dispatch::DispatchStatus;
use alas_payload::layout::LayoutSummary;
use alas_payload::oew::oew_and_cg;

use super::super::offdesign::OffDesignFlight;
use super::super::types::ConstraintFamily::Mass;
use super::super::types::{ConstraintResidual, SizedCandidate};

/// The takeoff-mass limit residuals of `plan`: `mtow_ceiling` for the
/// ceiling-bound modes (`MtowBand` evaluates it at its chosen mass, plus a
/// `mtow_band_closure` diagnostic), none for the modes with no ceiling.
///
/// `required_takeoff_mass_kg` is the unclamped mission requirement (closure
/// zero-fuel mass plus takeoff fuel), `closed_takeoff_mass_kg` the closed
/// analysis mass and `ceiling_kg` the plan's upper limit.
pub(crate) fn takeoff_mass_residuals(
    plan: &MtowPlan,
    required_takeoff_mass_kg: f64,
    closed_takeoff_mass_kg: f64,
    ceiling_kg: f64,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    if plan.upper_bound_kg.is_none() {
        return Vec::new();
    }
    let mut residuals = vec![ConstraintResidual::scaled(
        "mtow_ceiling",
        Mass,
        required_takeoff_mass_kg,
        ceiling_kg,
        "kg",
        required_takeoff_mass_kg - ceiling_kg,
        role,
    )];
    if plan.mode == MtowSizing::MtowBand {
        // How far the closed takeoff mass sits below the chosen mass: a
        // reported value, never a constraint (the ceiling above is the
        // only mass limit, as in the fixed requirement).
        residuals.push(ConstraintResidual::scaled(
            "mtow_band_closure",
            Mass,
            closed_takeoff_mass_kg,
            ceiling_kg,
            "kg",
            closed_takeoff_mass_kg - ceiling_kg,
            ResidualRole::Diagnostic,
        ));
    }
    residuals
}

/// The off-design route checks: ramp fuel against the usable tank capacity, route payload against the derived design structural
/// payload, and route takeoff mass against the closed MTOW.
///
/// A route whose dispatch did not converge has no requirement to compare,
/// so `offdesign_tow` is then reported as a failed boolean.
pub(crate) fn offdesign_residuals(
    flight: &OffDesignFlight,
    role: ResidualRole,
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
            role,
        ));
        return residuals;
    }
    if flight.usable_capacity_kg.is_finite() {
        // Taxi fuel is loaded at the ramp and occupies tank volume, as in
        // the reporting dispatch's tank ceiling.
        let ramp_fuel_kg = flight.dispatch.plan.ramp_fuel_kg();
        residuals.push(ConstraintResidual::scaled(
            "offdesign_fuel_capacity",
            Mass,
            ramp_fuel_kg,
            flight.usable_capacity_kg,
            "kg",
            ramp_fuel_kg - flight.usable_capacity_kg,
            role,
        ));
    }
    residuals.push(ConstraintResidual::scaled(
        "offdesign_payload",
        Mass,
        flight.payload_kg,
        flight.payload_limit_kg,
        "kg",
        flight.payload_kg - flight.payload_limit_kg,
        role,
    ));
    residuals.push(ConstraintResidual::scaled(
        "offdesign_tow",
        Mass,
        flight.required_takeoff_mass_kg,
        flight.mtow_kg,
        "kg",
        flight.required_takeoff_mass_kg - flight.mtow_kg,
        role,
    ));
    residuals
}

/// The finding that a `payload_adjusted` mission on its charted range flies
/// less than the configured load case: `payload_capped_at_chart`, the
/// configured payload against the charted one, a diagnostic reported only
/// when the cap reduces the payload. The configured payload is the layout
/// before the chart cap, laid out on the configuration and empty mass the
/// balance ledger reads.
pub(crate) fn chart_payload_cap_residuals(
    outcome: &super::SizingOutcome,
    config: &AlasConfig,
) -> Vec<ConstraintResidual> {
    let Some(cap_kg) = crate::mdo::build::chart_payload_cap_kg(config) else {
        return Vec::new();
    };
    let mut config = outcome.evaluated_at.map_or_else(
        || config.at_sized_closure_mass(outcome.sized.takeoff_mass_kg),
        |at| at.config(config),
    );
    if crate::objective::apply_candidate_payload_load_case(&mut config, &outcome.history.dv)
        .is_err()
    {
        // The balance ledger reports the same load-case error.
        return Vec::new();
    }
    let (oew_kg, x_oew_m) = oew_and_cg(&outcome.masses, &outcome.coords);
    let Ok(layout) = crate::mdo::build::structurally_capped_payload_layout(
        &config,
        &outcome.history.dv,
        &outcome.plane,
        oew_kg,
        x_oew_m,
    ) else {
        return Vec::new();
    };
    let configured_kg = layout.total_mass;
    // Not above the cap, or not comparable (NaN): nothing to report.
    if configured_kg.partial_cmp(&cap_kg) != Some(std::cmp::Ordering::Greater) {
        return Vec::new();
    }
    let carried = if matches!(layout.summary, LayoutSummary::Passenger(_)) {
        "the dispatch, the balance and the loading envelope carry the capped payload"
    } else {
        "the dispatch flies the capped payload; the freighter deck is not scaled, so \
         the balance and the loading envelope carry the configured one"
    };
    vec![ConstraintResidual::scaled(
        "payload_capped_at_chart",
        Mass,
        configured_kg,
        cap_kg,
        "kg",
        configured_kg - cap_kg,
        ResidualRole::Diagnostic,
    )
    .with_detail(format!(
        "configured load case {configured_kg:.0} kg capped at the charted {cap_kg:.0} kg; {carried}"
    ))]
}

/// The residuals of `sized`'s plan-dependent mass limits.
pub(crate) fn plan_residuals(
    plan: &MtowPlan,
    sized: &SizedCandidate,
    ceiling_kg: f64,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    let required_takeoff_mass_kg =
        sized.dispatch.zero_fuel_mass_kg + sized.dispatch.plan.takeoff_fuel_kg();
    let mut residuals = takeoff_mass_residuals(
        plan,
        required_takeoff_mass_kg,
        sized.takeoff_mass_kg,
        ceiling_kg,
        role,
    );
    if let Some(flight) = &sized.mtow.offdesign {
        residuals.extend(offdesign_residuals(flight, role));
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
        config.optimizer.objective.mtow_band_fraction = 0.1;
        // The candidate is resolved at the chosen mass before it is sized.
        config.at_band_mass(Some(105_000.0)).mtow_plan()
    }

    fn raw(residuals: &[ConstraintResidual], id: &str) -> f64 {
        residuals
            .iter()
            .find(|residual| residual.id == id)
            .map(|residual| residual.raw_residual)
            .unwrap_or_else(|| panic!("{id} missing"))
    }

    #[test]
    fn the_band_carries_the_fixed_ceiling_at_the_chosen_mass_and_nothing_else_hard() {
        let plan = band_plan();
        let fixed = {
            let mut config = AlasConfig::default();
            config.requirements.mtow_kg = 105_000.0;
            config.optimizer.objective.mtow_sizing = MtowSizing::FixedRequirement;
            config.mtow_plan()
        };
        assert_eq!(plan.upper_bound_kg, fixed.upper_bound_kg);
        for required_kg in [80_000.0, 104_990.0, 105_010.0, 120_000.0] {
            let band = takeoff_mass_residuals(
                &plan,
                required_kg,
                required_kg.min(105_000.0),
                105_000.0,
                ResidualRole::Constraint,
            );
            let reference = takeoff_mass_residuals(
                &fixed,
                required_kg,
                required_kg.min(105_000.0),
                105_000.0,
                ResidualRole::Constraint,
            );
            // Every non-diagnostic residual is the fixed one, value for value.
            let hard = |residuals: &[ConstraintResidual]| -> Vec<(String, f64, f64)> {
                residuals
                    .iter()
                    .filter(|residual| residual.role != ResidualRole::Diagnostic)
                    .map(|residual| {
                        (
                            residual.id.to_string(),
                            residual.raw_residual,
                            residual.normalized_violation,
                        )
                    })
                    .collect()
            };
            assert_eq!(hard(&band), hard(&reference), "{required_kg}");
            assert_eq!(hard(&band).len(), 1);
        }
        let at = |required_kg: f64| {
            takeoff_mass_residuals(
                &plan,
                required_kg,
                required_kg,
                105_000.0,
                ResidualRole::Constraint,
            )
        };
        assert!(raw(&at(104_990.0), "mtow_ceiling") < 0.0);
        assert!(raw(&at(105_010.0), "mtow_ceiling") > 0.0);
        // No lower edge: a light closure is not a violation, and the closure
        // below the chosen mass is only a reported diagnostic.
        assert!(at(60_000.0)
            .iter()
            .all(|residual| residual.normalized_violation <= 0.0));
        let closure = at(96_000.0)
            .into_iter()
            .find(|residual| residual.id == "mtow_band_closure")
            .expect("the closure is reported");
        assert_eq!(closure.role, ResidualRole::Diagnostic);
        assert_eq!(closure.raw_residual, -9_000.0);
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
                ResidualRole::Constraint,
            );
            assert_eq!(residuals.len(), expected, "{mode:?}");
            assert!(residuals
                .iter()
                .all(|residual| residual.id == "mtow_ceiling"));
        }
    }
}
