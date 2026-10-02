// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Physical rules of a cabin bounded by declared doors, and the sourced seat
//! counts it has to reach.

use alas_config::{presets, AlasConfig, DesignMode};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::builder::AircraftBuilder;

use super::super::{layout_rating, stations::MONUMENT_BAY_LENGTH_M};
use crate::build::{
    build_payload_layout, product_cabin_geometry,
    simulate_passenger_counts_for_seat_mix_with_source_cap,
};
use crate::cabin::cabin_deck_segments;
use crate::geometry::CabinGeometry;
use crate::layout::{DeckItem, ItemKind, LayoutSummary, PassengerSummary};

struct Built {
    config: AlasConfig,
    plane: Airplane,
    geometry: CabinGeometry,
}

fn built(name: &str, edit: impl FnOnce(&mut AlasConfig, &mut alas_config::DesignVector)) -> Built {
    let preset = presets::get(name).expect("registered preset");
    let mut config =
        AlasConfig::from_value(&serde_json::json!({ "preset": name })).expect("preset config");
    let mut design = preset.design_vector;
    edit(&mut config, &mut design);
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&design), true)
        .expect("preset geometry builds");
    let geometry = product_cabin_geometry(&plane, &config).expect("cabin frame");
    Built {
        config,
        plane,
        geometry,
    }
}

fn clean_sheet(config: &mut AlasConfig, _: &mut alas_config::DesignVector) {
    config.optimizer.design_space.mode = DesignMode::CleanSheet;
}

fn layout(built: &Built) -> (Vec<DeckItem>, PassengerSummary) {
    let layout = build_payload_layout(&built.plane, &built.config, 0.0, 0.0).expect("layout");
    let LayoutSummary::Passenger(summary) = layout.summary else {
        panic!("a passenger preset laid out a freighter");
    };
    (layout.items, *summary)
}

fn seats_for(built: &Built, cap: Option<i64>) -> crate::build::PassengerCounts {
    let pax = &built.config.cabin.passenger;
    let layout = presets::get(&built.config.preset)
        .expect("registered preset")
        .reference
        .certified_exit_layout;
    simulate_passenger_counts_for_seat_mix_with_source_cap(
        &built.geometry,
        pax,
        &pax.length_share_mix(),
        cap,
        layout,
    )
}

#[test]
fn a_declared_cabin_runs_between_its_first_and_last_doors_and_no_row_blocks_a_door() {
    for name in ["B787-9", "AVE"] {
        let built = built(name, clean_sheet);
        let g = &built.geometry;
        assert_eq!(g.door_stations.len(), 4, "{name}: four declared door pairs");
        let main = cabin_deck_segments(g)[0];
        let (first, last) = (g.door_stations[0], g.door_stations[3]);
        assert!((main.x0 - (first.zone().0 - MONUMENT_BAY_LENGTH_M)).abs() < 1e-9);
        assert!((main.x1 - (last.zone().1 + MONUMENT_BAY_LENGTH_M)).abs() < 1e-9);

        let (items, summary) = layout(&built);
        assert_eq!(summary.unseated_pax, 0, "{name}");
        for row in items.iter().filter(|item| item.kind == ItemKind::SeatRow) {
            let (a, b) = (row.x - 0.5 * row.length, row.x + 0.5 * row.length);
            assert!(a >= first.zone().1 - 1e-9 && b <= last.zone().0 + 1e-9);
            for door in &g.door_stations {
                let (d0, d1) = door.zone();
                assert!(
                    b <= d0 + 1e-9 || a >= d1 - 1e-9,
                    "{name}: a row at {a:.2}..{b:.2} m blocks the door at {:.2} m",
                    door.x
                );
            }
        }
        // Every door the structure has is drawn, where the source puts it.
        let exits: Vec<f64> = items
            .iter()
            .filter(|item| item.kind == ItemKind::Exit)
            .map(|item| item.x)
            .collect();
        assert_eq!(exits.len(), 8, "{name}: two cut-outs per pair");
        for door in &g.door_stations {
            assert!(exits.iter().any(|x| (x - door.x).abs() < 1e-9));
        }
    }
}

#[test]
fn the_exit_ceiling_is_the_sum_of_the_declared_pair_ratings() {
    for name in ["B787-9", "AVE", "ATR72-600", "A220-300", "A320-200"] {
        let built = built(name, clean_sheet);
        let layout_ = presets::get(name)
            .unwrap()
            .reference
            .certified_exit_layout
            .unwrap();
        let (_, summary) = layout(&built);
        assert_eq!(
            summary.geometric_capacity,
            layout_rating(&layout_),
            "{name}"
        );
        assert!(summary.seated_pax <= summary.geometric_capacity, "{name}");
    }
    // Four Type A pairs: 4 x 110 (CS 25.807(g)(1)).
    let b787 = presets::get("B787-9").unwrap();
    assert_eq!(
        layout_rating(&b787.reference.certified_exit_layout.unwrap()),
        440
    );
}

#[test]
fn the_787_9_seats_boeings_typical_two_class_cabin_in_its_62_m_body() {
    // D6-58333 Rev Q section 2.1.2: 290 seats, 28 business and 262 economy.
    let built = built("B787-9", clean_sheet);
    assert!((built.geometry.fus_len - 62.00).abs() < 1e-6);
    let floor = seats_for(&built, None);
    assert!(
        floor.total() >= 290,
        "the 787-9 floor holds {} seats, below Boeing's typical 290",
        floor.total()
    );
    let typical = seats_for(&built, Some(290));
    assert_eq!(typical.total(), 290);
    // Whole rows of the generic business seat: within one row of 28.
    assert!(
        (typical.business - 28).abs() <= 6,
        "business {} against 28",
        typical.business
    );
}

