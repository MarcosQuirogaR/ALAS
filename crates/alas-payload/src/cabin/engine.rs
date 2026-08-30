// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/cabin_layout.py (`build_passenger_layout`)
// Reference: alas @ rust-port-baseline.

//! The passenger layout engine: seats, monuments, exits and baggage assembled
//! into one interior.
//!
//! The four passes run in a fixed order because each depends on the last. The
//! seating carves out the bays; the monuments and exits fill them; the baggage
//! is trimmed toward the centre of gravity the seating produced. The order they
//! append their items in is part of the result -- a deck plan walking the list
//! differently would draw monuments over seats -- so it is reproduced exactly.

use alas_config::{DesignRequirements, PassengerCabinConfig};

use super::fittings::{place_baggage, place_exits, place_monuments, place_overhead_bins};
use super::resolve_aisle_width;
use super::seating::{place_seats, resolve_classes};
use crate::cargo::CargoMassSemantics;
use crate::geometry::CabinGeometry;
use crate::layout::{
    mass_properties, ItemKind, LayoutSummary, Mode, PassengerSummary, PayloadLayout,
};
use crate::numeric::round_to_digit;

/// Decimal places the deck utilization percentage is reported to.
const UTILIZATION_DIGITS: usize = 1;

/// Build the passenger interior for a cabin configuration on a fuselage.
///
/// The operating-empty mass and its balance are not arguments here, although
/// the dispatcher accepts them: only the freighter loader needs them, to solve
/// the payload centre of gravity backwards from a target *aircraft* centre of
/// gravity. A passenger cabin trims its bags toward the seating instead, which
/// it can see for itself.
pub fn build_passenger_layout(
    g: &CabinGeometry,
    pax: &PassengerCabinConfig,
    req: &DesignRequirements,
) -> PayloadLayout {
    build_passenger_layout_with_mass_semantics(g, pax, req, CargoMassSemantics::Net, true)
}

/// Build a passenger layout with the frozen gross-target baggage correction
/// used by the Python parity fixture. Product analyses should call
/// [`build_passenger_layout`].
pub fn build_passenger_layout_reference_compatibility(
    g: &CabinGeometry,
    pax: &PassengerCabinConfig,
    req: &DesignRequirements,
) -> PayloadLayout {
    build_passenger_layout_with_mass_semantics(
        g,
        pax,
        req,
        CargoMassSemantics::ReferenceGross,
        false,
    )
}

fn build_passenger_layout_with_mass_semantics(
    g: &CabinGeometry,
    pax: &PassengerCabinConfig,
    req: &DesignRequirements,
    mass_semantics: CargoMassSemantics,
    product_interior: bool,
) -> PayloadLayout {
    let mut classes = resolve_classes(pax, req.num_passengers);
    let total_pax: i64 = classes.iter().map(|class| class.config.count).sum();
    let aisle_w = resolve_aisle_width(pax, total_pax);

    let mut seating = place_seats(g, pax, &mut classes, aisle_w);
    let seated: i64 = classes.iter().map(|class| class.seated).sum();

    let mut items = std::mem::take(&mut seating.items);
    if product_interior {
        let overhead_bins = place_overhead_bins(g, &items);
        items.extend(overhead_bins);
    }
    let (monuments, monument_counts) = place_monuments(
        g,
        pax,
        &mut seating.bays,
        total_pax,
        seating.max_aisles,
        product_interior,
    );
    items.extend(monuments);
    let exits = place_exits(g, &seating);
    items.extend(exits.items);

    // The bags follow the passengers, so the trim target is the seating's own
    // balance. An empty cabin has none, and the middle of it is the neutral
    // answer rather than the datum.
    let (seat_mass, seat_cg) = seat_mass_and_cg(&items, g);
    let bags = place_baggage(g, pax, req, seated, seat_mass, seat_cg, mass_semantics);
    items.extend(bags.items);

    let (total_mass, cg_x, cg_y) = mass_properties(&items);
    let summary = PassengerSummary {
        total_pax,
        seated_pax: seated,
        unseated_pax: (total_pax - seated).max(0),
        classes: classes
            .iter()
            .map(|class| (class.name, class.seated))
            .collect(),
        lavatories: monument_counts.lavatories,
        galleys: monument_counts.galleys,
        accessible_lavatories: monument_counts.accessible_lavatories,
        wheelchair_stowages: monument_counts.wheelchair_stowages,
        exit_type: exits.exit_type,
        exit_pairs: exits.pairs,
        exit_capacity: exits.pairs * exits.capacity_per_side * 2,
        max_certifiable_capacity: seating.deck_caps.total,
        payload_t: total_mass / 1000.0,
        seat_mass_t: seat_mass / 1000.0,
        bag_mass_t: bags.bag_mass / 1000.0,
        belly_cargo_t: bags.belly_cargo / 1000.0,
        hold_capacity_t: bags.hold_capacity / 1000.0,
        hold_used_t: bags.hold_used / 1000.0,
        hold_ulds: bags.hold_ulds,
        aisle_width_m: aisle_w,
        max_abreast: seating.max_abreast,
        n_aisles: seating.max_aisles,
        deck_utilization: seating
            .deck_utilization
            .iter()
            .map(|&(deck, used)| (deck, round_to_digit(100.0 * used, UTILIZATION_DIGITS)))
            .collect(),
        cg_pct_mac: if total_mass > 0.0 {
            g.x_to_pct_mac(cg_x)
        } else {
            0.0
        },
        double_deck: g.is_double_deck,
    };

    PayloadLayout {
        mode: Mode::Passenger,
        items,
        total_mass,
        cg_x,
        cg_y,
        summary: LayoutSummary::Passenger(Box::new(summary)),
    }
}

