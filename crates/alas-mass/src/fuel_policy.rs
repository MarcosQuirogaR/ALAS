// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Turning a [`FuelPolicyConfig`] scheme into a priced [`FuelPlan`].
//!
//! Every scheme decomposes into the same eight named quantities (ICAO Annex
//! 6 Part I, 4.3.6.3), and this module differs only in which rule prices
//! each one:
//!
//! - **EASA basic** (CAT.OP.MPA.181(c), AMC1 CAT.OP.MPA.181): contingency is
//!   the larger of five percent of trip fuel and a five-minute hold at the
//!   holding altitude above the destination; the destination alternate is a
//!   missed-approach-and-diversion leg, or, when no alternate is carried, a
//!   fifteen-minute hold at the destination (CAT.OP.MPA.181(c)(4)(ii)); the
//!   final reserve is a thirty-minute hold evaluated at the estimated mass
//!   on arrival at the alternate, or at the destination when there is none
//!   (CAT.OP.MPA.181(c)(5), explicit on that mass basis).
//! - **FAA domestic** (14 CFR 121.639): no contingency quantity; the final
//!   reserve is forty-five minutes at normal cruise fuel consumption rather
//!   than a hold.
//! - **FAA flag and supplemental, turbine** (14 CFR 121.645(b)): contingency
//!   is ten percent of the flight time to the destination; the final
//!   reserve is a thirty-minute hold at the alternate, as in the EASA basic
//!   scheme.
//! - **Study convention**: the EASA basic numbers, read verbatim from the
//!   policy, but without the five-minute contingency floor unless the
//!   policy states a positive one. This carries no regulatory standing; it
//!   is the FAST-OAD/CeRAS-style convention conceptual-design tools use.
//! - **Trip fuel only**: taxi and trip fuel, nothing else. The frozen
//!   behaviour of the earlier maximum-available-fuel mission, kept for
//!   comparison rather than for design use.
//!
//! # The nested fixed point
//!
//! The final reserve is evaluated at a mass that includes fuel carried
//! upstream of it (contingency and the alternate), which in turn depend on
//! trip fuel, which depends on the takeoff mass this same plan's fuel adds
//! up to. [`plan_fuel`] does not iterate that: it evaluates every landing
//! mass once, in the order the rule names them (destination, then
//! alternate/reserve), from the `takeoff_mass_kg` it is given. Closing the
//! outer loop (finding the takeoff mass whose plan reproduces it) is
//! [`crate::dispatch::solve_dispatch`]'s job, and it calls this function
//! once per iteration.

use alas_config::{FuelPolicyConfig, FuelScheme};
use alas_units::{FOOT, NAUTICAL_MILE};

use crate::fuel_plan::{FuelBurnModel, FuelModelError, FuelPlan, FuelQuantity, FuelRule};

/// Seconds in a minute, spelled out because every duration in a
/// [`FuelPolicyConfig`] is minutes and every burn-model rate is per second.
const SECONDS_PER_MINUTE: f64 = 60.0;

/// The no-alternate holding allowance, CAT.OP.MPA.181(c)(4)(ii): fifteen
/// minutes at holding speed at the holding altitude above the destination,
/// carried instead of a diversion when no destination alternate is
/// required.
const NO_ALTERNATE_HOLD_MIN: f64 = 15.0;

/// The FAA flag/supplemental no-alternate allowance, 14 CFR 121.645(c): a
/// release to an airport with no alternate carries fuel for two hours at
/// normal cruising fuel consumption after reaching it.
const NO_ALTERNATE_FLAG_CRUISE_MIN: f64 = 120.0;

/// Fuel to hold for `minutes` at `mass_kg` and `altitude_m`.
///
/// A thin wrapper over [`FuelBurnModel::holding_fuel_flow_kg_s`]: every
/// holding quantity in every scheme (the EASA contingency floor, the
/// no-alternate allowance, the final reserve) is this same integration at a
/// different mass and duration.
pub fn holding_fuel_kg(
    model: &dyn FuelBurnModel,
    mass_kg: f64,
    altitude_m: f64,
    minutes: f64,
) -> Result<f64, FuelModelError> {
    let flow_kg_s = model.holding_fuel_flow_kg_s(mass_kg, altitude_m)?;
    Ok(flow_kg_s * minutes * SECONDS_PER_MINUTE)
}

