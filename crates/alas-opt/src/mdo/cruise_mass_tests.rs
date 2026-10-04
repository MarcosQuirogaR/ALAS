// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use alas_config::FuelScheme;
use alas_mass::fuel_plan::{FuelPlan, FuelQuantity, FuelRule, TAXI_IN_TIME_MIN};

use super::*;

const IDLE_FLOW_KG_S: f64 = 0.4;
const CONTINGENCY_FRACTION: f64 = 0.05;

fn declared(kg: f64) -> FuelQuantity {
    FuelQuantity {
        kg,
        rule: FuelRule::Declared,
    }
}

/// A plan of a route mission with a trip-share contingency (EASA basic).
fn route_plan(trip_kg: f64) -> FuelPlan {
    FuelPlan {
        scheme: FuelScheme::EasaBasic,
        taxi: FuelQuantity {
            kg: 300.0,
            rule: FuelRule::TaxiTime {
                minutes: 12.5,
                idle_fuel_flow_kg_s: IDLE_FLOW_KG_S,
            },
        },
        trip: FuelQuantity {
            kg: trip_kg,
            rule: FuelRule::TripBurn,
        },
        contingency: FuelQuantity {
            kg: CONTINGENCY_FRACTION * trip_kg,
            rule: FuelRule::TripFraction {
                fraction: CONTINGENCY_FRACTION,
            },
        },
        alternate: FuelQuantity {
            kg: 2_400.0,
            rule: FuelRule::Diversion {
                distance_m: 370_400.0,
            },
        },
        final_reserve: FuelQuantity {
            kg: 1_900.0,
            rule: FuelRule::Holding { minutes: 30.0 },
        },
        additional: declared(0.0),
        extra: declared(0.0),
        trip_time_s: 18_000.0,
        destination_landing_mass_kg: 150_000.0,
        reserve_landing_mass_kg: 146_000.0,
    }
}

/// Fuel left on landing by a mission flown on `trip_kg` with the reserves of
/// `plan`, the contingency rescaled to that trip: computed directly from the
/// quantities, not through the function under test.
fn landing_fuel_after_trip_kg(plan: &FuelPlan, trip_kg: f64) -> f64 {
    CONTINGENCY_FRACTION * trip_kg
        + plan.alternate.kg
        + plan.final_reserve.kg
        + plan.additional.kg
        + plan.extra.kg
        + IDLE_FLOW_KG_S * TAXI_IN_TIME_MIN * 60.0
}

/// A mission-closed loading (the dispatch's own takeoff fuel) gives back the
/// dispatch's own trip, so the mid-cruise mass is the closed mission's.
#[test]
fn a_dispatch_loading_flies_its_own_trip() {
    let plan = route_plan(40_000.0);
    let zero_fuel_kg = 150_000.0;
    let takeoff_kg = zero_fuel_kg + plan.takeoff_fuel_kg();
    let trip = trip_fuel_of_loading_kg(plan.takeoff_fuel_kg(), &plan);
    assert!((trip - 40_000.0).abs() < 1.0e-6, "trip {trip}");
    let mid = mid_cruise_mass_of_loading_kg(takeoff_kg, plan.takeoff_fuel_kg(), &plan);
    assert!((mid - (takeoff_kg - 20_000.0)).abs() < 1.0e-6, "mid {mid}");
}

/// Property over a range of loadings and route trips: the mid-cruise mass
/// is a real flight state of the loaded aircraft. It lies between the
/// landing state of the mission the loading flies (zero-fuel mass plus the
/// reserves that mission must keep) and the takeoff mass, the fuel burned is
/// the fuel loaded less those reserves (mass conservation), and it is
/// exactly halfway along the trip.
#[test]
fn the_mid_cruise_state_lies_between_the_landing_and_takeoff_states() {
    let zero_fuel_kg = 146_000.0;
    for route_trip_kg in [5_000.0, 30_000.0, 55_000.0, 90_000.0] {
        let plan = route_plan(route_trip_kg);
        let minimum_fuel_kg = landing_fuel_after_trip_kg(&plan, 0.0);
        for step in 0..=40 {
            let loaded_kg = minimum_fuel_kg + 3_000.0 * f64::from(step);
            let takeoff_kg = zero_fuel_kg + loaded_kg;
            let trip_kg = trip_fuel_of_loading_kg(loaded_kg, &plan);
            let landing_fuel_kg = landing_fuel_after_trip_kg(&plan, trip_kg);
            // Mass conservation: loaded = burned + kept.
            assert!(
                (trip_kg + landing_fuel_kg - loaded_kg).abs() < 1.0e-6,
                "route {route_trip_kg} load {loaded_kg}: trip {trip_kg} + kept \
                 {landing_fuel_kg} != loaded"
            );
            let mid = mid_cruise_mass_of_loading_kg(takeoff_kg, loaded_kg, &plan);
            let landing_kg = zero_fuel_kg + landing_fuel_kg;
            assert!(
                landing_kg - 1.0e-6 <= mid && mid <= takeoff_kg + 1.0e-6,
                "mid {mid} outside [{landing_kg}, {takeoff_kg}]"
            );
            assert!(
                (mid - 0.5 * (takeoff_kg + landing_kg)).abs() < 1.0e-6,
                "mid {mid} is not the mean of {takeoff_kg} and {landing_kg}"
            );
        }
    }
}

/// A maximum loading heavier than the route's dispatch flies a longer trip
/// than the route; mixing the loading's takeoff mass with the route's trip
/// would leave the mass heavier than any state that mission passes through
/// at mid-cruise.
#[test]
fn a_maximum_loading_burns_more_than_the_route_trip() {
    let plan = route_plan(55_000.0);
    let zero_fuel_kg = 146_000.0;
    let loaded_kg = 111_000.0;
    let takeoff_kg = zero_fuel_kg + loaded_kg;
    let mid = mid_cruise_mass_of_loading_kg(takeoff_kg, loaded_kg, &plan);
    let mixed = takeoff_kg - 0.5 * plan.trip.kg;
    assert!(mid < mixed, "mid {mid} vs mixed {mixed}");
}

/// Loading below the reserves flies no trip, and non-finite inputs are
/// reported as `NaN` rather than a mass.
#[test]
fn degenerate_loadings_do_not_invent_a_flight() {
    let plan = route_plan(20_000.0);
    assert_eq!(trip_fuel_of_loading_kg(1_000.0, &plan), 0.0);
    assert_eq!(
        mid_cruise_mass_of_loading_kg(200_000.0, 1_000.0, &plan),
        200_000.0
    );
    assert!(trip_fuel_of_loading_kg(f64::NAN, &plan).is_nan());
    assert!(mid_cruise_mass_of_loading_kg(f64::INFINITY, 10_000.0, &plan).is_nan());
    assert!(mid_cruise_mass_of_loading_kg(200_000.0, f64::NAN, &plan).is_nan());
}
