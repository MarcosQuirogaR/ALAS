// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! One cabin per case: the FLOPS cabin terms follow the seated layout.
//!
//! The first mass pass has to run before a cabin exists, so it evaluates the
//! registered or declared passenger counts. Once the detailed layout has
//! seated the cabin, the furnishings, passenger-service, cabin-crew and
//! air-conditioning terms must describe *that* cabin: the one whose
//! occupants are in the zero-fuel mass, and not a seed count the dynamic
//! capacity solve has since replaced. Before this seam the product report
//! could carry a 150-seat operating empty mass under a 180-seat payload.
//!
//! The optimizer path already keeps the two in step
//! (`alas_opt::objective_model::apply_candidate_payload_load_case`); this is
//! the same rule at the report boundary.

use alas_config::{
    AlasConfig, DesignRequirements, MassModelConfig, MassSizingBasis, PassengerCabinConfig,
};
use alas_payload::layout::{LayoutSummary, PayloadLayout};

/// The first-pass requirements and mass model when the cabin is declared by
/// count: the FLOPS class split and the passenger total are the declared
/// seats, so the first evaluation cannot reject a declared 140-seat cabin
/// against a registered 130-seat seed. A percent-share cabin is left to the
/// layout, whose seated result [`cabin_synchronized`] applies afterwards.
pub(super) fn declared_cabin(
    requirements: &DesignRequirements,
    mass_model: &MassModelConfig,
    cabin: &PassengerCabinConfig,
) -> (DesignRequirements, MassModelConfig) {
    if requirements.aircraft_type != "passenger"
        || cabin.class_mix_mode != "count"
        || cabin.total_seats() <= 0
    {
        return (requirements.clone(), mass_model.clone());
    }
    let count = |seats: i64| usize::try_from(seats.max(0)).unwrap_or(0);
    let mut synchronized_model = mass_model.clone();
    synchronized_model
        .flops_transport
        .first_class_passenger_count = Some(count(cabin.first.count));
    synchronized_model
        .flops_transport
        .business_class_passenger_count = Some(count(cabin.business.count));
    synchronized_model
        .flops_transport
        .tourist_class_passenger_count = Some(count(
        cabin.premium.count.max(0) + cabin.economy.count.max(0),
    ));
    let mut synchronized_requirements = requirements.clone();
    synchronized_requirements.num_passengers = cabin.total_seats();
    (synchronized_requirements, synchronized_model)
}

/// The FLOPS design gross mass a report at `takeoff_mass_kg` is sized on: a
/// registered aircraft keeps its declared design weight, a clean-sheet design
/// couples to the closed mass (`AlasConfig::at_closure_mass`).
pub(super) fn sized_design_gross_mass_kg(config: &AlasConfig, takeoff_mass_kg: f64) -> f64 {
    match config.mass_sizing_basis() {
        MassSizingBasis::FixedAircraft {
            design_gross_mass_kg,
            ..
        } => design_gross_mass_kg,
        MassSizingBasis::Coupled => takeoff_mass_kg,
    }
}

/// The requirements and mass model of the second mass pass, with the FLOPS
/// class counts and the passenger total taken from the seats the layout
/// placed. Cargo layouts, and passenger layouts that seated nobody, return
/// the inputs unchanged.
pub(super) fn cabin_synchronized(
    requirements: &DesignRequirements,
    mass_model: &MassModelConfig,
    layout: &PayloadLayout,
) -> (DesignRequirements, MassModelConfig) {
    let unchanged = || (requirements.clone(), mass_model.clone());
    if requirements.aircraft_type != "passenger" {
        return unchanged();
    }
    let LayoutSummary::Passenger(summary) = &layout.summary else {
        return unchanged();
    };
    let (mut first, mut business, mut tourist) = (0_i64, 0_i64, 0_i64);
    for &(name, seats) in &summary.classes {
        match name {
            "First" => first += seats,
            "Business" => business += seats,
            // FLOPS has three cabin classes; premium economy is priced as
            // tourist, the class its seat and service equations fit.
            _ => tourist += seats,
        }
    }
    let seated = first + business + tourist;
    if seated <= 0 {
        return unchanged();
    }
    let count = |seats: i64| usize::try_from(seats.max(0)).unwrap_or(0);
    let mut synchronized_model = mass_model.clone();
    synchronized_model
        .flops_transport
        .first_class_passenger_count = Some(count(first));
    synchronized_model
        .flops_transport
        .business_class_passenger_count = Some(count(business));
    synchronized_model
        .flops_transport
        .tourist_class_passenger_count = Some(count(tourist));
    let mut synchronized_requirements = requirements.clone();
    synchronized_requirements.num_passengers = seated;
    (synchronized_requirements, synchronized_model)
}