/// Price a full fuel plan for one mission, under one policy, at one
/// candidate takeoff mass.
///
/// `takeoff_mass_kg` is a candidate, not a solution: the trip, contingency,
/// alternate and final reserve are all priced from it and from the landing
/// masses it implies, but nothing here checks that the resulting plan's
/// takeoff fuel actually sums back to `takeoff_mass_kg` minus the airframe
/// and payload. [`crate::dispatch::solve_dispatch`] is where that check, and
/// the iteration that satisfies it, live.
///
/// # Errors
///
/// [`FuelModelError::MassOutOfRange`] if `takeoff_mass_kg` or a landing mass
/// derived from it is not finite and positive; [`FuelModelError::InvalidDistance`]
/// if `range_m` is not finite and nonnegative; whatever `model` itself
/// returns for a leg or a fuel flow it cannot price.
pub fn plan_fuel(
    policy: &FuelPolicyConfig,
    model: &dyn FuelBurnModel,
    takeoff_mass_kg: f64,
    range_m: f64,
) -> Result<FuelPlan, FuelModelError> {
    let takeoff_mass_kg = checked_mass(takeoff_mass_kg)?;
    if !range_m.is_finite() || range_m < 0.0 {
        return Err(FuelModelError::InvalidDistance {
            distance_m: range_m,
        });
    }

    let holding_altitude_m = policy.holding_altitude_ft * FOOT;
    let alternate_distance_m = policy.alternate_distance_nmi * NAUTICAL_MILE;

    let taxi = FuelQuantity {
        kg: model.taxi_fuel_flow_kg_s()? * policy.taxi_time_min * SECONDS_PER_MINUTE,
        rule: FuelRule::TaxiTime {
            minutes: policy.taxi_time_min,
        },
    };

    let trip_leg = model.trip(takeoff_mass_kg, range_m)?;
    let trip = FuelQuantity {
        kg: trip_leg.fuel_kg,
        rule: FuelRule::TripBurn,
    };
    let destination_landing_mass_kg = checked_mass(takeoff_mass_kg - trip.kg)?;

    let reserves = match policy.scheme {
        FuelScheme::EasaBasic => easa_style_reserves(
            policy,
            model,
            trip.kg,
            destination_landing_mass_kg,
            holding_altitude_m,
            alternate_distance_m,
            true,
        )?,
        FuelScheme::StudyConvention => easa_style_reserves(
            policy,
            model,
            trip.kg,
            destination_landing_mass_kg,
            holding_altitude_m,
            alternate_distance_m,
            policy.contingency_minimum_hold_min > 0.0,
        )?,
        FuelScheme::FaaDomestic => faa_domestic_reserves(
            policy,
            model,
            destination_landing_mass_kg,
            alternate_distance_m,
        )?,
        FuelScheme::FaaFlagSupplemental => faa_flag_reserves(
            policy,
            model,
            trip_leg.time_s,
            destination_landing_mass_kg,
            holding_altitude_m,
            alternate_distance_m,
        )?,
        FuelScheme::TripFuelOnly => ReserveQuantities {
            contingency: FuelQuantity::NONE,
            alternate: FuelQuantity::NONE,
            final_reserve: FuelQuantity::NONE,
            additional: FuelQuantity::NONE,
            extra: FuelQuantity::NONE,
            reserve_landing_mass_kg: destination_landing_mass_kg,
        },
    };

    Ok(FuelPlan {
        scheme: policy.scheme,
        taxi,
        trip,
        contingency: reserves.contingency,
        alternate: reserves.alternate,
        final_reserve: reserves.final_reserve,
        additional: reserves.additional,
        extra: reserves.extra,
        trip_time_s: trip_leg.time_s,
        destination_landing_mass_kg,
        reserve_landing_mass_kg: reserves.reserve_landing_mass_kg,
    })
}

