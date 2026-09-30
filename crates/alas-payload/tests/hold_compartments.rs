// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Baggage is stowed in compartments: the derived compartments of the ATR
//! 72-600, A320-200 and A220-300, the overflow split between them, and the
//! invariant that no requested mass is dropped.
//!
//! These are numerical-verification tests of the loading model. They compare
//! placements with each other and with physical bounds; they are not a
//! validation against a weight-and-balance manual.

// Failed construction is a failed test assertion.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use alas_config::{AlasConfig, BaggagePolicy, CargoDeckConfig, HoldCompartmentConfig, HoldDeck};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::builder::AircraftBuilder;
use alas_payload::cargo::{
    derive_hold_compartments, CargoLoadManager, HoldCompartment, STOWAGE_DENSITY_KG_M3,
};
use alas_payload::geometry::CabinGeometry;
use alas_payload::layout::{DeckItem, ItemKind, PassengerSummary, MAIN};
use alas_payload::{build_payload_layout, LayoutSummary, PayloadLayout};

/// A registered preset and the OEW CG the pipeline reports for it (percent
/// MAC, recorded from the mass statement). Only the balance target of the
/// baggage trim depends on it.
struct Case {
    name: &'static str,
    oew_cg_pct_mac: f64,
}

const ATR: Case = Case {
    name: "ATR72-600",
    oew_cg_pct_mac: 29.93,
};
const A320: Case = Case {
    name: "A320-200",
    oew_cg_pct_mac: 34.35,
};
const A220: Case = Case {
    name: "A220-300",
    oew_cg_pct_mac: 24.72,
};

struct Built {
    config: AlasConfig,
    plane: Airplane,
    oew_kg: f64,
}

/// Bag mass per passenger that no narrowbody hold can take, kg.
const OVERLOAD_BAG_KG: f64 = 400.0;

fn build(case: &Case, edit: impl FnOnce(&mut AlasConfig)) -> Built {
    let preset = alas_config::presets::get(case.name).expect("registered preset");
    let mut config =
        AlasConfig::from_value(&serde_json::json!({ "preset": case.name })).expect("config");
    edit(&mut config);
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("geometry");
    Built {
        oew_kg: preset
            .reference
            .oew_kg
            .unwrap_or(0.55 * config.requirements.mtow_kg),
        config,
        plane,
    }
}

fn geometry(built: &Built) -> CabinGeometry {
    CabinGeometry::new(
        &built.plane,
        &built.config.geometry,
        built.config.cabin.passenger.wall_thickness_m,
    )
    .expect("cabin frame")
}

fn layout(case: &Case, built: &Built) -> (PayloadLayout, Box<PassengerSummary>) {
    let g = geometry(built);
    let x_oew = g.pct_mac_to_x(case.oew_cg_pct_mac);
    let layout = build_payload_layout(&built.plane, &built.config, built.oew_kg, x_oew)
        .expect("layout builds");
    let LayoutSummary::Passenger(summary) = layout.summary.clone() else {
        panic!("passenger layout");
    };
    (layout, summary)
}

fn bags(layout: &PayloadLayout) -> Vec<&DeckItem> {
    layout
        .items
        .iter()
        .filter(|item| item.kind == ItemKind::Bag)
        .collect()
}

fn bag_mass(layout: &PayloadLayout) -> f64 {
    bags(layout).iter().map(|item| item.mass).sum()
}

fn bag_cg_x(layout: &PayloadLayout) -> f64 {
    let items = bags(layout);
    items.iter().map(|item| item.mass * item.x).sum::<f64>() / bag_mass(layout)
}

/// Longitudinal extent of the seats and monuments.
fn cabin_extent(layout: &PayloadLayout) -> (f64, f64) {
    layout
        .items
        .iter()
        .filter(|item| {
            matches!(
                item.kind,
                ItemKind::SeatRow
                    | ItemKind::Galley
                    | ItemKind::Lav
                    | ItemKind::AccessibleLav
                    | ItemKind::WheelchairStowage
            )
        })
        .fold((f64::MAX, f64::MIN), |(lo, hi), item| {
            (
                lo.min(item.x - item.length / 2.0),
                hi.max(item.x + item.length / 2.0),
            )
        })
}

fn compartments(built: &Built, layout: &PayloadLayout) -> Vec<HoldCompartment> {
    let g = geometry(built);
    let manager = CargoLoadManager::new(
        &g,
        CargoDeckConfig {
            use_main_deck: false,
            ..built.config.cabin.cargo.clone()
        },
    );
    derive_hold_compartments(
        &g,
        &manager.slots,
        Some(cabin_extent(layout)),
        &built.config.cabin.cargo.hold_compartments,
    )
}