/// The seated mass and where it balances, over the seat rows alone.
fn seat_mass_and_cg(items: &[crate::layout::DeckItem], g: &CabinGeometry) -> (f64, f64) {
    let mut mass = 0.0;
    let mut moment = 0.0;
    for item in items.iter().filter(|item| item.kind == ItemKind::SeatRow) {
        mass += item.mass;
        moment += item.mass * item.x;
    }
    if mass > 0.0 {
        (mass, moment / mass)
    } else {
        (mass, 0.5 * (g.cabin_start_x + g.cabin_end_x))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod product_tests {
    use super::*;
    use crate::build::build_payload_layout;
    use crate::layout::{ItemMeta, OverheadBinType};
    use alas_config::{presets, AlasConfig};
    use alas_geom::builder::AircraftBuilder;

    #[test]
    fn widebody_product_layout_has_side_and_center_bins_and_accessibility_items() {
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": "A380-800" }))
            .expect("the registered A380 preset loads");
        let preset = presets::get("A380-800").expect("the registered A380 preset resolves");
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .expect("the A380 geometry builds");
        let layout = build_payload_layout(&plane, &config, 0.0, 0.0)
            .expect("the A380 passenger layout builds");

        let bin_types: Vec<OverheadBinType> = layout
            .items
            .iter()
            .filter_map(|item| match &item.meta {
                ItemMeta::OverheadBin(meta) => Some(meta.bin_type),
                _ => None,
            })
            .collect();
        assert!(bin_types.contains(&OverheadBinType::Sidewall));
        assert!(bin_types.contains(&OverheadBinType::Center));
        assert!(layout
            .items
            .iter()
            .any(|item| item.kind == ItemKind::AccessibleLav));
        assert!(layout
            .items
            .iter()
            .any(|item| item.kind == ItemKind::WheelchairStowage));
    }

    #[test]
    fn seated_aircraft_presets_have_sidewall_bin_runs() {
        for name in presets::available() {
            let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
                .expect("preset config");
            let preset = presets::get(name).expect("registered preset");
            let plane = AircraftBuilder::new(Some(config.geometry.clone()))
                .build(Some(&preset.design_vector), true)
                .expect("preset geometry");
            let layout = build_payload_layout(&plane, &config, 0.0, 0.0).expect("layout");
            if layout
                .items
                .iter()
                .any(|item| item.kind == ItemKind::SeatRow)
            {
                assert!(
                    layout.items.iter().any(|item| matches!(item.meta,
                    ItemMeta::OverheadBin(meta) if meta.bin_type == OverheadBinType::Sidewall)),
                    "{name} has seated rows but no feasible sidewall bin run"
                );
            }
        }
    }
}
