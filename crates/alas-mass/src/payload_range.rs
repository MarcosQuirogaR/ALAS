// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reserve-inclusive range at a fixed takeoff mass and fuel load.
//!
//! A payload-range corner fixes the takeoff mass (maximum payload at MTOW,
//! full tanks at MTOW, or zero payload with full tanks) and the fuel on board
//! at brake release. The range it can fly is the largest still-air distance
//! whose full fuel plan (trip, contingency, alternate, final reserve and any
//! declared extra fuel, [`crate::fuel_policy::plan_fuel`]) fits in that fuel.
//! Because the takeoff mass is fixed, the plan's takeoff fuel is a
//! nondecreasing function of range and no mass iteration is needed; this is
//! the inverse of [`crate::dispatch::solve_dispatch`], which fixes the range
//! and solves the mass.

use alas_config::FuelPolicyConfig;

use crate::fuel_plan::{FuelBurnModel, FuelModelError};
use crate::fuel_policy::plan_fuel;

/// First upper bracket tried for the range search, m.
const FIRST_BRACKET_M: f64 = 250_000.0;

/// Largest range the search will bracket, m. Half of the Earth's
/// circumference is 20,000 km; twice that is beyond any still-air corner.
const MAXIMUM_BRACKET_M: f64 = 40_000_000.0;

/// Bisection iteration limit; 2^-64 of the bracket is far below any tolerance.
const MAXIMUM_ITERATIONS: usize = 64;

/// How a corner search ended.
#[derive(Debug, Clone, PartialEq)]
pub enum RangeStatus {
    /// The range is the largest distance whose plan fits the carried fuel.
    Solved,
    /// The carried fuel does not cover the reserves and the shortest
    /// flyable trip, so no distance can be dispatched.
    ZeroRange,
    /// The burn model or an input could not be evaluated; the text says why.
    ModelFailed(String),
}

/// One payload-range corner priced with reserves.
#[derive(Debug, Clone, PartialEq)]
pub struct RangeCorner {
    /// Largest still-air range the carried fuel supports with reserves, m.
    pub range_m: f64,
    /// Fuel on board at brake release, kg (the input, echoed).
    pub takeoff_fuel_kg: f64,
    /// Fuel the plan holds back beyond the trip at `range_m`: contingency,
    /// alternate, final reserve and declared additional fuel, kg.
    pub reserve_fuel_kg: f64,
    /// How the search ended.
    pub status: RangeStatus,
}

impl RangeCorner {
    fn without_range(carried_fuel_kg: f64, status: RangeStatus) -> Self {
        Self {
            range_m: 0.0,
            takeoff_fuel_kg: carried_fuel_kg,
            reserve_fuel_kg: 0.0,
            status,
        }
    }
}

/// One evaluation of the plan at a trial range.
enum Sample {
    /// Needed minus carried takeoff fuel, kg, and the plan's reserve fuel, kg.
    Fuel {
        excess_kg: f64,
        reserve_kg: f64,
    },
    /// The model reports the trip shorter than its minimum flyable range.
    TooShort,
    /// The model cannot price a trip this long from this mass.
    TooFar,
    /// The leg could not be flown to completion; the text says why. Above a
    /// fuel-verified range this means too far, otherwise it is unknown.
    NotConverged(String),
    Failed(String),
}

/// Bracket state of the range search. `verified` is the largest range at
/// which a plan was evaluated and fits the carried fuel, with its reserve;
/// `low_m` only bounds the search and may rest on a too-short or unconverged
/// range; `unconverged` records the last non-convergence seen before any fit.
struct Search {
    low_m: f64,
    high_m: f64,
    verified: Option<(f64, f64)>,
    unconverged: Option<String>,
}

impl Search {
    /// The status when no range was ever verified against the carried fuel.
    fn unverified_status(&self) -> RangeStatus {
        match &self.unconverged {
            Some(reason) => RangeStatus::ModelFailed(reason.clone()),
            None => RangeStatus::ZeroRange,
        }
    }
}

