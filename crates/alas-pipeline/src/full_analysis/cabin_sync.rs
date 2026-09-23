// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! One cabin per case: percent-mode FLOPS cabin terms follow the seated
//! layout, while count-mode terms retain the declared installed cabin.
//!
//! The first mass pass has to run before a cabin exists, so it evaluates the
//! registered or declared passenger counts. Once the detailed layout has
//! seated a percent-mode cabin, the furnishings, passenger-service, cabin-
//! crew and air-conditioning terms must describe *that* cabin: the one whose
//! occupants are in the zero-fuel mass, and not a seed count the dynamic
//! capacity solve has since replaced. A count-mode cabin is an installed
//! equipment declaration, so a layout shortfall is reported by payload while
//! its declared furnishings and service terms remain intact. Before this seam
//! the product report could carry a 150-seat operating empty mass under a
//! 180-seat payload.
//!
//! The optimizer path already keeps the two in step
//! (`alas_opt::objective_model::apply_candidate_payload_load_case`); this is
//! the same rule at the report boundary.

use alas_config::cabin::{annotate_flops_cabin_resolution, ResolvedPassengerCounts};
use alas_config::{
    AlasConfig, DesignRequirements, MassModelConfig, MassSizingBasis, PassengerCabinConfig,
};
use alas_payload::layout::{LayoutSummary, PayloadLayout};

/// Resolve the first-pass requirements and FLOPS mirrors from one cabin
/// authority. Count-mode seats are an installed-cabin declaration, so the
/// first evaluation cannot reject a declared 140-seat cabin against a
/// registered 130-seat seed. Percent-mode shares allocate the requested
/// passenger total unless counts are already materialized for that same
/// total; the detailed layout can then refine the result through
/// [`cabin_synchronized`].
pub fn declared_cabin(
    requirements: &DesignRequirements,
    mass_model: &MassModelConfig,
    cabin: &PassengerCabinConfig,
) -> (DesignRequirements, MassModelConfig) {
    if requirements.aircraft_type != "passenger" {
        return (requirements.clone(), mass_model.clone());
    }
    let canonical = cabin.canonicalized_for_product();
    let counts = canonical.resolved_flops_counts(requirements.num_passengers);
    let synchronized_model = with_flops_counts(mass_model, counts);
    let mut synchronized_requirements = requirements.clone();
    if cabin.class_mix_mode == "count" && counts.is_nonempty() {
        synchronized_requirements.num_passengers = counts.total();
    }
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
pub fn cabin_synchronized(
    requirements: &DesignRequirements,
    mass_model: &MassModelConfig,
    layout: &PayloadLayout,
) -> (DesignRequirements, MassModelConfig) {
    cabin_synchronized_from_layout(requirements, mass_model, layout)
}

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

fn with_flops_counts(
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
    // along with furnishings, service and air-conditioning; before this fix
    // `flight_attendant_count` was declared once at preset resolution and
    // never revisited here, so a synced cabin that grew (or shrank) kept its
    // stale crew count. Raise it to the regulatory operational minimum for
    // the now-seated headcount (14 CFR 121.391(a) / EASA ORO.CC.100: one
    // cabin crew member per 50 installed passenger seats, rounded up),
    // without ever lowering a larger declared count: a preset may crew above
    // the floor (the A320-200 case keeps 4 for 150 seats, above the 3 the
    // floor alone would give), and synchronization must not silently shed
    // that margin. Physics review v1.2, finding M3.
    let regulatory_minimum = to_count(counts.total()).div_ceil(50);
    let declared = synchronized_model
        .flops_transport
        .flight_attendant_count
        .unwrap_or(0);
    synchronized_model.flops_transport.flight_attendant_count =
        Some(declared.max(regulatory_minimum));
    synchronized_model
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_mode_replaces_stale_seed_counts_before_the_first_mass_pass() {
        let requirements = DesignRequirements {
            num_passengers: 200,
            ..Default::default()
        };
        let mut cabin = PassengerCabinConfig::default();
        cabin.first.share_pct = 10.0;
        cabin.business.share_pct = 20.0;
        cabin.economy.share_pct = 70.0;
        cabin.first.count = 2;
        cabin.business.count = 18;
        cabin.economy.count = 80;

        let (resolved_requirements, resolved_model) =
            declared_cabin(&requirements, &MassModelConfig::default(), &cabin);

        assert_eq!(resolved_requirements.num_passengers, 200);
        assert_eq!(
            resolved_model.flops_transport.first_class_passenger_count,
            Some(20)
        );
        assert_eq!(
            resolved_model
                .flops_transport
                .business_class_passenger_count,
            Some(40)
        );
        assert_eq!(
            resolved_model.flops_transport.tourist_class_passenger_count,
            Some(140)
        );
    }

    #[test]
    fn count_mode_preserves_the_installed_cabin_and_folds_legacy_premium() {
        let requirements = DesignRequirements {
            num_passengers: 999,
            ..Default::default()
        };
        let mut cabin = PassengerCabinConfig {
            class_mix_mode: "count".to_owned(),
            ..Default::default()
        };
        cabin.first.count = 4;
        cabin.business.count = 16;
        cabin.premium.count = 10;
        cabin.economy.count = 70;

        let (resolved_requirements, resolved_model) =
            declared_cabin(&requirements, &MassModelConfig::default(), &cabin);

        assert_eq!(resolved_requirements.num_passengers, 100);
        assert_eq!(
            resolved_model.flops_transport.first_class_passenger_count,
            Some(4)
        );
        assert_eq!(
            resolved_model
                .flops_transport
                .business_class_passenger_count,
            Some(16)
        );
        assert_eq!(
            resolved_model.flops_transport.tourist_class_passenger_count,
            Some(80)
        );
    }
}
