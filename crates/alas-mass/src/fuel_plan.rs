// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The fuel a mission carries, quantity by quantity, and the physics that
//! prices each one.
//!
//! Every operating rule decomposes the fuel on board the same way: taxi,
//! trip, contingency, destination alternate, final reserve, additional and
//! extra fuel, and differs only in how each part is computed. [`FuelPlan`]
//! holds the decomposition with the rule that produced every kilogram, so a
//! plan reads as evidence rather than as a total. [`FuelBurnModel`] is the
//! seam the policy evaluation reaches the aircraft through: a trip over a
//! distance, a diversion, and the fuel flows while holding, cruising and
//! taxiing. The analytic model the design search uses and the native
//! segment mission the pipeline flies both implement it, so the same policy
//! code sizes a candidate in the loop and checks the finalist afterwards.
//!
//! Masses are kilograms throughout. Volume enters only where a tank is
//! filled, with an explicit density.

use std::fmt;

use alas_config::FuelScheme;

/// Taxi-in ground time after landing, minutes, at ground idle (ICAO 7 %
/// thrust). ICAO Annex 16 Vol. II fixes the LTO taxi/ground-idle mode at
/// 26 min; ICAO Doc 9889, Airport Air Quality Manual (2011), splits that
/// reference time into 19 min taxi-out and 7 min taxi-in. The landing
/// ground roll after touchdown is flown at idle and is counted inside this
/// ground time; reverse thrust, which the LTO cycle excludes, is not
/// modelled. Taxi-out stays the policy's operator time.
pub const TAXI_IN_TIME_MIN: f64 = 7.0;

/// The rule that produced one quantity of a plan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FuelRule {
    /// Idle fuel flow for a ground time.
    TaxiTime {
        /// Taxi-out ground time priced into the quantity, minutes.
        minutes: f64,
        /// Ground-idle fuel flow with every engine running, kg/s. It also
        /// prices the taxi-in after landing ([`FuelPlan::taxi_in_fuel_kg`]).
        idle_fuel_flow_kg_s: f64,
    },
    /// The integrated burn of the design mission from takeoff to landing.
    TripBurn,
    /// A share of the trip fuel.
    TripFraction {
        /// The share.
        fraction: f64,
    },
    /// Holding at the policy altitude at the estimated landing mass.
    Holding {
        /// Holding time, minutes.
        minutes: f64,
    },
    /// A missed approach followed by a diversion of the given distance.
    Diversion {
        /// Still-air distance to the alternate, m.
        distance_m: f64,
    },
    /// Normal cruise fuel consumption for a time.
    CruiseTime {
        /// Cruise time, minutes.
        minutes: f64,
    },
    /// A share of the flight time to the destination.
    FlightTimeFraction {
        /// The share.
        fraction: f64,
    },
    /// A quantity declared in the policy.
    Declared,
    /// The scheme does not carry this quantity.
    NotCarried,
}

/// One named quantity of the plan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FuelQuantity {
    /// Mass, kg.
    pub kg: f64,
    /// The rule that produced it.
    pub rule: FuelRule,
}

impl FuelQuantity {
    /// A quantity the scheme does not carry.
    pub const NONE: Self = Self {
        kg: 0.0,
        rule: FuelRule::NotCarried,
    };
}

/// The fuel decomposition of one mission under one scheme.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FuelPlan {
    /// The scheme the plan was computed under.
    pub scheme: FuelScheme,
    /// Fuel burned before takeoff.
    pub taxi: FuelQuantity,
    /// Fuel burned from takeoff to landing at the destination.
    pub trip: FuelQuantity,
    /// Fuel carried against unforeseen deviations from the plan.
    pub contingency: FuelQuantity,
    /// Fuel to divert to and land at the alternate.
    pub alternate: FuelQuantity,
    /// Fuel that must remain on landing at the last aerodrome.
    pub final_reserve: FuelQuantity,
    /// Fuel for the specific cases the general reserves do not cover.
    pub additional: FuelQuantity,
    /// Extra and discretionary fuel.
    pub extra: FuelQuantity,
    /// Flight time from takeoff to landing at the destination, s.
    pub trip_time_s: f64,
    /// Estimated landing mass at the destination, kg.
    pub destination_landing_mass_kg: f64,
    /// Estimated landing mass at the alternate, or at the destination when
    /// no alternate is carried, kg.
    pub reserve_landing_mass_kg: f64,
}

impl FuelPlan {
    /// Fuel on board at brake release: everything loaded but the taxi-out
    /// fuel, so trip, every reserve, extra fuel and the taxi-in budget
    /// ([`Self::taxi_in_fuel_kg`]), which is carried through the flight and
    /// burned only after landing. The dispatch closure's takeoff mass is the
    /// zero-fuel mass plus this.
    pub fn takeoff_fuel_kg(&self) -> f64 {
        self.trip.kg
            + self.contingency.kg
            + self.alternate.kg
            + self.final_reserve.kg
            + self.additional.kg
            + self.extra.kg
            + self.taxi_in_fuel_kg()
    }