fn requested_kg(summary: &PassengerSummary) -> f64 {
    1_000.0 * (summary.bag_mass_t + summary.belly_cargo_t)
}

fn heavy_bags(config: &mut AlasConfig) {
    config.cabin.passenger.checked_bag_mass_kg = OVERLOAD_BAG_KG;
}

#[test]
fn the_atr_has_two_main_deck_compartments_outside_the_seats_with_its_bags_inside_the_hull() {
    let built = build(&ATR, |_| {});
    let g = geometry(&built);
    let (layout, summary) = layout(&ATR, &built);
    let compartments = compartments(&built, &layout);
    let (seat_start, seat_end) = cabin_extent(&layout);

    assert_eq!(compartments.len(), 2, "{compartments:?}");
    assert!(
        summary.hold_capacity_t == 0.0,
        "the ATR has no under-floor hold"
    );
    let (forward, aft) = (&compartments[0], &compartments[1]);
    assert!((forward.x_start_m - (g.x_min + 2.6)).abs() < 1e-9);
    assert!(forward.x_end_m <= seat_start + 1e-9);
    assert!(aft.x_start_m >= seat_end - 1e-9);
    assert!((aft.x_end_m - (g.cabin_end_x + 0.5 * g.tailcone_len)).abs() < 1e-9);
    for compartment in &compartments {
        assert!(compartment.length_m() >= 0.5);
        assert!(compartment.volume_m3 > 1.0, "{compartment:?}");
        assert_eq!(compartment.deck, HoldDeck::Main);
    }

    assert!(summary.hold_compartment_masses_kg.len() >= 2);
    assert!(summary
        .hold_compartment_masses_kg
        .iter()
        .all(|(_, mass)| *mass > 0.0));
    let x = bag_cg_x(&layout);
    assert!(x > g.x_min && x < g.x_max, "bag CG {x} m outside the hull");
    for item in bags(&layout) {
        assert_eq!(item.deck, MAIN);
        let owner = compartments
            .iter()
            .find(|c| c.contains(item.x))
            .expect("every block sits in a compartment");
        assert!(item.length <= owner.length_m() + 1e-9);
        assert!(item.x - item.length / 2.0 >= owner.x_start_m - 1e-9);
        assert!(item.x + item.length / 2.0 <= owner.x_end_m + 1e-9);
    }
}

#[test]
fn the_atr_overflow_is_not_one_aft_block_and_the_payload_leaves_the_tail() {
    let built = build(&ATR, |_| {});
    let g = geometry(&built);
    let (layout, summary) = layout(&ATR, &built);
    assert!(bags(&layout).len() >= 2);
    // The frozen single block sat at cabin_end - 1.5 m and put the payload CG
    // near 106 % MAC; splitting it over both compartments must pull it in.
    assert!(
        summary.cg_pct_mac < 90.0,
        "payload CG {:.1} %MAC",
        summary.cg_pct_mac
    );
    assert!(summary.forward_hold_baggage_fraction > 0.2);
    assert!(summary.forward_hold_baggage_fraction < 0.8);
    // The bags straddle the seats, so their centroid stays within the span
    // of the two compartment centres.
    let compartments = compartments(&built, &layout);
    let bag_x = bag_cg_x(&layout);
    assert!(
        bag_x > compartments[0].centroid_x_m() && bag_x < compartments[1].centroid_x_m(),
        "bag CG {bag_x} m at {:.1} %MAC",
        g.x_to_pct_mac(bag_x)
    );
}

#[test]
fn requested_baggage_mass_is_always_placed_in_full() {
    type Edit = fn(&mut AlasConfig);
    let cases: [(&Case, Edit); 8] = [
        (&ATR, |_| {}),
        (&A320, |_| {}),
        (&A220, |_| {}),
        (&ATR, heavy_bags),
        (&A320, heavy_bags),
        (&A220, heavy_bags),
        (&A320, |config| {
            config.cabin.cargo.baggage_policy = BaggagePolicy::VolumeProportional
        }),
        (&A220, |config| {
            config.cabin.cargo.baggage_policy = BaggagePolicy::VolumeProportional;
            heavy_bags(config);
        }),
    ];
    for (case, edit) in cases {
        let built = build(case, edit);
        let (layout, summary) = layout(case, &built);
        let placed = bag_mass(&layout);
        assert!(
            (placed - requested_kg(&summary)).abs() < 1e-6 * requested_kg(&summary).max(1.0),
            "{}: placed {placed} kg of {} kg",
            case.name,
            requested_kg(&summary)
        );
        let by_compartment: f64 = summary
            .hold_compartment_masses_kg
            .iter()
            .map(|(_, mass)| mass)
            .sum();
        assert!(
            (by_compartment - placed).abs() < 1e-6 * placed.max(1.0),
            "{}: compartments carry {by_compartment} kg of {placed} kg",
            case.name
        );
        assert!(summary.overload_kg >= 0.0);
    }
}