impl Search {
    /// Fold one sample at `range_m` into the bracket. `Ok(true)` means the
    /// lower bound rose, `Ok(false)` that the upper bound fell; `Err` ends
    /// the search.
    fn apply(&mut self, range_m: f64, outcome: Sample) -> Result<bool, RangeStatus> {
        match outcome {
            Sample::Failed(message) => Err(RangeStatus::ModelFailed(message)),
            Sample::Fuel {
                excess_kg,
                reserve_kg,
            } if excess_kg <= 0.0 => {
                self.low_m = range_m;
                self.verified = Some((range_m, reserve_kg));
                Ok(true)
            }
            Sample::TooShort => {
                self.low_m = range_m;
                Ok(true)
            }
            Sample::NotConverged(reason) => {
                if self.verified.is_some() {
                    self.high_m = range_m;
                    return Ok(false);
                }
                // Short legs can fail to complete the climb and acceleration
                // schedule, so before any range fits this is unknown, not
                // too far: keep searching upward and remember why.
                self.unconverged = Some(format!(
                    "the burn model did not converge at {range_m} m before any range fit the carried fuel: {reason}"
                ));
                self.low_m = range_m;
                Ok(true)
            }
            Sample::Fuel { .. } | Sample::TooFar => {
                self.high_m = range_m;
                Ok(false)
            }
        }
    }
}

fn sample(
    policy: &FuelPolicyConfig,
    model: &dyn FuelBurnModel,
    takeoff_mass_kg: f64,
    carried_fuel_kg: f64,
    range_m: f64,
) -> Sample {
    match plan_fuel(policy, model, takeoff_mass_kg, range_m) {
        Ok(plan) if plan.is_finite_and_nonnegative() => Sample::Fuel {
            excess_kg: plan.takeoff_fuel_kg() - carried_fuel_kg,
            reserve_kg: plan.reserve_fuel_kg(),
        },
        Ok(_) => Sample::TooFar,
        Err(FuelModelError::RouteTooShort { .. }) => Sample::TooShort,
        Err(FuelModelError::NotConverged(reason)) => Sample::NotConverged(reason),
        Err(FuelModelError::Cancelled) => Sample::Failed("fuel evaluation cancelled".to_owned()),
        Err(error @ FuelModelError::InvalidModel(_)) => Sample::Failed(error.to_string()),
        Err(_) => Sample::TooFar,
    }
}

