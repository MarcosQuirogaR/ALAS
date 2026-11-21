// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The landing-weight-limited payload of a route dispatch.
//!
//! A dispatcher does not take off at a mass that lands above the maximum
//! landing weight: the regulated takeoff weight is the lowest of the
//! structural takeoff limit, the maximum zero-fuel weight plus the takeoff
//! fuel, and the maximum landing weight plus the trip fuel, and the traffic
//! load the flight may carry is that weight less the operating weight and the
//! takeoff fuel (the load sheet's "allowed traffic load"). Torenbeek,
//! *Advanced Aircraft Design*, Wiley 2013, Sec. 2.2, Eq. 2.6 (`W_L = W_ZF +
//! W_resf`) and Jenkinson, *Civil Jet Aircraft Design*, 1999, Ch. 12 state the
//! same hierarchy for design: operating weight + payload + reserve fuel <=
//! MLW. The payload offloaded is at most the payload carried: the search never
//! goes below the operating empty mass.
//!
//! The route dispatch used to load the report's payload whatever its arrival:
//! a freighter at its maximum zero-fuel mass (DC-10-30F: MLW - MZFW =
//! 9,072 kg against 11.1 t of contingency, alternate and final reserve on a
//! 2,956 nmi route) then landed above its MLW and the run was declared
//! infeasible, where an operator offloads the excess traffic load and flies.
//! [`limit_to_landing_mass`] does that: it lowers the zero-fuel mass of the
//! dispatch until the destination landing mass meets the limit, so the
//! offloaded payload is the landing-weight limitation of the route, reported
//! by the feasibility stage, not an infeasible aircraft. The operating empty
//! mass, the fuel policy and the limits are unchanged.
//!
//! Only a flown policy load case is limited ([`limit_route_dispatch`]): a
//! maximum-available-fuel flight carries the report's full load, so the route
//! check priced beside it keeps that load and its arrival above the maximum
//! landing mass stays the hard landing-mass finding.
//!
//! The limit is met with the flown closure's mass resolution to spare
//! ([`super::closure_mass_tolerance_kg`]): the reported flight re-flies the
//! settled plan, and its arrival may differ from the dispatch's landing mass by
//! that resolution. A dispatch landing on MLW to a few kilograms then arrives a
//! few kilograms above it (DC-10-30F: 190,963 kg against 190,960 kg).

use alas_config::{AlasConfig, FuelPolicyConfig};
use alas_mass::dispatch::{
    solve_dispatch_with_initial_guess, DispatchLimits, DispatchSolution, DispatchStatus,
};
use alas_mass::fuel_plan::FuelBurnModel;

use crate::full_analysis::AnalysisReport;

/// Dispatch solves the zero-fuel-mass search may take. The landing mass is
/// the zero-fuel mass plus reserves that grow with the mass, so its slope on
/// the zero-fuel mass is just above one and a secant lands in two or three.
const PASSES: usize = 6;

/// [`limit_to_landing_mass`] on the route dispatch `solution` of the aircraft
/// `report` describes, with the dispatch settings of `config`, when the policy
/// load case is flown (`config.fuel_policy.fly_policy_load_case`); otherwise
/// `solution` unchanged. The search stops at the operating empty mass: the
/// dispatch's zero-fuel mass less the report's payload.
pub(super) fn limit_route_dispatch(
    config: &AlasConfig,
    report: &AnalysisReport,
    solution: DispatchSolution,
    range_m: f64,
    model: &dyn FuelBurnModel,
    limits: &DispatchLimits,
) -> DispatchSolution {
    if !config.fuel_policy.fly_policy_load_case {
        return solution;
    }
    let payload_kg = report
        .component_masses
        .get(alas_mass::breakdown::PAYLOAD)
        .copied()
        .filter(|kg| kg.is_finite() && *kg > 0.0)
        .unwrap_or(0.0);
    let minimum_zero_fuel_mass_kg = solution.zero_fuel_mass_kg - payload_kg;
    let margin_kg = super::closure_mass_tolerance_kg(config, solution.takeoff_mass_kg);
    limit_to_landing_mass(
        solution,
        minimum_zero_fuel_mass_kg,
        range_m,
        &config.fuel_policy,
        model,
        limits,
        config.optimizer.objective.sizing_max_iterations.max(1) as usize,
        config.optimizer.objective.sizing_tolerance_kg,
        margin_kg,
    )
}

