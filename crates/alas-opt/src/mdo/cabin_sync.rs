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
                with_layout_cabin(with_flops_counts(mass_model, counts), layout),
            );
        }
    }
    let (requirements, mass_model) =
        cabin_synchronized_from_layout(requirements, mass_model, layout);
    (requirements, with_layout_cabin(mass_model, layout))
}

/// `mass_model` with the layout's cabin crew ([`with_layout_cabin_crew`]) and
/// the all-coach capacity of the cabin shell
/// ([`PayloadLayout::all_coach_capacity`], NASA CR-151970 `Np`), which the
/// FLOPS electrical group reads: the capacity of the shell, not of the
/// declared count, and the airliner's for a converted freighter. A production
/// freighter has none.
fn with_layout_cabin(mass_model: MassModelConfig, layout: &PayloadLayout) -> MassModelConfig {
    let mut mass_model = with_layout_cabin_crew(mass_model, layout);
    mass_model.flops_transport.all_coach_passenger_count = layout.all_coach_capacity;
    mass_model
}

/// `mass_model` with the cabin crew raised to what the layout's decks need
/// (EASA ORO.CC.100(b): the type-certificate number of a registered aircraft
/// for each deck's seats, never fewer than one per 50 seats on that deck).
/// A declared larger count is kept, and manual cabin inputs are left alone.
fn with_layout_cabin_crew(
    mut mass_model: MassModelConfig,
    layout: &PayloadLayout,
) -> MassModelConfig {
    let LayoutSummary::Passenger(summary) = &layout.summary else {
        return mass_model;
    };
    let transport = &mut mass_model.flops_transport;
    if transport.manual_cabin_inputs || summary.seated_pax <= 0 {
        return mass_model;
    }
    let floor = usize::try_from(summary.min_cabin_crew.max(0)).unwrap_or(usize::MAX);
    let declared = transport.flight_attendant_count.unwrap_or(0);
    transport.flight_attendant_count = Some(declared.max(floor));
    mass_model
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
    if mass_model.flops_transport.manual_cabin_inputs {
        return mass_model.clone();
    }
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
    // that margin. ORO.CC.100(a) asks for none on a cabin of at most 19
    // seats, so a business jet's declared zero is kept.
    let regulatory_minimum = alas_config::minimum_cabin_crew(to_count(counts.total()));
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
    use super::{cabin_synchronized_for_cabin, with_flops_counts};
    use alas_config::cabin::ResolvedPassengerCounts;
    use alas_config::{presets, AlasConfig};
    use alas_geom::builder::AircraftBuilder;

    #[test]
    fn the_synchronized_cabin_crew_follows_each_deck_of_the_layout() {
        // A380: 414 main-deck and 141 upper-deck seats take ten and six
        // (EASA TCDS A.110 item 3.9), where one per 50 over the whole
        // aircraft gives twelve.
        let preset = presets::get("A380-800").expect("registered preset");
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": "A380-800" }))
            .expect("preset configuration");
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .expect("preset geometry");
        let layout =
            alas_payload::build_payload_layout(&plane, &config, 0.0, 0.0).expect("preset payload");
        let (_, model) = cabin_synchronized_for_cabin(
            &config.requirements,
            &config.mass_model,
            &config.cabin.passenger,
            &layout,
        );
        // Plus one at the forward stair: 32 of the upper-deck seats stand
        // forward of door U1 (more than 30).
        assert_eq!(model.flops_transport.flight_attendant_count, Some(17));
        // The electrical group reads the all-coach capacity of this same
        // cabin (NASA CR-151970 `Np`): at least the 555 seats it holds.
        let all_coach = layout.all_coach_capacity;
        assert_eq!(model.flops_transport.all_coach_passenger_count, all_coach);
        assert!(all_coach.is_some_and(|seats| seats >= 555), "{all_coach:?}");
    }

    /// The layout of `config` on the A320-200 preset geometry, and the mass
    /// model synchronized to it.
    fn a320_synchronized(
        config: &AlasConfig,
    ) -> (alas_payload::PayloadLayout, alas_config::MassModelConfig) {
        let preset = presets::get("A320-200").expect("registered preset");
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .expect("preset geometry");
        let layout =
            alas_payload::build_payload_layout(&plane, config, 0.0, 0.0).expect("preset payload");
        let (_, model) = cabin_synchronized_for_cabin(
            &config.requirements,
            &config.mass_model,
            &config.cabin.passenger,
            &layout,
        );
        (layout, model)
    }

    fn seated(layout: &alas_payload::PayloadLayout) -> i64 {
        match &layout.summary {
            alas_payload::layout::LayoutSummary::Passenger(summary) => summary.seated_pax,
            alas_payload::layout::LayoutSummary::Cargo(_) => 0,
        }
    }

    /// NASA CR-151970 `Np` is what the cabin "could carry" all-coach at
    /// 34 in (p. 5-12): a count-mode cabin declared below its capacity has
    /// the `Np` of the full cabin, not of its placed rows.
    #[test]
    fn an_underfilled_count_cabin_keeps_the_all_coach_capacity_of_its_shell() {
        let full = AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" }))
            .expect("preset configuration");
        let mut underfilled = full.clone();
        let cabin = &mut underfilled.cabin.passenger;
        cabin.class_mix_mode = "count".to_owned();
        cabin.first.count = 0;
        cabin.business.count = 0;
        cabin.premium.count = 0;
        cabin.economy.count = 100;
        underfilled.requirements.num_passengers = 100;
        let (full_layout, full_model) = a320_synchronized(&full);
        let (underfilled_layout, underfilled_model) = a320_synchronized(&underfilled);
        assert!(seated(&underfilled_layout) < seated(&full_layout));
        let all_coach = full_model.flops_transport.all_coach_passenger_count;
        assert!(all_coach.is_some_and(|seats| seats >= 60), "{all_coach:?}");
        assert_eq!(underfilled_model.flops_transport.all_coach_passenger_count, all_coach);
        assert_eq!(underfilled_layout.all_coach_capacity, full_layout.all_coach_capacity);
    }

    /// A conversion keeps the systems of the airliner it was built from
    /// (`converted_freighter_seats`), so the A320P2F prices its electrical
    /// group on the A320-200's `Np`; settings as the acceptance matrix builds
    /// the freighter (`published_forward_cg_limits.rs`).
    #[test]
    fn a_converted_freighter_keeps_the_all_coach_capacity_of_its_airliner() {
        let airliner = AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" }))
            .expect("preset configuration");
        let freighter = AlasConfig::from_value(&serde_json::json!({
            "preset": "A320-200",
            "requirements": {
                "mtow_kg": 78_000.0,
                "aircraft_type": "cargo",
                "cabin_preset": "Custom",
                "num_passengers": 0,
                "cargo_payload_kg": 21_400.0,
                "max_structural_payload_kg": 21_400.0,
            },
            "cabin": { "cargo": { "lower_deck_uld": "AUTO" } },
        }))
        .expect("freighter configuration");
        assert!(freighter.cabin.cargo.converted_freighter_seats.is_some());
        let (_, airliner_model) = a320_synchronized(&airliner);
        let (freighter_layout, freighter_model) = a320_synchronized(&freighter);
        assert_eq!(seated(&freighter_layout), 0);
        let all_coach = airliner_model.flops_transport.all_coach_passenger_count;
        assert!(all_coach.is_some(), "{all_coach:?}");
        assert_eq!(freighter_model.flops_transport.all_coach_passenger_count, all_coach);
        // A production freighter has no airliner systems and keeps equation 106.
        let mut production = freighter;
        production.cabin.cargo.converted_freighter_seats = None;
        let (production_layout, production_model) = a320_synchronized(&production);
        assert_eq!(production_layout.all_coach_capacity, None);
        assert_eq!(production_model.flops_transport.all_coach_passenger_count, None);
    }

    #[test]
    fn a_cabin_of_at_most_nineteen_seats_keeps_its_declared_zero_crew() {
        let mut mass_model = alas_config::MassModelConfig::default();
        mass_model.flops_transport.flight_attendant_count = Some(0);
        let counts = ResolvedPassengerCounts {
            first: 0,
            business: 8,
            tourist: 0,
        };
        let model = with_flops_counts(&mass_model, counts);
        assert_eq!(model.flops_transport.flight_attendant_count, Some(0));
    }
}