/// The largest range whose full fuel plan fits `carried_takeoff_fuel_kg` at
/// `takeoff_mass_kg`, found by bisection to `tolerance_m`.
///
/// `takeoff_mass_kg` is the mass at brake release and
/// `carried_takeoff_fuel_kg` the fuel inside it (tank fuel less taxi fuel,
/// which is burned before brake release). The returned range is the largest
/// range at which a plan was evaluated and fits the carried fuel, and the
/// reserve is that plan's. A burn model that reports a trip too short
/// ([`FuelModelError::RouteTooShort`]) counts as below reach; one that cannot
/// price a leg because it is too long counts as out of reach. A leg that does
/// not converge counts as out of reach once some range has fit; before that
/// it is unknown (short legs can fail the climb schedule), the search keeps
/// going upward, and if no range ever fits the status is
/// [`RangeStatus::ModelFailed`] with the reason. If no range fits and the
/// model never failed to converge, the status is [`RangeStatus::ZeroRange`].
///
/// Never panics: invalid inputs and model failures are reported through
/// [`RangeStatus`].
pub fn max_range_with_reserves(
    takeoff_mass_kg: f64,
    carried_takeoff_fuel_kg: f64,
    policy: &FuelPolicyConfig,
    model: &dyn FuelBurnModel,
    tolerance_m: f64,
) -> RangeCorner {
    let fuel_kg = carried_takeoff_fuel_kg;
    if !takeoff_mass_kg.is_finite() || takeoff_mass_kg <= 0.0 {
        return RangeCorner::without_range(
            fuel_kg,
            RangeStatus::ModelFailed(format!("takeoff mass {takeoff_mass_kg} kg is not usable")),
        );
    }
    if !fuel_kg.is_finite() || !tolerance_m.is_finite() || tolerance_m <= 0.0 {
        return RangeCorner::without_range(
            fuel_kg,
            RangeStatus::ModelFailed("carried fuel or tolerance is not finite".to_owned()),
        );
    }
    if fuel_kg <= 0.0 {
        return RangeCorner::without_range(fuel_kg, RangeStatus::ZeroRange);
    }

    let probe = |range_m: f64| sample(policy, model, takeoff_mass_kg, fuel_kg, range_m);
    let mut search = Search {
        low_m: 0.0,
        high_m: FIRST_BRACKET_M,
        verified: None,
        unconverged: None,
    };
    loop {
        let trial_m = search.high_m;
        match search.apply(trial_m, probe(trial_m)) {
            Err(status) => return RangeCorner::without_range(fuel_kg, status),
            Ok(false) => break,
            Ok(true) => {}
        }
        if trial_m >= MAXIMUM_BRACKET_M {
            if search.verified.is_none() {
                return RangeCorner::without_range(fuel_kg, search.unverified_status());
            }
            return RangeCorner::without_range(
                fuel_kg,
                RangeStatus::ModelFailed(format!(
                    "the carried fuel still covers {MAXIMUM_BRACKET_M} m; the burn model is not usable"
                )),
            );
        }
        search.high_m = (trial_m * 2.0).min(MAXIMUM_BRACKET_M);
    }

    for _ in 0..MAXIMUM_ITERATIONS {
        if search.high_m - search.low_m <= tolerance_m {
            break;
        }
        let mid_m = 0.5 * (search.low_m + search.high_m);
        if let Err(status) = search.apply(mid_m, probe(mid_m)) {
            return RangeCorner::without_range(fuel_kg, status);
        }
    }

    match search.verified {
        Some((range_m, reserve_fuel_kg)) if range_m >= tolerance_m => RangeCorner {
            range_m,
            takeoff_fuel_kg: fuel_kg,
            reserve_fuel_kg,
            status: RangeStatus::Solved,
        },
        _ => RangeCorner::without_range(fuel_kg, search.unverified_status()),
    }
}

#[cfg(test)]
mod tests {
    // Tests assert on values they construct here, so a failed unwrap is the
    // assertion failing, not a library invariant being broken.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::breguet::{BreguetFuelModel, SegmentFractions};
    use crate::dispatch::{solve_dispatch, DispatchLimits, DispatchStatus};
    use crate::fuel_plan::LegEstimate;
    use alas_config::FuelScheme;
    use alas_units::NAUTICAL_MILE;

    fn model() -> BreguetFuelModel {
        BreguetFuelModel {
            cruise_tas_m_s: 230.0,
            cruise_density_kg_m3: 0.38,
            holding_density_kg_m3: 1.167,
            wing_area_m2: 122.6,
            cd0: 0.020,
            induced_factor_k: 0.045,
            tsfc_cruise_kg_per_n_s: 1.70e-5,
            holding_tsfc_factor: 1.0,
            takeoff_fuel_flow_kg_s: 2.3,
            idle_fuel_flow_fraction: 0.07,
            gravity_m_s2: alas_units::STANDARD_GRAVITY,
            segment_fractions: SegmentFractions::default(),
            climb_descent_range_credit_m: 250_000.0,
        }
    }

    fn policy(scheme: FuelScheme) -> FuelPolicyConfig {
        FuelPolicyConfig {
            scheme,
            ..Default::default()
        }
    }

    const TOW_KG: f64 = 73_500.0;

    #[test]
    fn range_grows_with_fuel_and_zero_fuel_gives_zero_range() {
        let policy = policy(FuelScheme::EasaBasic);
        let mut previous = 0.0;
        for fuel_kg in [8_000.0, 12_000.0, 16_000.0, 20_000.0, 24_000.0] {
            let corner = max_range_with_reserves(TOW_KG, fuel_kg, &policy, &model(), 100.0);
            assert_eq!(corner.status, RangeStatus::Solved, "{fuel_kg} kg");
            assert!(corner.range_m > previous, "{fuel_kg}: {}", corner.range_m);
            assert!(corner.reserve_fuel_kg > 0.0);
            previous = corner.range_m;
        }
        let empty = max_range_with_reserves(TOW_KG, 0.0, &policy, &model(), 100.0);
        assert_eq!((empty.range_m, empty.status), (0.0, RangeStatus::ZeroRange));
        let below_reserves = max_range_with_reserves(TOW_KG, 500.0, &policy, &model(), 100.0);
        assert_eq!(below_reserves.status, RangeStatus::ZeroRange);
    }

