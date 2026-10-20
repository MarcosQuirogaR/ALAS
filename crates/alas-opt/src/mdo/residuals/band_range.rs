// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The payload-range floor of the MTOW band mode.
//!
//! The band searches the takeoff mass, and the objective rewards a lighter
//! aircraft, so without a capability requirement the search gives the
//! registered aircraft's range away (C919 band winner of 2026-10-18: corner B
//! range 2,249 -> 996 nmi). A band design must keep the registered aircraft's
//! corner B: the maximum structural payload flown at the maximum takeoff
//! mass, with the fuel the mass budget leaves (limited by the tanks), over the
//! range the registered aircraft flies it. Corner B is the corner the report's
//! payload-range chart draws ([`alas-pipeline`'s `corner_ranges`]); the same
//! reserve rules ([`alas_mass::fuel_policy::plan_fuel`], the configured fuel
//! policy) and the same mission model ([`candidate_mission_model`], flown as a
//! range search) price both sides.
//!
//! The baseline range is resolved once per configuration (the registered
//! design sized through the band path at the band target `T`, the declared
//! MTOW),
//! by the corner search. The candidate is not searched: range is monotone in
//! fuel at a fixed takeoff mass, so `range_B(candidate) >= R` is exactly
//! "the reserve-inclusive takeoff fuel for `R` at the candidate's corner B
//! mass fits the fuel it carries", one fuel plan instead of a bisection.
//! The residual is that fuel excess, kg.
//!
//! Tolerance: `R = range_B(baseline) - 500 m`, i.e. a relative tolerance of
//! `500 m / range_B(baseline)` (about 1e-4). It is the resolution of the
//! corner range search itself ([`BAND_RANGE_TOLERANCE_M`]), a numerical
//! tolerance and not a design margin.

use alas_config::{presets, AlasConfig, DesignMode, MtowSizing};
use alas_mass::fuel_plan::FuelBurnModel;
use alas_mass::fuel_policy::plan_fuel;
use alas_mass::payload_range::{max_range_with_reserves, RangeStatus};

use crate::mdo::mission_model::SegmentMissionModel;
use crate::mdo::nominal_cache::NominalCache;
use crate::mdo::types::ConstraintFamily::Mass;
use crate::mdo::types::{ConstraintResidual, SizedCandidate};
use crate::mdo::{candidate_mission_model, PlannedTrips, ResidualRole};

/// Resolution of the corner range search, m. Equal to the value the reporting
/// chart searches with (`alas_pipeline::quick_analysis::corners::
/// CORNER_TOLERANCE_M`), so the baseline range found here is the chart's to
/// within one search step.
pub(super) const BAND_RANGE_TOLERANCE_M: f64 = 500.0;

/// Seconds per minute for the taxi allowance.
const SECONDS_PER_MINUTE: f64 = 60.0;

/// The registered aircraft's corner B.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct BaselineCornerB {
    /// Maximum structural payload carried at the corner, kg.
    pub payload_kg: f64,
    /// Reserve-inclusive still-air range at the corner, m.
    pub range_m: f64,
}

/// Takeoff mass and carried fuel of corner B, kg: `payload_kg` at `mtow_kg`
/// with the fuel the mass budget leaves, limited by the tank capacity less
/// the taxi fuel burned before brake release (the report's `corner_loads`).
fn corner_b_load(
    oew_kg: f64,
    mtow_kg: f64,
    tank_capacity_kg: f64,
    taxi_kg: f64,
    payload_kg: f64,
) -> (f64, f64) {
    let budget_kg = (mtow_kg - oew_kg).max(0.0);
    let tank_kg = (tank_capacity_kg - taxi_kg).max(0.0);
    let fuel_kg = tank_kg.min(budget_kg - payload_kg).max(0.0);
    (oew_kg + payload_kg + fuel_kg, fuel_kg)
}

/// The sized aircraft's range-search model and its taxi fuel, kg.
fn range_model(config: &AlasConfig, sized: &SizedCandidate) -> Option<(SegmentMissionModel, f64)> {
    let model = candidate_mission_model(config, &sized.fuel_artifacts)
        .ok()?
        .for_range_search();
    let taxi_kg =
        model.taxi_fuel_flow_kg_s().ok()? * config.fuel_policy.taxi_time_min * SECONDS_PER_MINUTE;
    taxi_kg.is_finite().then_some((model, taxi_kg))
}