#[test]
fn a_forced_overload_is_reported_and_spread_over_both_compartments() {
    for case in [&A320, &A220] {
        let normal = build(case, |_| {});
        let (_, normal_summary) = layout(case, &normal);
        assert_eq!(normal_summary.overload_kg, 0.0, "{}", case.name);

        let built = build(case, heavy_bags);
        let (layout, summary) = layout(case, &built);
        assert!(summary.overload_kg > 0.0, "{}", case.name);
        assert!(summary.overload_kg < requested_kg(&summary));
        let loaded: Vec<&(String, f64)> = summary
            .hold_compartment_masses_kg
            .iter()
            .filter(|(_, mass)| *mass > 1.0)
            .collect();
        assert!(loaded.len() >= 2, "{}: {loaded:?}", case.name);
        assert!(
            summary.forward_hold_baggage_fraction > 0.0
                && summary.forward_hold_baggage_fraction < 1.0
        );
        assert!(bags(&layout).len() >= 2, "{}", case.name);
    }
}

#[test]
fn volume_proportional_loading_moves_a320_baggage_aft_of_the_target_cg_split() {
    let target = build(&A320, |_| {});
    let (_, target_summary) = layout(&A320, &target);
    let volume = build(&A320, |config| {
        config.cabin.cargo.baggage_policy = BaggagePolicy::VolumeProportional
    });
    let (_, volume_summary) = layout(&A320, &volume);
    assert!(
        volume_summary.forward_hold_baggage_fraction < target_summary.forward_hold_baggage_fraction
    );
    assert!(volume_summary.cg_pct_mac > target_summary.cg_pct_mac);
    assert!(volume_summary.forward_hold_baggage_fraction > 0.2);
}

#[test]
fn a_declared_compartment_list_replaces_the_derived_one() {
    let declared = vec![
        HoldCompartmentConfig {
            name: "Nose".to_owned(),
            x_start_m: 2.6,
            x_end_m: 3.5,
            volume_m3: 3.0,
            max_net_kg: Some(300.0),
            deck: HoldDeck::Main,
        },
        HoldCompartmentConfig {
            name: "Tail".to_owned(),
            x_start_m: 22.7,
            x_end_m: 24.1,
            volume_m3: 6.0,
            max_net_kg: None,
            deck: HoldDeck::Main,
        },
    ];
    let built = build(&ATR, |config| {
        config.cabin.cargo.hold_compartments = declared.clone()
    });
    let (layout, summary) = layout(&ATR, &built);
    let names: Vec<&str> = summary
        .hold_compartment_masses_kg
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(names, ["Nose", "Tail"]);
    assert!((bag_mass(&layout) - requested_kg(&summary)).abs() < 1e-6);
    // The nose limit is 300 kg and the tail takes 960 kg by volume, so the
    // request (1,120 kg) fits and the nose stays at or below its limit.
    assert!(summary.hold_compartment_masses_kg[0].1 <= 300.0 + 1e-6);
    assert!(summary.hold_compartment_masses_kg[1].1 <= 6.0 * STOWAGE_DENSITY_KG_M3 + 1e-6);
    assert_eq!(summary.overload_kg, 0.0);
}

#[test]
fn a320_derived_hold_volumes_are_reported_against_the_published_ones() {
    let built = build(&A320, |_| {});
    let (layout, _) = layout(&A320, &built);
    let g = geometry(&built);
    let manager = CargoLoadManager::new(
        &g,
        CargoDeckConfig {
            use_main_deck: false,
            ..built.config.cabin.cargo.clone()
        },
    );
    let derived = derive_hold_compartments(&g, &manager.slots, Some(cabin_extent(&layout)), &[]);
    let total: f64 = derived.iter().map(|c| c.volume_m3).sum();
    // Usable volumes from the A320 airport-planning manual (FWD, AFT, BULK),
    // with the caveat that the extraction's column alignment is unverified.
    let published_total = 13.28 + 18.26 + 5.88;
    assert!(derived.iter().all(|c| c.volume_m3 > 0.0));
    // The integral has no frame, floor-beam or door allowance, so it may
    // exceed the published usable volume, but not by a different order.
    assert!(
        total > 0.8 * published_total && total < 1.6 * published_total,
        "derived {total:.2} m3 against published {published_total:.2} m3 in {derived:?}"
    );
}