/// The reserve-side quantities a scheme populates, plus the mass the
/// aircraft is estimated to land at with only the final reserve left: the
/// alternate for schemes that carry one, the destination otherwise.
struct ReserveQuantities {
    contingency: FuelQuantity,
    alternate: FuelQuantity,
    final_reserve: FuelQuantity,
    additional: FuelQuantity,
    extra: FuelQuantity,
    reserve_landing_mass_kg: f64,
}

/// Additional and extra fuel are read verbatim from the policy under every
/// scheme that carries them: neither is computed from the mission.
fn declared_additional_and_extra(policy: &FuelPolicyConfig) -> (FuelQuantity, FuelQuantity) {
    (
        FuelQuantity {
            kg: policy.additional_fuel_kg,
            rule: FuelRule::Declared,
        },
        FuelQuantity {
            kg: policy.extra_fuel_kg,
            rule: FuelRule::Declared,
        },
    )
}

/// EASA basic (CAT.OP.MPA.181(c), AMC1) and the study convention that reuses
/// its structure.
///
/// `apply_hold_floor` selects whether contingency is floored at all: EASA
/// basic always evaluates the floor (a zero-minute floor simply loses the
/// comparison to the trip-fuel fraction), while the study convention skips
/// the holding-flow call entirely unless the policy states a positive floor,
/// per the scheme's own documentation.
#[allow(clippy::too_many_arguments)]
fn easa_style_reserves(
    policy: &FuelPolicyConfig,
    model: &dyn FuelBurnModel,
    trip_kg: f64,
    destination_landing_mass_kg: f64,
    holding_altitude_m: f64,
    alternate_distance_m: f64,
    apply_hold_floor: bool,
) -> Result<ReserveQuantities, FuelModelError> {
    let contingency_by_fraction_kg = policy.contingency_trip_fraction * trip_kg;
    let contingency = if apply_hold_floor {
        let contingency_hold_kg = holding_fuel_kg(
            model,
            destination_landing_mass_kg,
            holding_altitude_m,
            policy.contingency_minimum_hold_min,
        )?;
        if contingency_by_fraction_kg >= contingency_hold_kg {
            FuelQuantity {
                kg: contingency_by_fraction_kg,
                rule: FuelRule::TripFraction {
                    fraction: policy.contingency_trip_fraction,
                },
            }
        } else {
            FuelQuantity {
                kg: contingency_hold_kg,
                rule: FuelRule::Holding {
                    minutes: policy.contingency_minimum_hold_min,
                },
            }
        }
    } else {
        FuelQuantity {
            kg: contingency_by_fraction_kg,
            rule: FuelRule::TripFraction {
                fraction: policy.contingency_trip_fraction,
            },
        }
    };

    let alternate = if policy.alternate_distance_nmi > 0.0 {
        let leg = model.diversion(destination_landing_mass_kg, alternate_distance_m)?;
        FuelQuantity {
            kg: leg.fuel_kg,
            rule: FuelRule::Diversion {
                distance_m: alternate_distance_m,
            },
        }
    } else {
        let kg = holding_fuel_kg(
            model,
            destination_landing_mass_kg,
            holding_altitude_m,
            NO_ALTERNATE_HOLD_MIN,
        )?;
        FuelQuantity {
            kg,
            rule: FuelRule::Holding {
                minutes: NO_ALTERNATE_HOLD_MIN,
            },
        }
    };

    let reserve_landing_mass_kg = checked_mass(destination_landing_mass_kg - alternate.kg)?;
    let final_reserve_kg = holding_fuel_kg(
        model,
        reserve_landing_mass_kg,
        holding_altitude_m,
        policy.final_reserve_hold_min,
    )?;
    let final_reserve = FuelQuantity {
        kg: final_reserve_kg,
        rule: FuelRule::Holding {
            minutes: policy.final_reserve_hold_min,
        },
    };

    let (additional, extra) = declared_additional_and_extra(policy);
    Ok(ReserveQuantities {
        contingency,
        alternate,
        final_reserve,
        additional,
        extra,
        reserve_landing_mass_kg,
    })
}