/// `solution` with its zero-fuel mass lowered, when its destination landing
/// mass exceeds `limits.mlw_kg` less `margin_kg`, to the heaviest zero-fuel
/// mass whose dispatch lands at or below that (to `tolerance_kg`), and never
/// below `minimum_zero_fuel_mass_kg` (the operating empty mass: all the
/// payload offloaded). `margin_kg` is the mass resolution of the flight that
/// re-flies the dispatch, so that flight's arrival stays at or below MLW.
///
/// A solution within the limit, without a declared limit, or that could not
/// be solved is returned unchanged; so is one no search pass brings within
/// the limit, whose excess the feasibility stage then reports as before.
// Each argument is the dispatch input of the same name in `solve_dispatch_with_initial_guess`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn limit_to_landing_mass(
    solution: DispatchSolution,
    minimum_zero_fuel_mass_kg: f64,
    range_m: f64,
    policy: &FuelPolicyConfig,
    model: &dyn FuelBurnModel,
    limits: &DispatchLimits,
    max_iterations: usize,
    tolerance_kg: f64,
    margin_kg: f64,
) -> DispatchSolution {
    let Some(mlw_kg) = limits.mlw_kg.filter(|kg| kg.is_finite() && *kg > 0.0) else {
        return solution;
    };
    // The landing mass the dispatch must meet: MLW less the re-flown arrival's resolution.
    let limit_kg = mlw_kg - margin_kg.max(0.0);
    let solved = |solution: &DispatchSolution| {
        !matches!(
            solution.status,
            DispatchStatus::ModelFailed(_) | DispatchStatus::Cancelled
        )
    };
    if !solved(&solution) || solution.destination_landing_mass_kg <= limit_kg {
        return solution;
    }
    // Aim half a tolerance under the limit: the landing check is strict.
    let target_kg = limit_kg - 0.5 * tolerance_kg;
    // (zero-fuel mass, landing mass) of the last solve, for the secant.
    let mut last = (
        solution.zero_fuel_mass_kg,
        solution.destination_landing_mass_kg,
    );
    let mut slope = 1.0;
    let mut takeoff_guess_kg = solution.takeoff_mass_kg;
    let mut best: Option<DispatchSolution> = None;
    for _ in 0..PASSES {
        let zero_fuel_mass_kg =
            (last.0 - (last.1 - target_kg) / slope).max(minimum_zero_fuel_mass_kg);
        // Pinned at the operating empty mass: no payload is left to offload.
        if !(zero_fuel_mass_kg.is_finite() && zero_fuel_mass_kg > 0.0)
            || zero_fuel_mass_kg >= last.0
        {
            break;
        }
        let next = solve_dispatch_with_initial_guess(
            zero_fuel_mass_kg,
            takeoff_guess_kg + (zero_fuel_mass_kg - last.0),
            range_m,
            policy,
            model,
            limits,
            max_iterations,
            tolerance_kg,
        );
        if !solved(&next) {
            break;
        }
        let landing_kg = next.destination_landing_mass_kg;
        // Reserves grow with mass, so the landing mass rises at least one
        // to one with the zero-fuel mass; a flatter secant is noise.
        let secant = (landing_kg - last.1) / (zero_fuel_mass_kg - last.0);
        slope = if secant.is_finite() && secant >= 1.0 {
            secant
        } else {
            1.0
        };
        last = (zero_fuel_mass_kg, landing_kg);
        takeoff_guess_kg = next.takeoff_mass_kg;
        let within = landing_kg <= limit_kg;
        let on_limit = within && limit_kg - landing_kg <= tolerance_kg;
        if within
            && best
                .as_ref()
                .is_none_or(|kept| next.zero_fuel_mass_kg > kept.zero_fuel_mass_kg)
        {
            best = Some(next);
        }
        if on_limit {
            break;
        }
    }
    best.unwrap_or(solution)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use alas_config::FuelScheme;
    use alas_mass::fuel_plan::{FuelModelError, LegEstimate};

    /// A burn model whose trip and reserves scale with mass, as a jet's do.
    pub(crate) struct Proportional;

    impl FuelBurnModel for Proportional {
        fn trip(&self, mass_kg: f64, _range_m: f64) -> Result<LegEstimate, FuelModelError> {
            Ok(LegEstimate {
                fuel_kg: 0.2 * mass_kg,
                time_s: 20_000.0,
            })
        }
        fn diversion(&self, mass_kg: f64, _distance_m: f64) -> Result<LegEstimate, FuelModelError> {
            Ok(LegEstimate {
                fuel_kg: 0.025 * mass_kg,
                time_s: 2_700.0,
            })
        }
        fn holding_fuel_flow_kg_s(&self, mass_kg: f64, _: f64) -> Result<f64, FuelModelError> {
            Ok(1.1e-5 * mass_kg)
        }
        fn cruise_fuel_flow_kg_s(&self, mass_kg: f64) -> Result<f64, FuelModelError> {
            Ok(1.2e-5 * mass_kg)
        }
        fn taxi_fuel_flow_kg_s(&self) -> Result<f64, FuelModelError> {
            Ok(0.5)
        }
    }

    pub(crate) fn policy() -> FuelPolicyConfig {
        FuelPolicyConfig {
            scheme: FuelScheme::EasaBasic,
            ..FuelPolicyConfig::default()
        }
    }

    pub(crate) fn limits(mlw_kg: Option<f64>) -> DispatchLimits {
        DispatchLimits {
            mtow_kg: 260_000.0,
            mzfw_kg: None,
            mlw_kg,
            usable_capacity_kg: None,
        }
    }

    #[test]
    fn a_dispatch_above_its_landing_limit_offloads_payload_onto_the_limit() {
        let model = Proportional;
        let full = solve_dispatch_with_initial_guess(
            181_000.0,
            240_000.0,
            5.0e6,
            &policy(),
            &model,
            &limits(None),
            50,
            1.0,
        );
        let mlw_kg = full.destination_landing_mass_kg - 1_500.0;
        let limited = limit_to_landing_mass(
            full.clone(),
            full.zero_fuel_mass_kg - 70_000.0,
            5.0e6,
            &policy(),
            &model,
            &limits(Some(mlw_kg)),
            50,
            1.0,
            0.0,
        );
        assert!(limited.destination_landing_mass_kg <= mlw_kg);
        assert!(mlw_kg - limited.destination_landing_mass_kg <= 1.0);
        // The offload is a little less than the excess: reserves fall with it.
        let offload_kg = full.zero_fuel_mass_kg - limited.zero_fuel_mass_kg;
        assert!(offload_kg > 1_000.0 && offload_kg < 1_500.0, "{offload_kg}");
    }

    #[test]
    fn a_dispatch_within_its_landing_limit_is_unchanged() {
        let model = Proportional;
        let full = solve_dispatch_with_initial_guess(
            181_000.0,
            240_000.0,
            5.0e6,
            &policy(),
            &model,
            &limits(None),
            50,
            1.0,
        );
        let mlw_kg = full.destination_landing_mass_kg + 1.0;
        let kept = limit_to_landing_mass(
            full.clone(),
            full.zero_fuel_mass_kg - 70_000.0,
            5.0e6,
            &policy(),
            &model,
            &limits(Some(mlw_kg)),
            50,
            1.0,
            0.0,
        );
        assert_eq!(kept, full);
    }

    #[test]
    fn the_offload_never_exceeds_the_payload_carried() {
        let model = Proportional;
        let full = solve_dispatch_with_initial_guess(
            181_000.0,
            240_000.0,
            5.0e6,
            &policy(),
            &model,
            &limits(None),
            50,
            1.0,
        );
        // 500 kg of payload against a 1,500 kg excess: the landing limit cannot be met.
        let mlw_kg = full.destination_landing_mass_kg - 1_500.0;
        let kept = limit_to_landing_mass(
            full.clone(),
            full.zero_fuel_mass_kg - 500.0,
            5.0e6,
            &policy(),
            &model,
            &limits(Some(mlw_kg)),
            50,
            1.0,
            0.0,
        );
        // Nothing lands within the limit, so the full load and its excess stand.
        assert_eq!(kept, full);
    }

    /// DC-10-30F: a dispatch landing 3 kg under MLW was kept, and the
    /// flight that re-flew it arrived 3 kg over. Within the closure's mass
    /// resolution of the limit the dispatch is offloaded below MLW by that
    /// resolution, so the re-flown arrival cannot cross it.
    #[test]
    fn a_dispatch_within_the_closure_resolution_of_its_landing_limit_is_offloaded_below_it() {
        let model = Proportional;
        let full = solve_dispatch_with_initial_guess(
            181_000.0,
            240_000.0,
            5.0e6,
            &policy(),
            &model,
            &limits(None),
            50,
            1.0,
        );
        let margin_kg = 2.0e-4 * full.takeoff_mass_kg;
        let mlw_kg = full.destination_landing_mass_kg + 3.0;
        let limited = limit_to_landing_mass(
            full.clone(),
            full.zero_fuel_mass_kg - 70_000.0,
            5.0e6,
            &policy(),
            &model,
            &limits(Some(mlw_kg)),
            50,
            1.0,
            margin_kg,
        );
        let landing_kg = limited.destination_landing_mass_kg;
        assert!(landing_kg <= mlw_kg - margin_kg, "{landing_kg} vs {mlw_kg} - {margin_kg}");
        assert!(mlw_kg - margin_kg - landing_kg <= 1.0, "{landing_kg}");
        assert!(limited.zero_fuel_mass_kg < full.zero_fuel_mass_kg);
        // A dispatch clear of MLW by more than the resolution is unchanged.
        let clear = limit_to_landing_mass(
            full.clone(),
            full.zero_fuel_mass_kg - 70_000.0,
            5.0e6,
            &policy(),
            &model,
            &limits(Some(full.destination_landing_mass_kg + margin_kg + 1.0)),
            50,
            1.0,
            margin_kg,
        );
        assert_eq!(clear, full);
    }
}
