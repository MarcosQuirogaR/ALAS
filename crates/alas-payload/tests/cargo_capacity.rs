// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Capacity and occupancy regressions for physical cargo layouts.

// Failed construction or lookup is a failed test assertion.
#![allow(clippy::expect_used)]

use alas_config::{presets, AlasConfig, CargoDeckConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::{
    cabin::cabin_deck_segments, cabin::select_exit_type, CabinGeometry, CargoLoadManager,
};

#[test]
fn a_hold_narrower_than_the_pallet_takes_it_turned_and_stacked_to_its_height() {
    // C-295M: 2.36 m wide and 1.90 m high, against a 2.74 x 2.24 m, 2.44 m pallet.
    let preset = presets::get("C295").expect("registered preset");
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": "C295" }))
        .expect("preset configuration");
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("preset geometry");
    let layout = alas_payload::build_payload_layout(&plane, &config, 0.0, 0.0).expect("payload");
    let alas_payload::LayoutSummary::Cargo(summary) = &layout.summary else {
        panic!("the C295 has a freight hold");
    };
    assert!(summary.n_slots >= 5, "{} positions", summary.n_slots);
    assert!(
        (summary.loaded_net_payload_t - config.requirements.cargo_payload_kg / 1000.0).abs() < 1e-6,
        "{} t loaded",
        summary.loaded_net_payload_t
    );
    let per_position = summary.volume_m3 / summary.n_slots as f64;
    assert!((per_position - 13.6 * 1.90 / 2.44).abs() < 1e-9);
}

/// Seated passengers, exit type, exit pairs and exit capacity of a preset's cabin.
fn exits_of(name: &str) -> (i64, &'static str, i64, i64) {
    let preset = presets::get(name).expect("registered preset");
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
        .expect("preset configuration");
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("preset geometry");
    let layout =
        alas_payload::build_payload_layout(&plane, &config, 0.0, 0.0).expect("preset payload");
    let alas_payload::LayoutSummary::Passenger(summary) = layout.summary else {
        panic!("{name} must use a passenger layout");
    };
    (
        summary.seated_pax,
        summary.exit_type,
        summary.exit_pairs,
        summary.exit_capacity,
    )
}

#[test]
fn a_declared_exit_arrangement_is_installed_whole_however_few_seats_are_filled() {
    // Both cabins sit well below the ceiling of their four declared pairs.
    for name in ["A320-200", "E195-E2"] {
        let layout = presets::get(name)
            .expect("registered preset")
            .reference
            .certified_exit_layout
            .expect("a declared arrangement");
        let (seated, _, pairs, capacity) = exits_of(name);
        assert!(
            seated < alas_payload::cabin::layout_rating(&layout),
            "{name}"
        );
        assert_eq!(
            (pairs, capacity),
            (4, alas_payload::cabin::layout_rating(&layout)),
            "{name}"
        );
    }
}

#[test]
fn the_lj60_installs_its_sourced_exits_not_the_small_cabin_default() {
    // NTSB AAR-10/02 s.1.15.3 p.17: a Type I door left, a Type III hatch
    // right: one pair rated by its smaller exit, Type III. The table rates it
    // 35, but one exit in each side seats 19 (CS 25.807(g)(3)).
    let (_, exit_type, pairs, capacity) = exits_of("LJ60");
    assert_eq!((exit_type, pairs, capacity), ("III", 1, 19));
}

fn geometry(name: &str) -> CabinGeometry {
    let preset = presets::get(name).expect("registered preset");
    let config =
        AlasConfig::from_value(&serde_json::json!({"preset": name})).expect("preset configuration");
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("preset geometry");
    CabinGeometry::new(
        &plane,
        &config.geometry,
        config.cabin.passenger.wall_thickness_m,
    )
    .expect("preset cabin geometry")
}

#[test]
fn available_capacity_does_not_depend_on_dispatched_cargo_mass() {
    let g = geometry("B787-9");
    let mut manager = CargoLoadManager::new(
        &g,
        CargoDeckConfig {
            use_main_deck: false,
            ..Default::default()
        },
    );
    let empty = manager.capacity_summary();
    assert!(empty.uld_positions > 0);
    assert_eq!(manager.mass_props().0, 0.0);
    manager.solve(
        1_000.0,
        g.x_wing_ac,
        &|slot| (slot.x - g.x_wing_ac).abs(),
        true,
    );
    assert!(manager.mass_props().0 > 0.0);
    assert_eq!(empty, manager.capacity_summary());
    assert!(
        manager
            .slots
            .iter()
            .filter(|slot| slot.payload > 1.0)
            .count()
            < empty.uld_positions
    );
}

#[test]
fn bulk_capacity_is_not_reported_as_uld_capacity() {
    let g = geometry("B787-9");
    let manager = CargoLoadManager::new(
        &g,
        CargoDeckConfig {
            use_main_deck: false,
            lower_deck_uld: "BLK".to_owned(),
            ..Default::default()
        },
    );
    let capacity = manager.capacity_summary();
    assert!(capacity.bulk_positions > 0);
    assert_eq!(capacity.uld_positions, 0);
    assert_eq!(capacity.container_internal_volume_m3, 0.0);
    assert!(capacity.bulk_nominal_volume_m3 > 0.0);
    assert!(capacity.bulk_usable_volume_m3 > 0.0);
    // Loose freight fills the compartments it is loaded into: its usable volume is the
    // stowage share of the enclosed hold volume, not the nominal blocks.
    assert!(
        (capacity.bulk_usable_volume_m3
            - alas_payload::cargo::BULK_STOWAGE_FACTOR * capacity.hold_volume_m3)
            .abs()
            < 1e-6
    );
}

#[test]
fn shallow_bulk_slots_scale_volume_and_mass_with_realized_height() {
    for name in ["A220-300", "A320-200"] {
        let g = geometry(name);
        let mut manager = CargoLoadManager::new(
            &g,
            CargoDeckConfig {
                use_main_deck: false,
                lower_deck_uld: "BLK".to_owned(),
                ..Default::default()
            },
        );
        let capacity = manager.capacity_summary();
        assert!(capacity.bulk_positions > 0, "{name}");
        assert_eq!(capacity.uld_positions, 0, "{name}");
        assert!(
            (capacity.bulk_usable_volume_m3
                - alas_payload::cargo::BULK_STOWAGE_FACTOR * capacity.hold_volume_m3)
                .abs()
                < 1e-6,
            "{name}"
        );
        for slot in &manager.slots {
            assert_eq!(slot.uld.code, "BLK", "{name}");
            // A bulk position is realized below its nominal block height in the
            // shallow narrowbody holds.
            assert!(slot.realized_height_m >= 0.5 && slot.realized_height_m < slot.uld.height);
            assert!(slot.realized_width_m > 0.0 && slot.realized_width_m <= slot.uld.width);
            let ratio = (slot.realized_height_m / slot.uld.height)
                * (slot.realized_width_m / slot.uld.width);
            // A shallow position holds the smaller of the nominal block scaled by its
            // clearance and its share of its compartment's volume at the stowage density
            // (ICAO 161 kg/m3): the nominal block (2,000 kg in 3.0 m3, 667 kg/m3) is denser
            // than any baggage or freight stows, and a loose block conforms to the hold.
            // This hold is all loose bulk, so a block's share of its compartment is the
            // compartment's volume times the block's length over the bulk length.
            let compartments =
                alas_payload::cargo::derive_hold_compartments(&g, &manager.slots, None, &[]);
            let compartment = compartments
                .iter()
                .find(|c| c.contains(slot.x))
                .expect("every block stands in a compartment");
            let bulk_length: f64 = manager
                .slots
                .iter()
                .filter(|s| compartment.contains(s.x))
                .map(|s| s.uld.length)
                .sum();
            let share =
                alas_payload::cargo::BULK_STOWAGE_FACTOR * compartment.volume_m3 * slot.uld.length
                    / bulk_length
                    * alas_payload::cargo::STOWAGE_DENSITY_KG_M3;
            assert!(
                (slot.usable_volume_m3() * alas_payload::cargo::STOWAGE_DENSITY_KG_M3 - share)
                    .abs()
                    < 1e-6,
                "{name}"
            );
            let expected = (slot.uld.max_net() * ratio).min(share);
            assert!((slot.max_net() - expected).abs() < 1e-9, "{name}");
        }
        manager.solve(1.0e9, g.x_wing_ac, &|slot| slot.x, true);
        let loaded: f64 = manager.slots.iter().map(|slot| slot.payload).sum();
        assert!((loaded - capacity.net_capacity_kg).abs() < 1e-6, "{name}");
    }
}

#[test]
fn a_loaded_bulk_hold_has_mass_but_no_containers() {
    let g = geometry("B787-9");
    let config = CargoDeckConfig {
        use_main_deck: false,
        lower_deck_uld: "BLK".to_owned(),
        ..Default::default()
    };
    let req = alas_config::DesignRequirements {
        cargo_payload_kg: 1_000.0,
        ..Default::default()
    };
    let layout = alas_payload::build_cargo_layout(&g, &config, &req, 0.0, 0.0);
    assert!((layout.total_mass - 1_000.0).abs() < 1.0);
    assert!(layout
        .items
        .iter()
        .all(|item| item.meta == alas_payload::ItemMeta::BulkBag));
    let alas_payload::LayoutSummary::Cargo(summary) = layout.summary else {
        panic!("cargo load must have cargo summary");
    };
    assert_eq!(summary.n_ulds, 0);
    assert_eq!(summary.tare_mass_t, 0.0);
}

#[test]
fn physical_hold_positions_never_double_book_the_bulk_footprint() {
    for name in presets::available() {
        let g = geometry(name);
        let manager = CargoLoadManager::new(
            &g,
            CargoDeckConfig {
                use_main_deck: false,
                ..Default::default()
            },
        );
        for (index, first) in manager.slots.iter().enumerate() {
            for second in manager.slots.iter().skip(index + 1) {
                let x_overlap = (first.x - second.x).abs()
                    < (first.uld.length + second.uld.length) * 0.5 - 1e-9;
                let y_overlap =
                    (first.y - second.y).abs() < (first.uld.width + second.uld.width) * 0.5 - 1e-9;
                assert!(
                    first.deck != second.deck || !x_overlap || !y_overlap,
                    "{name}: {} overlaps {}",
                    first.sid,
                    second.sid
                );
            }
        }
    }
}

#[test]
fn a220_passenger_baggage_uses_the_configured_bulk_only_system() {
    let preset = presets::get("A220-300").expect("registered A220");
    let config = AlasConfig::from_value(&serde_json::json!({"preset": "A220-300"}))
        .expect("A220 configuration");
    assert_eq!(config.cabin.cargo.lower_deck_uld, "BLK");
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("A220 geometry");
    let layout =
        alas_payload::build_payload_layout(&plane, &config, 0.0, 0.0).expect("A220 payload");
    assert!(!layout
        .items
        .iter()
        .any(|item| matches!(item.meta, alas_payload::ItemMeta::Container(_))));
    let bulk_mass: f64 = layout
        .items
        .iter()
        .filter(|item| item.meta == alas_payload::ItemMeta::BulkBag)
        .map(|item| item.mass)
        .sum();
    let alas_payload::LayoutSummary::Passenger(summary) = layout.summary else {
        panic!("passenger summary");
    };
    assert!(bulk_mass > 0.0);
    assert_eq!(summary.hold_ulds, 0);
    assert!((bulk_mass - 1000.0 * (summary.bag_mass_t + summary.belly_cargo_t)).abs() < 1.0);
    let g = geometry("A220-300");
    let manager = CargoLoadManager::new(
        &g,
        CargoDeckConfig {
            use_main_deck: false,
            ..config.cabin.cargo.clone()
        },
    );
    assert_eq!(manager.capacity_summary().uld_positions, 0);
    assert_eq!(manager.capacity_summary().container_internal_volume_m3, 0.0);
}

#[test]
fn a220_registered_source_capacity_is_applied_during_row_allocation() {
    let preset = presets::get("A220-300").expect("registered A220");
    let config = AlasConfig::from_value(&serde_json::json!({"preset": "A220-300"}))
        .expect("A220 configuration");
    assert_eq!(config.requirements.num_passengers, 140);
    assert_eq!(config.cabin.passenger.class_mix_mode, "percent");
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("A220 geometry");
    let cabin = CabinGeometry::new(
        &plane,
        &config.geometry,
        config.cabin.passenger.wall_thickness_m,
    )
    .expect("A220 cabin geometry");
    let segments = cabin_deck_segments(&cabin);
    assert_eq!(segments.len(), 1);
    assert_eq!(select_exit_type(cabin.diameter_m).name, "III");
    let deck_length_m = segments[0].x1 - segments[0].x0;
    assert!((deck_length_m - 29.75).abs() < 0.01);
    let layout = alas_payload::build_payload_layout(&plane, &config, 0.0, 0.0)
        .expect("A220 source-capped payload");
    let alas_payload::LayoutSummary::Passenger(summary) = &layout.summary else {
        panic!("A220 registered preset must use passenger layout");
    };

    assert_eq!(preset.reference.certified_max_seats, Some(145));
    assert_eq!(summary.source_capacity_cap, Some(140));
    assert_eq!(summary.source_exit_layout, Some("C-III-C"));
    assert_eq!(summary.exit_type, "C-III-C");
    assert_eq!(summary.exit_pairs, 3);
    assert_eq!(summary.exit_capacity, 145);
    // The source sequence replaces the generic diameter proxy on the product
    // path.  Keep the registered 145-seat sum and the actual row-pass result
    // visible; this must not turn the source cap into a post-hoc count
    // truncation.
    assert_eq!(summary.geometric_capacity, 145);
    // The fixed-aircraft basis further caps the layout at the 140-seat planning
    // cabin, below the 145-seat exit sum, so the row pass stops there.
    assert!(
        summary
            .source_capacity_cap
            .expect("the source caps the capacity")
            <= summary.geometric_capacity
    );
    assert_eq!(summary.max_certifiable_capacity, 140);
    assert_eq!(summary.capacity_binding, "source_exit_layout");
    assert_eq!(summary.total_pax, summary.max_certifiable_capacity);
    assert_eq!(summary.seated_pax, summary.total_pax);
    assert_eq!(summary.unseated_pax, 0);

    // The cap is enforced by the same row pass that creates the mass-bearing
    // items.  This guards against reporting 145 after laying out a larger
    // geometry and truncating only the summary count.
    let row_seats: i64 = layout
        .items
        .iter()
        .filter_map(|item| match &item.meta {
            alas_payload::ItemMeta::Seat(meta) => Some(meta.filled),
            _ => None,
        })
        .sum();
    assert_eq!(row_seats, summary.seated_pax);
    let exit_types: Vec<&str> = layout
        .items
        .iter()
        .filter_map(|item| match &item.meta {
            alas_payload::ItemMeta::Exit(meta) => Some(meta.exit_type),
            _ => None,
        })
        .collect();
    assert_eq!(exit_types, vec!["C", "C", "III", "III", "C", "C"]);

    // CS-25.807's adjacent-exit spacing criterion is 18.3 m for the
    // applicable source arrangement.  The model emits two physical doors at
    // each pair station; collapse those coincident x coordinates before
    // checking the longitudinal pair spacing.  This is a geometry/layout
    // check only and does not claim the full evacuation demonstration.
    let mut pair_stations: Vec<f64> = layout
        .items
        .iter()
        .filter_map(|item| match &item.meta {
            alas_payload::ItemMeta::Exit(_) => Some(item.x),
            _ => None,
        })
        .collect();
    pair_stations.sort_by(f64::total_cmp);
    pair_stations.dedup_by(|a, b| (*a - *b).abs() < 1.0e-9);
    assert_eq!(pair_stations.len(), 3);
    assert!(pair_stations
        .windows(2)
        .all(|window| window[1] - window[0] <= 18.3 + 1.0e-9));
    let row_mass_kg: f64 = layout
        .items
        .iter()
        .filter(|item| item.kind == alas_payload::ItemKind::SeatRow)
        .map(|item| item.mass)
        .sum();
    assert!((row_mass_kg - 1_000.0 * summary.seat_mass_t).abs() < 1.0e-9);
}

#[test]
fn a_shallow_bulk_only_hold_cannot_fall_back_to_a_container_system() {
    let mut g = geometry("B787-9");
    // A hold 1.3 m high under the same ceiling: tall enough for the 1.14 m
    // LD3-45, too shallow for the 1.5 m bulk block.
    let x_mid = 0.5 * (g.cabin_start_x + g.cabin_end_x);
    g.lower_deck.floor_frac = g.lower_deck.ceil_frac - 1.3 / g.internal_half_height(x_mid);
    let container_manager = CargoLoadManager::new(
        &g,
        CargoDeckConfig {
            use_main_deck: false,
            lower_deck_uld: "LD3-45".to_owned(),
            ..Default::default()
        },
    );
    assert!(container_manager.capacity_summary().uld_positions > 0);
    let bulk_manager = CargoLoadManager::new(
        &g,
        CargoDeckConfig {
            use_main_deck: false,
            lower_deck_uld: "BLK".to_owned(),
            ..Default::default()
        },
    );
    assert_eq!(bulk_manager.lower_uld.code, "BLK");
    assert_eq!(bulk_manager.capacity_summary().uld_positions, 0);
}

#[test]
fn a_summary_reports_the_installed_holds_however_little_this_load_fills() {
    // B787-9: ACAP section 2.6.2 draws 20 + 16 LD-3 positions; the registered cabin's
    // baggage loads far fewer of them.
    let preset = presets::get("B787-9").expect("registered preset");
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": "B787-9" }))
        .expect("preset configuration");
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("preset geometry");
    let layout = alas_payload::build_payload_layout(&plane, &config, 0.0, 0.0).expect("payload");
    let alas_payload::LayoutSummary::Passenger(summary) = &layout.summary else {
        panic!("the B787-9 preset is a passenger cabin");
    };
    let installed = summary.hold_capacity;
    assert!(summary.hold_ulds < installed.uld_positions as i64);
    assert_eq!(installed.main_deck_uld_positions, 0);
    assert_eq!(installed.lower_deck_uld_positions, installed.uld_positions);
    assert_eq!(installed.uld_positions, 36);
    // The enclosed compartment volume is more than the containers and the
    // bulk position hold; the usable volume is those two together.
    assert!(installed.hold_volume_m3 > installed.usable_volume_m3());
    assert!(installed.bulk_usable_volume_m3 > 0.0);
    assert_eq!(
        installed.usable_volume_m3(),
        installed.container_usable_volume_m3 + installed.bulk_usable_volume_m3
    );
}

#[test]
fn a_freighter_summary_counts_main_and_lower_deck_positions_apart() {
    let preset = presets::get("B747-400").expect("registered preset");
    let mut value = serde_json::json!({ "preset": "B747-400" });
    value["requirements"] =
        serde_json::json!({ "aircraft_type": "cargo", "cargo_payload_kg": 1000.0 });
    let config = AlasConfig::from_value(&value).expect("freighter configuration");
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("preset geometry");
    let layout = alas_payload::build_payload_layout(&plane, &config, 0.0, 0.0).expect("payload");
    let alas_payload::LayoutSummary::Cargo(summary) = &layout.summary else {
        panic!("a cargo configuration lays out a freighter deck");
    };
    let installed = summary.installed;
    assert!(installed.main_deck_uld_positions > 0);
    assert!(installed.lower_deck_uld_positions > 0);
    assert_eq!(
        installed.main_deck_uld_positions + installed.lower_deck_uld_positions,
        installed.uld_positions
    );
    // One tonne loads a handful of positions; the installed count does not follow it.
    assert!((summary.n_ulds as usize) < installed.uld_positions);
}

#[test]
fn each_deck_installs_its_own_declared_exit_pairs() {
    // 747-400: five Type A pairs on the main deck and one on the upper deck
    // (EASA TCDS IM.A.196 III.20, 660). A380-800: five and three (880, above
    // the certified 868). A350-900: four (440). The upper deck must not repeat
    // the main deck's pairs.
    for (name, pairs, capacity) in [
        ("B747-400", 6, 660),
        ("A380-800", 8, 880),
        ("A350-900", 4, 440),
    ] {
        let (_, _, installed_pairs, installed_capacity) = exits_of(name);
        assert_eq!(
            (installed_pairs, installed_capacity),
            (pairs, capacity),
            "{name}"
        );
    }
}

/// The main-deck positions of a preset flown as a freighter, as installed.
fn main_deck_slots(name: &str) -> Vec<alas_payload::cargo::CargoSlot> {
    main_deck_slots_of(name, None)
}

/// [`main_deck_slots`] with the main-deck ULD named instead of the preset's.
fn main_deck_slots_of(
    name: &str,
    main_deck_uld: Option<&str>,
) -> Vec<alas_payload::cargo::CargoSlot> {
    let preset = presets::get(name).expect("registered preset");
    let mut value = serde_json::json!({ "preset": name });
    value["requirements"] = serde_json::json!({ "aircraft_type": "cargo" });
    value["cabin"] = serde_json::json!({ "cargo": { "lower_deck_uld": "AUTO" } });
    if let Some(uld) = main_deck_uld {
        value["cabin"]["cargo"]["main_deck_uld"] = serde_json::json!(uld);
    }
    let config = AlasConfig::from_value(&value).expect("freighter configuration");
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("preset geometry");
    let geometry = alas_payload::build::product_cabin_geometry(&plane, &config).expect("cabin");
    let manager = CargoLoadManager::new(
        &geometry,
        CargoDeckConfig {
            use_main_deck: true,
            ..config.cabin.cargo.clone()
        },
    );
    manager
        .slots
        .into_iter()
        .filter(|slot| slot.deck == "main")
        .collect()
}

#[test]
fn a_narrowbody_freighter_turns_its_pallets_across_the_deck() {
    // A320 section: a 125-inch side fits across the 3.6 m floor once the
    // netted load follows the crown, and the 88-inch side down the deck gives
    // more rows than the 125-inch side lengthwise (7 positions).
    let slots = main_deck_slots("A320-200");
    assert!(slots.len() >= 10, "{} positions", slots.len());
    // EFW A320/A321P2F brochure: an 85 in high main-deck door bounds the build.
    let door_m = 85.0 * 0.0254;
    for slot in &slots {
        assert!(slot.uld.width > slot.uld.length, "{} lengthwise", slot.sid);
        assert!(slot.realized_height_m <= door_m + 1e-12);
        assert!(
            slot.realized_height_m > slot.uld.height,
            "{} built up",
            slot.sid
        );
    }
}

#[test]
fn a_widebody_freighter_builds_its_pallets_up_to_the_section_and_the_door() {
    // 747-400 ACAP section 2.7.9: the 120 in (3.05 m) high side cargo door
    // bounds the build; the crown takes the outboard edges of the pallets two
    // abreast below that, so each is built higher than the 1.63 m lower-deck
    // contour and lower than the door.
    let door_m = 120.0 * 0.0254;
    let slots = main_deck_slots("B747-400");
    for slot in &slots {
        assert!(slot.realized_height_m > slot.uld.height, "{}", slot.sid);
        assert!(slot.realized_height_m <= door_m + 1e-12, "{}", slot.sid);
    }
    let volume: f64 = slots.iter().map(|slot| slot.usable_volume_m3()).sum();
    let nominal: f64 = slots.iter().map(|slot| slot.uld.volume_m3).sum();
    assert!(volume > 1.5 * nominal, "{volume:.1} of {nominal:.1} m3");
}

#[test]
fn a_tapering_main_deck_row_keeps_one_pallet_on_the_centre_line() {
    // 747-400 nose: the section ahead of the wing is too narrow for two
    // pallets side by side, not for one.
    let slots = main_deck_slots("B747-400");
    let first_x = slots
        .iter()
        .map(|slot| slot.x)
        .fold(f64::INFINITY, f64::min);
    let first_row: Vec<_> = slots.iter().filter(|slot| slot.x == first_x).collect();
    assert_eq!(first_row.len(), 1);
    assert!(first_row[0].y.abs() < 1e-12);
    assert!(slots.len() >= 25, "{} positions", slots.len());
}

#[test]
fn the_published_compartment_loads_sum_to_the_hold_structural_limit() {
    // B787-9: EASA TCDS IM.A.115 III.21, 32,005 + 25,655 + 2,735 kg.
    // A380-800: EASA TCDS EASA.A.110 III.3.10, 28,577 + 20,310 + 2,515 kg.
    // A320-200: EASA TCDS EASA.A.064 item 21, 3,402 + 4,536 + 1,497 kg.
    // A340-300: EASA TCDS EASA.A.015 item 19, 22,861 + 18,507 + 3,468 kg.
    // A350-900: EASA TCDS EASA.A.151 III.2.6, 22,000 + 19,000 + 3,468 kg.
    for (name, limit_kg) in [
        ("B787-9", 60_395.0),
        ("A380-800", 51_402.0),
        ("A320-200", 9_435.0),
        ("A340-300", 44_836.0),
        ("A350-900", 44_468.0),
    ] {
        let preset = presets::get(name).expect("registered preset");
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
            .expect("preset configuration");
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .expect("preset geometry");
        let layout =
            alas_payload::build_payload_layout(&plane, &config, 0.0, 0.0).expect("payload");
        let alas_payload::LayoutSummary::Passenger(summary) = &layout.summary else {
            panic!("{name} is a passenger cabin");
        };
        assert_eq!(
            summary.hold_capacity.structural_limit_kg,
            Some(limit_kg),
            "{name}"
        );
    }
}

#[test]
fn a_cabin_without_cabin_crew_seats_its_passengers_in_one_class() {
    // The LJ60's 5.38 m cabin, certified for eight (EASA TCDS IM.A.212
    // III.9): split 15/85 into business and economy, the class bay left room
    // for one seat. Eight seats need no cabin crew (ORO.CC.100(a)), so the
    // cabin is one class and seats all eight.
    let (main, upper, _, crew) = crew_of("LJ60", 0.0);
    assert_eq!((main, upper, crew), (8, 0, 0));
}

/// Seats per deck, upper-deck seats in rows forward of `upper_station_m`, and
/// the minimum cabin crew of a preset's passenger cabin.
fn crew_of(name: &str, upper_station_m: f64) -> (i64, i64, i64, i64) {
    let preset = presets::get(name).expect("registered preset");
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
        .expect("preset configuration");
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("preset geometry");
    let layout =
        alas_payload::build_payload_layout(&plane, &config, 0.0, 0.0).expect("preset payload");
    let seats_on = |deck: &str| -> i64 {
        layout
            .items
            .iter()
            .filter(|item| item.deck == deck)
            .filter_map(|item| match &item.meta {
                alas_payload::layout::ItemMeta::Seat(seat) => Some(seat.filled),
                _ => None,
            })
            .sum()
    };
    let (main, upper) = (seats_on("main"), seats_on("upper"));
    let upper_forward: i64 = layout
        .items
        .iter()
        .filter(|item| item.deck == "upper" && item.x < upper_station_m)
        .filter_map(|item| match &item.meta {
            alas_payload::layout::ItemMeta::Seat(seat) => Some(seat.filled),
            _ => None,
        })
        .sum();
    let alas_payload::LayoutSummary::Passenger(summary) = layout.summary else {
        panic!("{name} must use a passenger layout");
    };
    (main, upper, upper_forward, summary.min_cabin_crew)
}

#[test]
fn each_passenger_deck_is_crewed_to_its_own_type_certificate_row() {
    // EASA TCDS A.110 item 3.9: A380 upper deck six up to 300 seats, main
    // deck ten up to 500, each never below one per 50 seats on the deck
    // (ORO.CC.100(b)); one per 50 over the whole aircraft would give twelve.
    // The six take one more at the forward stair when more than 30 seats
    // stand forward of door U1, 20.94 m aft of the nose (A380 AC 2-7-0).
    let (main, upper, forward, crew) = crew_of("A380-800", 20.94);
    assert!(upper > 0 && upper <= 300 && main <= 500, "{main} + {upper}");
    let per_deck = |seats: i64, established: i64| established.max((seats + 49) / 50);
    let stair = i64::from(forward > 30);
    assert_eq!(crew, per_deck(main, 10) + per_deck(upper, 6 + stair));
    assert!(crew > (main + upper + 49) / 50);
    // EASA TCDS A.064: the basic A320 keeps four at 150 seats; three needs
    // modification 150364.
    let (main, upper, _, crew) = crew_of("A320-200", 0.0);
    assert_eq!((main, upper, crew), (150, 0, 4));
    // ORO.CC.100(a): no cabin crew on a cabin certified for at most 19 seats.
    assert_eq!(crew_of("LJ60", 0.0).3, 0);
}

#[test]
fn the_lower_hold_loads_do_not_bound_a_freighter_main_deck() {
    // The A320 compartment loads (EASA TCDS A.064) bound its lower holds;
    // the converted freighter's main-deck positions are outside them.
    let preset = presets::get("A320-200").expect("registered preset");
    let mut value = serde_json::json!({ "preset": "A320-200" });
    value["requirements"] =
        serde_json::json!({ "aircraft_type": "cargo", "cargo_payload_kg": 1000.0 });
    value["cabin"] = serde_json::json!({ "cargo": { "lower_deck_uld": "AUTO" } });
    let config = AlasConfig::from_value(&value).expect("freighter configuration");
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("preset geometry");
    let layout = alas_payload::build_payload_layout(&plane, &config, 0.0, 0.0).expect("payload");
    let alas_payload::LayoutSummary::Cargo(summary) = &layout.summary else {
        panic!("a cargo configuration lays out a freighter deck");
    };
    assert!(summary.installed.main_deck_uld_positions > 0);
    assert_eq!(summary.installed.structural_limit_kg, None);
}

#[test]
fn a_built_up_pallet_holds_its_load_area_times_its_built_height() {
    // The load stands 2.5 in inside each edge of the base (P6P usable loading
    // area 120 by 91 in on 125 by 96 in) and follows the section over it: its
    // volume is that area times the mean height it is built to, not the
    // lower-deck contour volume scaled by height.
    let margin = 2.5 * 0.0254;
    for name in ["B747-400", "DC-10", "A320-200"] {
        let slots = main_deck_slots(name);
        assert!(!slots.is_empty(), "{name}");
        for slot in &slots {
            let area = (slot.uld.length - 2.0 * margin) * (slot.uld.width - 2.0 * margin);
            let expected = area * slot.realized_height_m;
            assert!(
                (slot.usable_volume_m3() - expected).abs() < 1.0e-9,
                "{name} {}: {} against {expected}",
                slot.sid,
                slot.usable_volume_m3()
            );
        }
    }
    // Worked by hand on the DC-10 freighter's constant circular section:
    // 6.02 m across less a 0.15 m wall leaves a 2.86 m inner radius about
    // z = 0.20 m, over a main floor at z = -0.19 m. The P6P outboard of the
    // centre line, at y = -1.245 m and 2.44 m across, loads 2.313 m of it from
    // y = -2.4015 to -0.0885 m. The lining stands 0.39 + sqrt(2.86^2 - y^2) m
    // above the floor, 1.943 m at the outboard edge of the load; the 96 in
    // main-deck load contour (2.438 m, TBC-CA-2067) caps it inboard of
    // |y| = 1.996 m. The mean of that profile across the load is 2.398 m,
    // on 3.053 by 2.313 m: 16.936 m3.
    let slots = main_deck_slots_of("DC-10", Some("P6P"));
    let slot = slots
        .iter()
        .find(|slot| slot.uld.code == "P6P" && (slot.x - 15.05).abs() < 0.01 && slot.y < -1.0)
        .expect("the DC-10 freighter's outboard P6P at 15.05 m");
    assert!(
        (slot.realized_height_m - 2.398).abs() < 0.005,
        "{}: built to {} m",
        slot.sid,
        slot.realized_height_m
    );
    assert!(
        (slot.usable_volume_m3() - 16.936).abs() < 0.04,
        "{}: {} m3",
        slot.sid,
        slot.usable_volume_m3()
    );
}

/// Boeing 747-400 ACAP D6-58326-1 Rev F sections 2.6.1 (p.2-32) and 2.6.3
/// (p.2-34): the forward and the aft lower hold each take "8 full-width or 16
/// half-width (LD1)" containers, so 32 LD1 positions in the passenger and in
/// the freighter. Numerical verification of the declared holds; the declared
/// stations carry a read uncertainty of about 0.3 m.
#[test]
fn the_747_400_lower_holds_install_the_printed_16_half_width_positions_each() {
    let holds = alas_config::presets::declared_lower_holds("B747-400").expect("declared holds");
    let rows: Vec<_> = holds.iter().filter_map(|h| h.container_rows).collect();
    assert_eq!(rows, [8, 8], "forward and aft container holds, bulk apart");
    for freighter in [false, true] {
        let preset = presets::get("B747-400").expect("registered preset");
        let mut value = serde_json::json!({ "preset": "B747-400" });
        if freighter {
            value["requirements"] =
                serde_json::json!({ "aircraft_type": "cargo", "cargo_payload_kg": 1000.0 });
        }
        let config = AlasConfig::from_value(&value).expect("configuration");
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .expect("preset geometry");
        let layout =
            alas_payload::build_payload_layout(&plane, &config, 0.0, 0.0).expect("payload");
        let lower = match &layout.summary {
            alas_payload::LayoutSummary::Cargo(s) => s.installed.lower_deck_uld_positions,
            alas_payload::LayoutSummary::Passenger(s) => s.hold_capacity.lower_deck_uld_positions,
            #[allow(unreachable_patterns)]
            _ => panic!("unexpected summary"),
        };
        assert_eq!(lower, 2 * 8 * 2, "freighter = {freighter}");
    }
}