    /// Fuel loaded at the ramp: taxi-out fuel and takeoff fuel, which holds
    /// the taxi-in budget. It bounds [`Self::block_fuel_kg`] under every
    /// scheme, because fuel burned must have been loaded.
    pub fn ramp_fuel_kg(&self) -> f64 {
        self.takeoff_fuel_kg() + self.taxi.kg
    }

    /// Fuel burned taxiing in after landing at the destination: the
    /// ground-idle flow of the taxi quantity for [`TAXI_IN_TIME_MIN`]. It is
    /// part of block fuel and is carried in the takeoff fuel under every
    /// scheme. No operating rule plans it as a separate quantity (ICAO Annex
    /// 6 Part I 4.3.6.3 taxi fuel is the fuel before takeoff), but fuel
    /// burned must have been loaded, and it is on board from brake release
    /// to the destination. It leaves every reserve unchanged and is part of
    /// the fuel remaining on landing, beside the reserves.
    pub fn taxi_in_fuel_kg(&self) -> f64 {
        match self.taxi.rule {
            FuelRule::TaxiTime {
                idle_fuel_flow_kg_s,
                ..
            } => idle_fuel_flow_kg_s * TAXI_IN_TIME_MIN * 60.0,
            _ => 0.0,
        }
    }

    /// Fuel consumed by the nominal flight, gate to gate: taxi-out, trip and
    /// taxi-in.
    pub fn block_fuel_kg(&self) -> f64 {
        self.taxi.kg + self.trip.kg + self.taxi_in_fuel_kg()
    }

    /// Fuel remaining on landing at the destination in the nominal flight:
    /// the reserves, extra fuel and the taxi-in budget.
    pub fn destination_landing_fuel_kg(&self) -> f64 {
        self.takeoff_fuel_kg() - self.trip.kg
    }

    /// Fuel the plan protects beyond the nominal trip.
    pub fn reserve_fuel_kg(&self) -> f64 {
        self.contingency.kg + self.alternate.kg + self.final_reserve.kg + self.additional.kg
    }

    /// Whether every quantity is finite and nonnegative.
    pub fn is_finite_and_nonnegative(&self) -> bool {
        [
            self.taxi,
            self.trip,
            self.contingency,
            self.alternate,
            self.final_reserve,
            self.additional,
            self.extra,
        ]
        .iter()
        .all(|quantity| quantity.kg.is_finite() && quantity.kg >= 0.0)
            && self.trip_time_s.is_finite()
            && self.destination_landing_mass_kg.is_finite()
            && self.reserve_landing_mass_kg.is_finite()
    }
}

/// Fuel and time of one flown leg.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LegEstimate {
    /// Fuel burned over the leg, kg.
    pub fuel_kg: f64,
    /// Elapsed time, s.
    pub time_s: f64,
}

/// Why a burn model could not price a quantity.
#[derive(Debug, Clone, PartialEq)]
pub enum FuelModelError {
    /// Work stopped by the caller; no physical feasibility conclusion exists.
    Cancelled,
    /// The mass asked for lies outside the model's validity.
    MassOutOfRange {
        /// The mass, kg.
        mass_kg: f64,
    },
    /// The distance asked for is not finite and nonnegative.
    InvalidDistance {
        /// The distance, m.
        distance_m: f64,
    },
    /// The model's own inputs are not usable.
    InvalidModel(String),
    /// The requested still-air distance is shorter than the climb and
    /// descent footprint of the configured profile at its lowest usable
    /// cruise altitude, so no vertical profile can fly it without overflying
    /// the route.
    RouteTooShort {
        /// The requested distance, m.
        range_m: f64,
        /// The smallest distance the profile can fly, m.
        minimum_range_m: f64,
    },
    /// The leg could not be flown to completion.
    NotConverged(String),
}

impl fmt::Display for FuelModelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => write!(formatter, "fuel evaluation cancelled"),
            Self::MassOutOfRange { mass_kg } => {
                write!(
                    formatter,
                    "mass {mass_kg} kg is outside the burn model's range"
                )
            }
            Self::InvalidDistance { distance_m } => {
                write!(
                    formatter,
                    "distance {distance_m} m is not finite and nonnegative"
                )
            }
            Self::InvalidModel(reason) => write!(formatter, "burn model is invalid: {reason}"),
            Self::NotConverged(reason) => write!(formatter, "leg did not converge: {reason}"),
            Self::RouteTooShort {
                range_m,
                minimum_range_m,
            } => write!(
                formatter,
                "route of {range_m} m is shorter than the {minimum_range_m} m climb/descent footprint"
            ),
        }
    }
}

impl std::error::Error for FuelModelError {}