/// The registered aircraft's maximum structural payload at the corner, kg:
/// the tightest of the declared cap, the preset MZFW less the modelled OEW and
/// the mass budget `MTOW - OEW`; with no cap and no MZFW, the carried payload
/// (the report's `payload_capacity_estimate`).
fn baseline_payload_kg(config: &AlasConfig, sized: &SizedCandidate) -> f64 {
    let positive = |value: f64| value.is_finite() && value > 0.0;
    let oew_kg = sized.operating_empty_mass_kg;
    let mut payload_kg = (config.requirements.mtow_kg - oew_kg).max(0.0);
    let mzfw_limit_kg = presets::get(&config.preset)
        .ok()
        .and_then(|preset| preset.reference.mzfw_kg)
        .map(|mzfw_kg| mzfw_kg - oew_kg)
        .filter(|kg| positive(*kg));
    if let Some(limit_kg) = mzfw_limit_kg {
        payload_kg = payload_kg.min(limit_kg);
    }
    let cap_kg = config.requirements.max_structural_payload_kg;
    if positive(cap_kg) {
        payload_kg = payload_kg.min(cap_kg);
    } else if mzfw_limit_kg.is_none() {
        payload_kg = payload_kg.min(sized.payload_kg.max(0.0));
    }
    payload_kg
}

/// Size the registered aircraft under the band configuration `config` at the
/// band target `T` and find its corner B. This is the candidate path itself
/// evaluated at `m = T` (band at `T` equals the fixed requirement at `T`, value
/// for value: `tests/mtow_band_invariant.rs`), so a candidate that is the
/// registered aircraft at its own mass meets the floor up to the search
/// resolution, with no model offset to absorb. The report's baseline column
/// is the same aircraft closed as a baseline sandbox
/// (`baseline_closure_config`); its corner B fuel differed from this one by at most 0.1 % in the 2026-10-19 probe.
fn resolve(config: &AlasConfig) -> Option<BaselineCornerB> {
    let design = presets::get(&config.preset).ok()?.design_vector;
    let baseline = config.clone();
    let outcome = crate::mdo::sizing::run_candidate_with_fuselage_policy(
        &baseline,
        &design.to_array(),
        false,
    )
    .ok()?;
    let sized = &outcome.sized;
    if !sized.usable_capacity_kg.is_finite() {
        return None;
    }
    let (model, taxi_kg) = range_model(&baseline, sized)?;
    let payload_kg = baseline_payload_kg(&baseline, sized);
    let (tow_kg, fuel_kg) = corner_b_load(
        sized.operating_empty_mass_kg,
        baseline.requirements.mtow_kg,
        sized.usable_capacity_kg,
        taxi_kg,
        payload_kg,
    );
    let corner = max_range_with_reserves(
        tow_kg,
        fuel_kg,
        &baseline.fuel_policy,
        &PlannedTrips(&model),
        BAND_RANGE_TOLERANCE_M,
    );
    (matches!(corner.status, RangeStatus::Solved) && corner.range_m > 0.0).then_some(
        BaselineCornerB {
            payload_kg,
            range_m: corner.range_m,
        },
    )
}

/// The cache of [`baseline_corner_b`].
pub(super) static BASELINE_CORNER_B: NominalCache<BaselineCornerB> = NominalCache::new();

/// Whether `config` is a band run on a registered aircraft, the only case
/// with a baseline whose capability is to be kept.
pub(super) fn applies(config: &AlasConfig) -> bool {
    config.optimizer.objective.mtow_sizing == MtowSizing::MtowBand
        && config.optimizer.design_space.mode == DesignMode::ReferenceAdaptation
}

/// The registered aircraft's corner B for the band configuration `config`,
/// resolved once per configuration (the candidate's chosen mass is replaced by
/// the band target `T`, the declared mass, before the key is formed).
pub(super) fn baseline_corner_b(config: &AlasConfig) -> Option<BaselineCornerB> {
    let nominal = config.at_band_mass(Some(config.mtow_band_target_kg()));
    BASELINE_CORNER_B.get_or_resolve(&nominal, resolve)
}