    #[test]
    fn reserves_shorten_the_range_of_the_same_fuel() {
        let fuel_kg = 16_000.0;
        let none = max_range_with_reserves(
            TOW_KG,
            fuel_kg,
            &policy(FuelScheme::TripFuelOnly),
            &model(),
            10.0,
        );
        let easa = max_range_with_reserves(
            TOW_KG,
            fuel_kg,
            &policy(FuelScheme::EasaBasic),
            &model(),
            10.0,
        );
        assert!(none.reserve_fuel_kg == 0.0);
        assert!(easa.range_m < none.range_m - 200_000.0, "{easa:?} {none:?}");
        let share = easa.reserve_fuel_kg / fuel_kg;
        assert!((0.05..0.5).contains(&share), "reserve share {share}");
    }

    #[test]
    fn a_dispatch_solution_round_trips_to_its_range() {
        let policy = policy(FuelScheme::EasaBasic);
        let model = model();
        let limits = DispatchLimits {
            mtow_kg: 80_000.0,
            mzfw_kg: None,
            mlw_kg: None,
            usable_capacity_kg: None,
        };
        for range_nmi in [800.0, 1_500.0, 2_200.0] {
            let range_m = range_nmi * NAUTICAL_MILE;
            let solution = solve_dispatch(55_000.0, range_m, &policy, &model, &limits, 100, 0.01);
            assert_eq!(solution.status, DispatchStatus::Converged);
            let corner = max_range_with_reserves(
                solution.takeoff_mass_kg,
                solution.plan.takeoff_fuel_kg(),
                &policy,
                &model,
                1.0,
            );
            assert_eq!(corner.status, RangeStatus::Solved);
            assert!(
                (corner.range_m - range_m).abs() < 2_000.0,
                "{range_nmi} nmi: recovered {} m of {range_m} m",
                corner.range_m
            );
        }
    }

    /// Linear burn of 3 kg per km with optional minimum range and a band of
    /// ranges where the leg fails to converge.
    struct SyntheticModel {
        minimum_range_m: f64,
        not_converged_from_m: f64,
        short_legs_unconverged: bool,
    }

    impl FuelBurnModel for SyntheticModel {
        fn trip(&self, _mass_kg: f64, range_m: f64) -> Result<LegEstimate, FuelModelError> {
            if range_m < self.minimum_range_m {
                if self.short_legs_unconverged {
                    return Err(FuelModelError::NotConverged("climb schedule".to_owned()));
                }
                return Err(FuelModelError::RouteTooShort {
                    range_m,
                    minimum_range_m: self.minimum_range_m,
                });
            }
            if range_m > self.not_converged_from_m {
                return Err(FuelModelError::NotConverged("energy deficit".to_owned()));
            }
            Ok(LegEstimate {
                fuel_kg: 3.0e-3 * range_m,
                time_s: range_m / 230.0,
            })
        }

        fn diversion(&self, _: f64, _: f64) -> Result<LegEstimate, FuelModelError> {
            Ok(LegEstimate {
                fuel_kg: 600.0,
                time_s: 2_400.0,
            })
        }

        fn holding_fuel_flow_kg_s(&self, _: f64, _: f64) -> Result<f64, FuelModelError> {
            Ok(0.6)
        }

        fn cruise_fuel_flow_kg_s(&self, _: f64) -> Result<f64, FuelModelError> {
            Ok(0.8)
        }

        fn taxi_fuel_flow_kg_s(&self) -> Result<f64, FuelModelError> {
            Ok(0.1)
        }
    }

