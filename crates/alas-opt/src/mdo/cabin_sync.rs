// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! One cabin per case, shared by the search and the report: a percent-mode
//! cabin is priced on the cabin the payload layout seats, a count-mode cabin
//! keeps its declared installed equipment (see
//! `alas_pipeline::full_analysis::cabin_sync`, which re-exports this rule for
//! the report boundary).

use alas_config::cabin::{annotate_flops_cabin_resolution, ResolvedPassengerCounts};
use alas_config::{DesignRequirements, MassModelConfig, PassengerCabinConfig};
use alas_payload::layout::{LayoutSummary, PayloadLayout};

/// Layouts a mass pass may build before it keeps the last one: the layout on
/// the first empty mass, the layout on the synchronized cabin's empty mass,
/// and two more for a cabin whose seated count moves once more with the empty
/// mass. A seated count is an integer, so a non-oscillating cabin settles in
/// a few layouts; the bound only stops a pathological two-cycle.
pub const MAX_PAYLOAD_LAYOUTS: usize = 4;

/// Synchronize the second mass pass while retaining an explicit count-mode
/// installed cabin. A declared cabin is an architecture input: if the payload
/// row packer seats fewer rows because the geometry cannot accommodate the
/// declaration, that occupancy shortfall is reported by payload and must not
/// silently shrink the installed furnishings/service mass. Percent-mode cabins
/// remain dynamic and use the seated layout summary.
pub fn cabin_synchronized_for_cabin(
    requirements: &DesignRequirements,
    mass_model: &MassModelConfig,
    cabin: &PassengerCabinConfig,
    layout: &PayloadLayout,
) -> (DesignRequirements, MassModelConfig) {
    if requirements.aircraft_type == "passenger" && cabin.class_mix_mode == "count" {
        let canonical = cabin.canonicalized_for_product();
        // An empty count-mode cabin is still a provisional seed and must use
        // the layout fallback; only a nonempty declared total is an installed
        // cabin whose shortfall must remain visible to feasibility.
        if canonical.total_seats() > 0 {
            let counts = canonical.resolved_flops_counts(requirements.num_passengers);
            let mut synchronized_requirements = requirements.clone();
            synchronized_requirements.num_passengers = counts.total();
            return (
                synchronized_requirements,
                with_flops_counts(mass_model, counts),
            );
        }
    }
    cabin_synchronized_from_layout(requirements, mass_model, layout)
}

fn cabin_synchronized_from_layout(
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
            // FLOPS (NASA/TM-2017-219627) has three cabin classes and no
            // premium-economy term; a "Premium" layout row is priced as
            // tourist here, the closest published class, rather than
            // interpolated or given its own unsourced equation. This
            // understates furnishings (NPF/NPB/NPT eq. 110: 44 lb tourist
            // vs 78 lb business) and passenger service (eq. 124: 2.529 lb
            // vs 3.846 lb) by roughly (78-44)+(3.846-2.529) lb, about 20 kg,
            // per premium seat -- order 0.5 t on a 24-seat premium cabin.
            // Documented rather than corrected: physics review v1.2,
            // finding M4.
            _ => tourist += seats,
        }
    }
    let seated = first + business + tourist;
    if seated <= 0 {
        return unchanged();
    }
    let synchronized_model = with_flops_counts(
        mass_model,
        ResolvedPassengerCounts {
            first,
            business,
            tourist,
        },
    );
    let mut synchronized_requirements = requirements.clone();
    synchronized_requirements.num_passengers = seated;
    (synchronized_requirements, synchronized_model)
}

/// `mass_model` with the FLOPS cabin terms and crew count set to `counts`.
pub fn with_flops_counts(
    mass_model: &MassModelConfig,
    counts: ResolvedPassengerCounts,
) -> MassModelConfig {
    let to_count = |value: i64| usize::try_from(value.max(0)).unwrap_or(usize::MAX);
    let mut synchronized_model = mass_model.clone();
    annotate_flops_cabin_resolution(&mut synchronized_model, counts);
    synchronized_model
        .flops_transport
        .first_class_passenger_count = Some(to_count(counts.first));
    synchronized_model
        .flops_transport
        .business_class_passenger_count = Some(to_count(counts.business));
    synchronized_model
        .flops_transport
        .tourist_class_passenger_count = Some(to_count(counts.tourist));
    // The module doc promises the cabin-crew term follows the seated cabin
    // along with furnishings, service and air-conditioning.
    // `flight_attendant_count` is declared once at preset resolution, so
    // without this step a synced cabin that grew (or shrank) would keep a
    // stale crew count. Raise it to the regulatory operational minimum for
    // the now-seated headcount (14 CFR 121.391(a) / EASA ORO.CC.100: one
    // cabin crew member per 50 installed passenger seats, rounded up),
    // without ever lowering a larger declared count: a preset may crew above
    // the floor (the A320-200 case keeps 4 for 150 seats, above the 3 the
    // floor alone would give), and synchronization must not silently shed
    // that margin.
    let regulatory_minimum = to_count(counts.total()).div_ceil(50);
    let declared = synchronized_model
        .flops_transport
        .flight_attendant_count
        .unwrap_or(0);
    synchronized_model.flops_transport.flight_attendant_count =
        Some(declared.max(regulatory_minimum));
    synchronized_model
}