#[test]
fn ave_holds_the_777_9_standard_two_class_426_at_its_printed_pitches() {
    // D6-86073 Rev G Table 2-1 note 1 and Figure 2-3: 42 business at 85 in,
    // 384 economy at 32 in, 10 abreast.
    let built = built("AVE", clean_sheet);
    let economy = &built.config.cabin.passenger.economy;
    assert!((economy.pitch_m - 32.0 * 0.0254).abs() < 1e-12);
    assert_eq!(economy.abreast, 10);
    let standard = seats_for(&built, Some(426));
    assert_eq!(standard.total(), 426);
    assert!(
        (standard.business - 42).abs() <= 6,
        "business {} against 42",
        standard.business
    );
}

#[test]
fn a_longer_body_never_seats_fewer_and_keeps_its_doors_in_order() {
    let base = built("B787-9", clean_sheet);
    let stretched = built("B787-9", |config, design| {
        clean_sheet(config, design);
        design.fuselage_length_m += 4.0;
    });
    assert_eq!(stretched.geometry.door_stations.len(), 4);
    // The first door keeps its nose distance and the last its tail distance.
    let (b, s) = (&base.geometry, &stretched.geometry);
    assert!((s.door_stations[0].x - b.door_stations[0].x).abs() < 1e-9);
    assert!(((s.x_max - s.door_stations[3].x) - (b.x_max - b.door_stations[3].x)).abs() < 1e-9);
    let (base_seats, stretched_seats) = (seats_for(&base, None), seats_for(&stretched, None));
    assert!(stretched_seats.total() >= base_seats.total());
}

#[test]
fn every_provisioned_monument_has_a_bay_clear_of_the_seats() {
    for name in ["B787-9", "AVE"] {
        let built = built(name, clean_sheet);
        let (items, summary) = layout(&built);
        let monuments: Vec<&DeckItem> = items
            .iter()
            .filter(|item| {
                matches!(
                    item.kind,
                    ItemKind::Galley | ItemKind::Lav | ItemKind::AccessibleLav
                )
            })
            .collect();
        assert_eq!(
            monuments.len() as i64,
            summary.galleys + summary.lavatories,
            "{name}"
        );
        for monument in &monuments {
            assert!(monument.width > 0.0, "{name}: a monument with no room");
            let (a, b) = (
                monument.x - 0.5 * monument.length,
                monument.x + 0.5 * monument.length,
            );
            for row in items.iter().filter(|item| item.kind == ItemKind::SeatRow) {
                let (r0, r1) = (row.x - 0.5 * row.length, row.x + 0.5 * row.length);
                assert!(
                    r1 <= a + 1e-9 || r0 >= b - 1e-9,
                    "{name}: a {:?} at {a:.2}..{b:.2} m overlaps a row at {r0:.2}..{r1:.2} m",
                    monument.kind
                );
            }
        }
    }
}

#[test]
fn the_atr_lays_out_its_72_seats_at_the_factsheet_pitch_with_baggage_fore_and_aft() {
    // ATR 72-600 factsheet p.22: 72 seats at 29 in pitch.
    let built = built("ATR72-600", |_, _| {});
    let (items, summary) = layout(&built);
    assert_eq!(summary.seated_pax, 72);
    assert_eq!(summary.unseated_pax, 0);
    let rows: Vec<&DeckItem> = items
        .iter()
        .filter(|item| item.kind == ItemKind::SeatRow)
        .collect();
    for row in &rows {
        assert!(
            (row.length - 29.0 * 0.0254).abs() < 1e-9,
            "pitch {}",
            row.length
        );
    }
    // The exits are rated for the 78-seat certified maximum.
    assert!(summary.geometric_capacity >= 78);
    // The floor the seats leave is the baggage's: compartments ahead of and
    // behind the cabin, both large enough to hold a bag.
    let g = &built.geometry;
    let extent = items
        .iter()
        .filter(|item| {
            matches!(
                item.kind,
                ItemKind::SeatRow | ItemKind::Galley | ItemKind::Lav | ItemKind::AccessibleLav
            )
        })
        .fold((f64::MAX, f64::MIN), |(a, b), item| {
            (
                a.min(item.x - 0.5 * item.length),
                b.max(item.x + 0.5 * item.length),
            )
        });
    let compartments = crate::cargo::derive_hold_compartments(g, &[], Some(extent), &[]);
    assert_eq!(compartments.len(), 2, "{compartments:?}");
    assert!(compartments[0].x_end_m <= extent.0 + 1e-9);
    assert!(compartments[1].x_start_m >= extent.1 - 1e-9);
}

#[test]
fn a_preset_without_door_stations_keeps_the_generic_cabin() {
    let built = built("A320-200", clean_sheet);
    let g = &built.geometry;
    assert!(g.door_stations.is_empty());
    let main = cabin_deck_segments(g)[0];
    assert!((main.x0 - (g.cabin_start_x + 0.5)).abs() < 1e-12);
    assert!((main.x1 - (g.cabin_end_x + 0.25 * g.tailcone_len)).abs() < 1e-12);
}