    #[test]
    fn fuel_below_the_reserves_with_a_minimum_range_is_zero_range() {
        let model = SyntheticModel {
            minimum_range_m: 300_000.0,
            not_converged_from_m: f64::INFINITY,
            short_legs_unconverged: false,
        };
        let corner =
            max_range_with_reserves(TOW_KG, 500.0, &policy(FuelScheme::EasaBasic), &model, 100.0);
        assert_eq!(corner.status, RangeStatus::ZeroRange);
        assert_eq!((corner.range_m, corner.reserve_fuel_kg), (0.0, 0.0));
    }

    #[test]
    fn non_convergence_above_the_verified_range_is_too_far() {
        let model = SyntheticModel {
            minimum_range_m: 0.0,
            not_converged_from_m: 3_000_000.0,
            short_legs_unconverged: false,
        };
        let policy = policy(FuelScheme::EasaBasic);
        let corner = max_range_with_reserves(TOW_KG, 16_000.0, &policy, &model, 100.0);
        assert_eq!(corner.status, RangeStatus::Solved);
        assert!(
            corner.range_m <= 3_000_000.0 && corner.range_m > 2_990_000.0,
            "{}",
            corner.range_m
        );
        let plan = plan_fuel(&policy, &model, TOW_KG, corner.range_m).unwrap();
        assert_eq!(corner.reserve_fuel_kg, plan.reserve_fuel_kg());
        assert!(plan.takeoff_fuel_kg() <= 16_000.0);
    }

    #[test]
    fn non_convergence_before_any_fit_is_a_model_failure() {
        let model = SyntheticModel {
            minimum_range_m: 0.0,
            not_converged_from_m: 100_000.0,
            short_legs_unconverged: false,
        };
        let corner = max_range_with_reserves(
            TOW_KG,
            16_000.0,
            &policy(FuelScheme::EasaBasic),
            &model,
            100.0,
        );
        assert!(matches!(corner.status, RangeStatus::ModelFailed(_)));
    }

    #[test]
    fn short_legs_that_do_not_converge_do_not_hide_a_fuel_limited_range() {
        let policy = policy(FuelScheme::EasaBasic);
        let open = SyntheticModel {
            minimum_range_m: 0.0,
            not_converged_from_m: f64::INFINITY,
            short_legs_unconverged: false,
        };
        let unconverged = SyntheticModel {
            minimum_range_m: 300_000.0,
            not_converged_from_m: f64::INFINITY,
            short_legs_unconverged: true,
        };
        let a = max_range_with_reserves(TOW_KG, 16_000.0, &policy, &open, 10.0);
        let b = max_range_with_reserves(TOW_KG, 16_000.0, &policy, &unconverged, 10.0);
        assert_eq!(a, b);
        let starved = max_range_with_reserves(TOW_KG, 500.0, &policy, &unconverged, 10.0);
        assert!(matches!(starved.status, RangeStatus::ModelFailed(_)));
    }

    #[test]
    fn a_minimum_range_does_not_change_a_fuel_limited_corner() {
        let policy = policy(FuelScheme::EasaBasic);
        let open = SyntheticModel {
            minimum_range_m: 0.0,
            not_converged_from_m: f64::INFINITY,
            short_legs_unconverged: false,
        };
        let bounded = SyntheticModel {
            minimum_range_m: 300_000.0,
            not_converged_from_m: f64::INFINITY,
            short_legs_unconverged: false,
        };
        let a = max_range_with_reserves(TOW_KG, 16_000.0, &policy, &open, 10.0);
        let b = max_range_with_reserves(TOW_KG, 16_000.0, &policy, &bounded, 10.0);
        assert_eq!(a.status, RangeStatus::Solved);
        assert_eq!(a, b);
        assert!(a.range_m > 3_000_000.0 && a.range_m < 6_000_000.0);
    }

    #[test]
    fn invalid_inputs_are_reported_not_panicked() {
        let policy = policy(FuelScheme::EasaBasic);
        for (mass, fuel) in [(f64::NAN, 1.0), (-1.0, 1.0), (TOW_KG, f64::NAN)] {
            let corner = max_range_with_reserves(mass, fuel, &policy, &model(), 100.0);
            assert!(matches!(corner.status, RangeStatus::ModelFailed(_)));
        }
    }
}