/// FAA domestic operations, 14 CFR 121.639: no contingency quantity, and a
/// forty-five-minute reserve at normal cruise fuel consumption rather than a
/// hold.
fn faa_domestic_reserves(
    policy: &FuelPolicyConfig,
    model: &dyn FuelBurnModel,
    destination_landing_mass_kg: f64,
    alternate_distance_m: f64,
) -> Result<ReserveQuantities, FuelModelError> {
    let alternate = if policy.alternate_distance_nmi > 0.0 {
        let leg = model.diversion(destination_landing_mass_kg, alternate_distance_m)?;
        FuelQuantity {
            kg: leg.fuel_kg,
            rule: FuelRule::Diversion {
                distance_m: alternate_distance_m,
            },
        }
    } else {
        FuelQuantity::NONE
    };

    let reserve_landing_mass_kg = checked_mass(destination_landing_mass_kg - alternate.kg)?;
    let final_reserve_kg = model.cruise_fuel_flow_kg_s(reserve_landing_mass_kg)?
        * policy.domestic_cruise_reserve_min
        * SECONDS_PER_MINUTE;
    let final_reserve = FuelQuantity {
        kg: final_reserve_kg,
        rule: FuelRule::CruiseTime {
            minutes: policy.domestic_cruise_reserve_min,
        },
    };

    let (additional, extra) = declared_additional_and_extra(policy);
    Ok(ReserveQuantities {
        contingency: FuelQuantity::NONE,
        alternate,
        final_reserve,
        additional,
        extra,
        reserve_landing_mass_kg,
    })
}

/// FAA flag and supplemental turbine operations, 14 CFR 121.645(b).
///
/// The rule states the contingency in time, not mass: fuel "to fly for a
/// period of 10 percent of the total time required to fly from the airport
/// of departure to, and land at, the airport to which it was released"
/// (121.645(b)(2)). It is priced at normal cruise consumption at the
/// destination landing mass, which is where that time would be flown; a
/// share of trip fuel would over-count on a climb-heavy sector. With no
/// alternate the release must carry two hours at normal cruise consumption
/// instead (121.645(c)). The final reserve is a thirty-minute hold at
/// 1,500 ft above the alternate, or the destination without one
/// (121.645(b)(4)).
fn faa_flag_reserves(
    policy: &FuelPolicyConfig,
    model: &dyn FuelBurnModel,
    trip_time_s: f64,
    destination_landing_mass_kg: f64,
    holding_altitude_m: f64,
    alternate_distance_m: f64,
) -> Result<ReserveQuantities, FuelModelError> {
    let destination_cruise_flow_kg_s = model.cruise_fuel_flow_kg_s(destination_landing_mass_kg)?;
    let contingency = FuelQuantity {
        kg: policy.flag_flight_time_fraction * trip_time_s * destination_cruise_flow_kg_s,
        rule: FuelRule::FlightTimeFraction {
            fraction: policy.flag_flight_time_fraction,
        },
    };

    let alternate = if policy.alternate_distance_nmi > 0.0 {
        let leg = model.diversion(destination_landing_mass_kg, alternate_distance_m)?;
        FuelQuantity {
            kg: leg.fuel_kg,
            rule: FuelRule::Diversion {
                distance_m: alternate_distance_m,
            },
        }
    } else {
        FuelQuantity {
            kg: destination_cruise_flow_kg_s * NO_ALTERNATE_FLAG_CRUISE_MIN * SECONDS_PER_MINUTE,
            rule: FuelRule::CruiseTime {
                minutes: NO_ALTERNATE_FLAG_CRUISE_MIN,
            },
        }
    };

    let reserve_landing_mass_kg = checked_mass(destination_landing_mass_kg - alternate.kg)?;
    let final_reserve_kg = holding_fuel_kg(
        model,
        reserve_landing_mass_kg,
        holding_altitude_m,
        policy.final_reserve_hold_min,
    )?;
    let final_reserve = FuelQuantity {
        kg: final_reserve_kg,
        rule: FuelRule::Holding {
            minutes: policy.final_reserve_hold_min,
        },
    };

    let (additional, extra) = declared_additional_and_extra(policy);
    Ok(ReserveQuantities {
        contingency,
        alternate,
        final_reserve,
        additional,
        extra,
        reserve_landing_mass_kg,
    })
}