/// The corner B residual of one band candidate (`config` is the candidate's
/// configuration, `outcome.mtow_ceiling` its chosen mass): the fuel the plan
/// needs for the baseline's corner B range, less the fuel the candidate
/// carries at that corner, kg. Empty when the mode has no baseline to keep.
pub(super) fn residuals(
    outcome: &crate::mdo::sizing::SizingOutcome,
    config: &AlasConfig,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    if !applies(config) {
        return Vec::new();
    }
    let unavailable = |id: &'static str| {
        vec![ConstraintResidual::direct(
            id, Mass, 1.0, 0.0, "bool", 1.0, 1.0, role,
        )]
    };
    let Some(baseline) = baseline_corner_b(&outcome.registered_config(config)) else {
        return unavailable("band_range_baseline_unavailable");
    };
    let sized = &outcome.sized;
    let candidate = config.at_band_mass(Some(outcome.mtow_ceiling));
    let Some((model, taxi_kg)) = range_model(&candidate, sized) else {
        return unavailable("band_range_unavailable");
    };
    if !sized.usable_capacity_kg.is_finite() {
        return unavailable("band_range_unavailable");
    }
    let (tow_kg, carried_kg) = corner_b_load(
        sized.operating_empty_mass_kg,
        outcome.mtow_ceiling,
        sized.usable_capacity_kg,
        taxi_kg,
        baseline.payload_kg,
    );
    // The aircraft cannot lift the baseline's corner B payload at all.
    let shortfall_kg =
        (sized.operating_empty_mass_kg + baseline.payload_kg - outcome.mtow_ceiling).max(0.0);
    let required_range_m = baseline.range_m - BAND_RANGE_TOLERANCE_M;
    let needed_kg = plan_fuel(
        &candidate.fuel_policy,
        &PlannedTrips(&model),
        tow_kg,
        required_range_m,
    )
    .ok()
    .filter(|plan| plan.is_finite_and_nonnegative())
    .map(|plan| plan.takeoff_fuel_kg());
    let Some(needed_kg) = needed_kg else {
        // The model cannot price the baseline's corner B range from this
        // takeoff mass: out of reach, never a pass.
        return unavailable("band_range_unreachable");
    };
    vec![ConstraintResidual::scaled(
        "band_range_corner_b",
        Mass,
        needed_kg,
        carried_kg,
        "kg",
        needed_kg - carried_kg + shortfall_kg,
        role,
    )]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corner_b_carries_the_budget_or_the_tank_whichever_is_smaller() {
        // Budget-limited: 100 - 60 - 30 = 10 kg of fuel, tank has room.
        let (tow, fuel) = corner_b_load(60.0, 100.0, 50.0, 2.0, 30.0);
        assert_eq!((tow, fuel), (100.0, 10.0));
        // Tank-limited: 50 - 2 = 48 kg of fuel against a 70 kg budget.
        let (tow, fuel) = corner_b_load(60.0, 160.0, 50.0, 2.0, 30.0);
        assert_eq!((tow, fuel), (60.0 + 30.0 + 48.0, 48.0));
        // A payload that does not fit leaves no fuel and never goes negative.
        let (tow, fuel) = corner_b_load(60.0, 80.0, 50.0, 2.0, 30.0);
        assert_eq!((tow, fuel), (90.0, 0.0));
    }

    #[test]
    fn only_a_band_on_a_registered_aircraft_carries_the_floor() {
        let mut config = AlasConfig::default();
        config.optimizer.objective.mtow_sizing = MtowSizing::MtowBand;
        config.optimizer.design_space.mode = DesignMode::ReferenceAdaptation;
        assert!(applies(&config));
        config.optimizer.objective.mtow_sizing = MtowSizing::FixedRequirement;
        assert!(!applies(&config));
        config.optimizer.objective.mtow_sizing = MtowSizing::PayloadAdjusted;
        assert!(!applies(&config));
        config.optimizer.objective.mtow_sizing = MtowSizing::MtowBand;
        config.optimizer.design_space.mode = DesignMode::CleanSheet;
        assert!(!applies(&config));
    }
}