/// The aircraft physics a fuel policy is evaluated against.
///
/// Every method takes the mass at the start of what it prices, because fuel
/// flow at a given speed depends on the lift the aircraft has to make.
pub trait FuelBurnModel {
    /// Opt in only when identical inputs give identical answers for this fixed
    /// model throughout a dispatch solve. Mutable/stochastic models keep false.
    fn deterministic_for_dispatch(&self) -> bool {
        false
    }
    /// Checked before dispatch evaluations, including exact memoized hits.
    fn check_cancellation(&self) -> Result<(), FuelModelError> {
        Ok(())
    }

    /// Fuel and time from takeoff to landing over `range_m`, starting at
    /// `takeoff_mass_kg`.
    fn trip(&self, takeoff_mass_kg: f64, range_m: f64) -> Result<LegEstimate, FuelModelError>;

    /// Fuel and time of a missed approach followed by a diversion over
    /// `distance_m` to a landing at the alternate, starting at
    /// `start_mass_kg`.
    fn diversion(&self, start_mass_kg: f64, distance_m: f64)
        -> Result<LegEstimate, FuelModelError>;

    /// Fuel flow while holding at minimum-drag speed, kg/s.
    fn holding_fuel_flow_kg_s(&self, mass_kg: f64, altitude_m: f64) -> Result<f64, FuelModelError>;

    /// Fuel flow at normal cruise, kg/s.
    fn cruise_fuel_flow_kg_s(&self, mass_kg: f64) -> Result<f64, FuelModelError>;

    /// Fuel flow at ground idle with every engine running, kg/s.
    fn taxi_fuel_flow_kg_s(&self) -> Result<f64, FuelModelError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quantity(kg: f64) -> FuelQuantity {
        FuelQuantity {
            kg,
            rule: FuelRule::Declared,
        }
    }

    fn plan() -> FuelPlan {
        FuelPlan {
            scheme: FuelScheme::EasaBasic,
            taxi: quantity(200.0),
            trip: quantity(10_000.0),
            contingency: quantity(500.0),
            alternate: quantity(1_500.0),
            final_reserve: quantity(1_000.0),
            additional: quantity(0.0),
            extra: quantity(300.0),
            trip_time_s: 7_200.0,
            destination_landing_mass_kg: 60_000.0,
            reserve_landing_mass_kg: 58_500.0,
        }
    }

    #[test]
    fn the_plan_sums_close_in_every_direction() {
        let plan = plan();
        assert_eq!(plan.takeoff_fuel_kg(), 13_300.0);
        assert_eq!(plan.ramp_fuel_kg(), 13_500.0);
        assert_eq!(plan.block_fuel_kg(), 10_200.0);
        assert_eq!(plan.destination_landing_fuel_kg(), 3_300.0);
        assert_eq!(plan.reserve_fuel_kg(), 3_000.0);
        assert!(plan.is_finite_and_nonnegative());
    }

    /// Taxi-in fuel is on board from brake release to the gate: it is part
    /// of the takeoff fuel and of the fuel remaining on landing, and what is
    /// loaded but not burned gate to gate is exactly the reserves and extra.
    #[test]
    fn the_taxi_in_budget_is_carried_from_brake_release_to_the_gate() {
        let mut plan = plan();
        plan.taxi = FuelQuantity {
            kg: 200.0,
            rule: FuelRule::TaxiTime {
                minutes: 10.0,
                idle_fuel_flow_kg_s: 200.0 / 600.0,
            },
        };
        let taxi_in_kg = 200.0 / 600.0 * TAXI_IN_TIME_MIN * 60.0;
        assert_eq!(plan.taxi_in_fuel_kg(), taxi_in_kg);
        assert_eq!(plan.takeoff_fuel_kg(), 13_300.0 + taxi_in_kg);
        assert_eq!(plan.ramp_fuel_kg(), plan.takeoff_fuel_kg() + 200.0);
        assert_eq!(plan.destination_landing_fuel_kg(), 3_300.0 + taxi_in_kg);
        assert!(
            (plan.ramp_fuel_kg() - plan.block_fuel_kg() - plan.reserve_fuel_kg() - plan.extra.kg)
                .abs()
                < 1e-9
        );
    }

    #[test]
    fn a_negative_or_non_finite_quantity_is_visible() {
        let mut plan = plan();
        plan.extra = quantity(-1.0);
        assert!(!plan.is_finite_and_nonnegative());
        let mut plan = super::tests::plan();
        plan.trip_time_s = f64::NAN;
        assert!(!plan.is_finite_and_nonnegative());
        assert_eq!(FuelQuantity::NONE.kg, 0.0);
        assert_eq!(FuelQuantity::NONE.rule, FuelRule::NotCarried);
    }

    #[test]
    fn model_errors_describe_what_was_asked_for() {
        let text = FuelModelError::MassOutOfRange { mass_kg: 5.0 }.to_string();
        assert!(text.contains("5 kg"));
        let text = FuelModelError::InvalidDistance { distance_m: -1.0 }.to_string();
        assert!(text.contains("-1 m"));
    }
}