/// A mass computed partway through a plan, checked before it is fed into the
/// next quantity: a plan that silently carried a negative or infinite
/// landing mass forward would price everything after it from garbage.
fn checked_mass(mass_kg: f64) -> Result<f64, FuelModelError> {
    if mass_kg.is_finite() && mass_kg > 0.0 {
        Ok(mass_kg)
    } else {
        Err(FuelModelError::MassOutOfRange { mass_kg })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuel_plan::LegEstimate;
    use alas_testkit::{agrees, Tier};

    /// A linear burn model with no aerodynamics: every quantity is a fixed
    /// share of the mass it starts from. Its purpose is to make the
    /// arithmetic [`plan_fuel`] performs checkable by hand, not to be
    /// physically representative.
    struct ToyModel {
        trip_fraction: f64,
        diversion_fraction: f64,
        holding_flow_fraction: f64,
        cruise_flow_fraction: f64,
        taxi_flow_kg_s: f64,
        cruise_speed_m_s: f64,
    }

    impl FuelBurnModel for ToyModel {
        fn trip(&self, takeoff_mass_kg: f64, range_m: f64) -> Result<LegEstimate, FuelModelError> {
            Ok(LegEstimate {
                fuel_kg: self.trip_fraction * takeoff_mass_kg,
                time_s: range_m / self.cruise_speed_m_s,
            })
        }

        fn diversion(
            &self,
            start_mass_kg: f64,
            distance_m: f64,
        ) -> Result<LegEstimate, FuelModelError> {
            Ok(LegEstimate {
                fuel_kg: self.diversion_fraction * start_mass_kg,
                time_s: distance_m / self.cruise_speed_m_s,
            })
        }

        fn holding_fuel_flow_kg_s(
            &self,
            mass_kg: f64,
            _altitude_m: f64,
        ) -> Result<f64, FuelModelError> {
            Ok(self.holding_flow_fraction * mass_kg)
        }

        fn cruise_fuel_flow_kg_s(&self, mass_kg: f64) -> Result<f64, FuelModelError> {
            Ok(self.cruise_flow_fraction * mass_kg)
        }

        fn taxi_fuel_flow_kg_s(&self) -> Result<f64, FuelModelError> {
            Ok(self.taxi_flow_kg_s)
        }
    }

    fn toy_model() -> ToyModel {
        ToyModel {
            trip_fraction: 0.10,
            diversion_fraction: 0.02,
            holding_flow_fraction: 0.00001,
            cruise_flow_fraction: 0.00003,
            taxi_flow_kg_s: 0.05,
            cruise_speed_m_s: 200.0,
        }
    }

    const TOW_KG: f64 = 100_000.0;
    const RANGE_M: f64 = 1_000_000.0;

    #[test]
    fn the_closure_identities_hold_on_a_priced_plan() {
        let policy = FuelPolicyConfig::default();
        let model = toy_model();
        let plan = plan_fuel(&policy, &model, TOW_KG, RANGE_M).unwrap();

        assert!(agrees(
            plan.trip.kg,
            model.trip_fraction * TOW_KG,
            Tier::Closed
        ));
        assert!(agrees(
            plan.takeoff_fuel_kg(),
            plan.trip.kg
                + plan.contingency.kg
                + plan.alternate.kg
                + plan.final_reserve.kg
                + plan.additional.kg
                + plan.extra.kg,
            Tier::Closed
        ));
        assert!(agrees(
            plan.ramp_fuel_kg(),
            plan.takeoff_fuel_kg() + plan.taxi.kg,
            Tier::Closed
        ));
        assert!(agrees(
            plan.block_fuel_kg(),
            plan.taxi.kg + plan.trip.kg,
            Tier::Closed
        ));
        assert!(agrees(
            plan.destination_landing_fuel_kg(),
            plan.takeoff_fuel_kg() - plan.trip.kg,
            Tier::Closed
        ));
        assert!(agrees(
            plan.reserve_fuel_kg(),
            plan.contingency.kg + plan.alternate.kg + plan.final_reserve.kg + plan.additional.kg,
            Tier::Closed
        ));
        assert!(plan.is_finite_and_nonnegative());
    }

    #[test]
    fn easa_basic_contingency_is_the_trip_fraction_when_it_exceeds_the_hold() {
        let policy = FuelPolicyConfig::default(); // 5% fraction, 5-minute floor
        let model = toy_model();
        let plan = plan_fuel(&policy, &model, TOW_KG, RANGE_M).unwrap();

        let expected_hold_kg = holding_fuel_kg(
            &model,
            plan.destination_landing_mass_kg,
            policy.holding_altitude_ft * FOOT,
            policy.contingency_minimum_hold_min,
        )
        .unwrap();
        let expected_fraction_kg = policy.contingency_trip_fraction * plan.trip.kg;
        assert!(expected_fraction_kg > expected_hold_kg);
        assert_eq!(
            plan.contingency.rule,
            FuelRule::TripFraction {
                fraction: policy.contingency_trip_fraction
            }
        );
        assert!(agrees(
            plan.contingency.kg,
            expected_fraction_kg,
            Tier::Closed
        ));
    }

    #[test]
    fn easa_basic_contingency_is_the_hold_when_the_trip_fraction_does_not_reach_it() {
        let policy = FuelPolicyConfig {
            contingency_trip_fraction: 0.002, // far below the 5-minute hold
            ..Default::default()
        };
        let model = toy_model();
        let plan = plan_fuel(&policy, &model, TOW_KG, RANGE_M).unwrap();

        let expected_hold_kg = holding_fuel_kg(
            &model,
            plan.destination_landing_mass_kg,
            policy.holding_altitude_ft * FOOT,
            policy.contingency_minimum_hold_min,
        )
        .unwrap();
        assert_eq!(
            plan.contingency.rule,
            FuelRule::Holding {
                minutes: policy.contingency_minimum_hold_min
            }
        );
        assert!(agrees(plan.contingency.kg, expected_hold_kg, Tier::Closed));
    }

    #[test]
    fn easa_basic_with_no_alternate_carries_the_fifteen_minute_hold() {
        let policy = FuelPolicyConfig {
            alternate_distance_nmi: 0.0,
            ..Default::default()
        };
        let model = toy_model();
        let plan = plan_fuel(&policy, &model, TOW_KG, RANGE_M).unwrap();

        assert_eq!(
            plan.alternate.rule,
            FuelRule::Holding {
                minutes: NO_ALTERNATE_HOLD_MIN
            }
        );
        let expected_kg = holding_fuel_kg(
            &model,
            plan.destination_landing_mass_kg,
            policy.holding_altitude_ft * FOOT,
            NO_ALTERNATE_HOLD_MIN,
        )
        .unwrap();
        assert!(agrees(plan.alternate.kg, expected_kg, Tier::Closed));
    }

    #[test]
    fn easa_basic_final_reserve_is_evaluated_at_the_reserve_landing_mass() {
        let policy = FuelPolicyConfig::default();
        let model = toy_model();
        let plan = plan_fuel(&policy, &model, TOW_KG, RANGE_M).unwrap();

        let expected_reserve_landing_mass_kg = plan.destination_landing_mass_kg - plan.alternate.kg;
        assert!(agrees(
            plan.reserve_landing_mass_kg,
            expected_reserve_landing_mass_kg,
            Tier::Closed
        ));
        let expected_kg = holding_fuel_kg(
            &model,
            expected_reserve_landing_mass_kg,
            policy.holding_altitude_ft * FOOT,
            policy.final_reserve_hold_min,
        )
        .unwrap();
        assert!(agrees(plan.final_reserve.kg, expected_kg, Tier::Closed));
    }

    #[test]
    fn faa_domestic_has_no_contingency_and_a_cruise_time_reserve() {
        let policy = FuelPolicyConfig {
            scheme: FuelScheme::FaaDomestic,
            ..Default::default()
        };
        let model = toy_model();
        let plan = plan_fuel(&policy, &model, TOW_KG, RANGE_M).unwrap();

        assert_eq!(plan.contingency, FuelQuantity::NONE);
        assert_eq!(
            plan.final_reserve.rule,
            FuelRule::CruiseTime {
                minutes: policy.domestic_cruise_reserve_min
            }
        );
        let expected_kg = model
            .cruise_fuel_flow_kg_s(plan.reserve_landing_mass_kg)
            .unwrap()
            * policy.domestic_cruise_reserve_min
            * SECONDS_PER_MINUTE;
        assert!(agrees(plan.final_reserve.kg, expected_kg, Tier::Closed));
    }

    #[test]
    fn faa_flag_supplemental_contingency_is_a_flight_time_fraction_at_cruise_flow() {
        let policy = FuelPolicyConfig {
            scheme: FuelScheme::FaaFlagSupplemental,
            ..Default::default()
        };
        let model = toy_model();
        let plan = plan_fuel(&policy, &model, TOW_KG, RANGE_M).unwrap();

        assert_eq!(
            plan.contingency.rule,
            FuelRule::FlightTimeFraction {
                fraction: policy.flag_flight_time_fraction
            }
        );
        // 121.645(b)(2) is written in flight time, priced at cruise
        // consumption at the mass the aircraft would fly that time at.
        let cruise_flow_kg_s = model
            .cruise_fuel_flow_kg_s(plan.destination_landing_mass_kg)
            .unwrap();
        assert!(agrees(
            plan.contingency.kg,
            policy.flag_flight_time_fraction * plan.trip_time_s * cruise_flow_kg_s,
            Tier::Closed
        ));
        assert_eq!(
            plan.alternate.rule,
            FuelRule::Diversion {
                distance_m: policy.alternate_distance_nmi * alas_units::NAUTICAL_MILE
            }
        );
    }

    #[test]
    fn faa_flag_supplemental_without_an_alternate_carries_two_hours_of_cruise() {
        let policy = FuelPolicyConfig {
            scheme: FuelScheme::FaaFlagSupplemental,
            alternate_distance_nmi: 0.0,
            ..Default::default()
        };
        let model = toy_model();
        let plan = plan_fuel(&policy, &model, TOW_KG, RANGE_M).unwrap();

        // 121.645(c): a release with no alternate carries two hours at
        // normal cruise consumption in place of the diversion.
        assert_eq!(
            plan.alternate.rule,
            FuelRule::CruiseTime {
                minutes: NO_ALTERNATE_FLAG_CRUISE_MIN
            }
        );
        let cruise_flow_kg_s = model
            .cruise_fuel_flow_kg_s(plan.destination_landing_mass_kg)
            .unwrap();
        assert!(agrees(
            plan.alternate.kg,
            cruise_flow_kg_s * 120.0 * SECONDS_PER_MINUTE,
            Tier::Closed
        ));
    }

    #[test]
    fn trip_fuel_only_carries_no_reserves() {
        let policy = FuelPolicyConfig {
            scheme: FuelScheme::TripFuelOnly,
            additional_fuel_kg: 500.0, // must still be ignored by this scheme
            extra_fuel_kg: 500.0,
            ..Default::default()
        };
        let model = toy_model();
        let plan = plan_fuel(&policy, &model, TOW_KG, RANGE_M).unwrap();

        assert_eq!(plan.contingency, FuelQuantity::NONE);
        assert_eq!(plan.alternate, FuelQuantity::NONE);
        assert_eq!(plan.final_reserve, FuelQuantity::NONE);
        assert_eq!(plan.additional, FuelQuantity::NONE);
        assert_eq!(plan.extra, FuelQuantity::NONE);
        assert!(plan.trip.kg > 0.0);
        assert!(plan.taxi.kg > 0.0);
        assert_eq!(
            plan.reserve_landing_mass_kg,
            plan.destination_landing_mass_kg
        );
    }

    #[test]
    fn an_invalid_takeoff_mass_or_range_is_reported_and_not_computed() {
        let policy = FuelPolicyConfig::default();
        let model = toy_model();
        assert!(matches!(
            plan_fuel(&policy, &model, f64::NAN, RANGE_M),
            Err(FuelModelError::MassOutOfRange { .. })
        ));
        assert!(matches!(
            plan_fuel(&policy, &model, TOW_KG, -1.0),
            Err(FuelModelError::InvalidDistance { .. })
        ));
    }
}
