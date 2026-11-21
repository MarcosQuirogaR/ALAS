// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The validity status follows the chart domain of the evaluated point.

use super::*;

fn validity(
    model: &EmpiricalTurbofanModel,
    flight: FlightCondition,
    demand: PropulsionDemand,
) -> ValidityStatus {
    model
        .evaluate(&request_at(flight, demand))
        .unwrap_or_else(|error| panic!("evaluate: {error}"))
        .validity
}

#[test]
fn a_point_inside_the_charts_is_valid() {
    let model = a320_class_model();
    let cruise = standard_flight(10_668.0, 0.78);
    for demand in [
        PropulsionDemand::NormalizedForce(0.8),
        PropulsionDemand::Rating(PropulsionRating::MaximumClimb),
        PropulsionDemand::Rating(PropulsionRating::Cruise),
        PropulsionDemand::Rating(PropulsionRating::FlightIdle),
    ] {
        assert_eq!(validity(&model, cruise, demand), ValidityStatus::Valid);
    }
    let takeoff = PropulsionDemand::Rating(PropulsionRating::TakeoffGoAround);
    assert!(validity(&model, standard_flight(0.0, 0.0), takeoff).is_valid());
    assert!(validity(&model, standard_flight(0.0, 0.25), takeoff).is_valid());
}

#[test]
fn a_point_outside_the_charts_is_extrapolated_and_names_the_bound() {
    let model = a320_class_model();
    let reason = |flight, demand| match validity(&model, flight, demand) {
        ValidityStatus::Extrapolated { reason } => reason,
        ValidityStatus::Valid => panic!("{demand:?} should be outside the charts"),
    };
    // Below the lowest Mach sample of the maximum-climb charts.
    let slow = reason(
        standard_flight(3_000.0, 0.2),
        PropulsionDemand::Rating(PropulsionRating::MaximumClimb),
    );
    assert!(
        slow.contains("Mach 0.200") && slow.contains("maximum-climb"),
        "{slow}"
    );
    // Take-off rating above the take-off charts' Mach and height.
    let fast = reason(
        standard_flight(8_000.0, 0.6),
        PropulsionDemand::Rating(PropulsionRating::TakeoffGoAround),
    );
    assert!(
        fast.contains("Mach 0.600") && fast.contains("20000 ft"),
        "{fast}"
    );
    // Maximum continuous has no chart of its own.
    let continuous = reason(
        standard_flight(10_668.0, 0.78),
        PropulsionDemand::Rating(PropulsionRating::MaximumContinuous),
    );
    assert!(continuous.contains("maximum-continuous"), "{continuous}");
}

#[test]
fn a_bypass_ratio_outside_the_charts_is_extrapolated_everywhere() {
    let mut deck = a320_class_model().deck;
    deck.bypass_ratio = 2.5;
    let model = model_from_deck(deck);
    let cruise = standard_flight(10_668.0, 0.78);
    let status = validity(&model, cruise, PropulsionDemand::NormalizedForce(0.8));
    assert!(
        status
            .extrapolation_reason()
            .is_some_and(|reason| reason.contains("bypass ratio 2.50")),
        "{status:?}"
    );
    let capability = model
        .capability(cruise, FailureState::None)
        .unwrap_or_else(|error| panic!("capability: {error}"));
    assert!(!capability.validity.is_valid());
    assert!(a320_class_model()
        .capability(cruise, FailureState::None)
        .is_ok_and(|capability| capability.validity.is_valid()));
}
