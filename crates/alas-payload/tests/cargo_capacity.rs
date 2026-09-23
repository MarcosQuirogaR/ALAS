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
    assert!(capacity.bulk_usable_volume_m3 <= capacity.bulk_nominal_volume_m3);
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
            capacity.bulk_usable_volume_m3 < capacity.bulk_nominal_volume_m3,
            "{name}"
        );
        for slot in &manager.slots {
            assert_eq!(slot.uld.code, "BLK", "{name}");
            assert!(slot.realized_height_m >= 0.9 && slot.realized_height_m < slot.uld.height);
            let ratio = slot.realized_height_m / slot.uld.height;
            assert!((slot.usable_volume_m3() - slot.uld.volume_m3 * ratio).abs() < 1e-9);
            assert!((slot.max_net() - slot.uld.max_net() * ratio).abs() < 1e-9);
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
    assert_eq!(config.requirements.num_passengers, 130);
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
    assert_eq!(summary.source_capacity_cap, Some(145));
    assert_eq!(summary.source_exit_layout, Some("C-III-C"));
    assert_eq!(summary.exit_type, "C-III-C");
    assert_eq!(summary.exit_pairs, 3);
    assert_eq!(summary.exit_capacity, 145);
    // The source sequence replaces the generic diameter proxy on the product
    // path.  Keep the registered 145-seat sum and the actual row-pass result
    // visible; this must not turn the source cap into a post-hoc count
    // truncation.
    assert_eq!(summary.geometric_capacity, 145);
    assert!(
        summary.geometric_capacity
            <= summary
                .source_capacity_cap
                .expect("the source exit layout caps the capacity")
    );
    assert_eq!(summary.max_certifiable_capacity, summary.geometric_capacity);
    assert_eq!(summary.capacity_binding, "source_exit_layout");
    assert_eq!(summary.total_pax, summary.geometric_capacity);
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
    g.lower_deck.floor_frac = -0.55;
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
