// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Payload definitions of the v1.5 acceptance cases C2, C3 and C4: they must
//! build, balance and report. The mission legs of the cases belong to the
//! mission plan; this checks only the
//! payload side (zones, point masses, positions and the ledger).

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use alas_config::{
    presets, AlasConfig, FloorZoneConfig, MassCategory, PointMassConfig, SeatClassConfig, ZoneDeck,
    ZoneKind,
};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::builder::AircraftBuilder;
use alas_payload::build::product_cabin_geometry;
use alas_payload::declared_items::{format_ledger, ledger};
use alas_payload::{build_payload_layout, ItemKind, LayoutSummary};

fn built(name: &str) -> (AlasConfig, Airplane) {
    let preset = presets::get(name).unwrap();
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .unwrap();
    (config, plane)
}

fn zone(
    name: &str,
    kind: ZoneKind,
    x0: f64,
    area_m2: f64,
    width: f64,
    kg_m2: f64,
    occupants: i64,
) -> FloorZoneConfig {
    FloorZoneConfig {
        name: name.to_owned(),
        kind,
        deck: ZoneDeck::Main,
        x_start_m: x0,
        x_end_m: x0 + area_m2 / width,
        width_m: Some(width),
        y_center_m: 0.0,
        furnishing_kg_per_m2: kg_m2,
        total_mass_kg: None,
        occupants,
        carve_seat_floor: true,
        movable: false,
        x_range_m: None,
    }
}

fn point(id: &str, x: f64, y: f64, z: f64, kg: f64, cat: MassCategory) -> PointMassConfig {
    PointMassConfig {
        id: id.to_owned(),
        name: id.to_owned(),
        x_m: x,
        y_m: y,
        z_m: z,
        mass_kg: kg,
        category: cat,
        inertia_kgm2: None,
        releasable: false,
        drag_area_m2: None,
        movable: false,
        x_range_m: None,
    }
}

/// C2: presidential wide-body. Zones from the case (areas and densities as
/// stated), 40 business and 60 economy seats, satcom rack and two pods.
#[test]
fn declared_equipment_stays_out_of_the_payload_ledger_and_the_statement_builds() {
    let preset = presets::get("A320-200").unwrap();
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" })).unwrap();
    config.cabin.payload_items.point_masses =
        vec![point("rack", 8.0, 0.0, 0.5, 800.0, MassCategory::Equipment)];
    let report = alas_pipeline::FullAnalysis::new(config.clone())
        .run(&preset.design_vector, true)
        .unwrap();
    let layout = report.payload_layout.as_ref().unwrap();
    let items_kg: f64 = layout.payload_items().map(|item| item.mass).sum();
    assert!((items_kg - layout.total_mass).abs() < 1.0e-9);
    assert_eq!(layout.equipment_mass(), 800.0);
    let feasibility = alas_pipeline::feasibility::assess_physical_feasibility(
        &config,
        &preset.design_vector,
        &report,
        None,
    );
    assert!(feasibility.mass_balance.is_some());
}

#[test]
fn c2_presidential_layout_builds_and_balances() {
    let (mut config, plane) = built("B787-9");
    let g = product_cabin_geometry(&plane, &config).unwrap();
    // Forward to aft along the main deck, the 4.8 m wide rooms centred.
    config.cabin.payload_items.zones = vec![
        zone("Forward office", ZoneKind::Office, 8.0, 25.0, 4.8, 180.0, 4),
        zone(
            "Bedroom and bath",
            ZoneKind::Bedroom,
            13.5,
            30.0,
            4.8,
            150.0,
            2,
        ),
        zone("Conference", ZoneKind::Office, 20.0, 20.0, 4.8, 0.0, 10),
        zone("Ops and comms", ZoneKind::Ops, 25.0, 15.0, 4.8, 400.0, 3),
    ];
    let crown = g.zc_at(g.x_wing_le + 12.0) + 0.9 * g.internal_half_height(g.x_wing_le + 12.0);
    config.cabin.payload_items.point_masses = vec![
        point(
            "satcom",
            g.x_wing_le + 12.0,
            0.0,
            crown,
            450.0,
            MassCategory::Equipment,
        ),
        point(
            "pod-port",
            g.x_wing_le + 6.0,
            -14.0,
            -1.0,
            300.0,
            MassCategory::Equipment,
        ),
        point(
            "pod-starboard",
            g.x_wing_le + 6.0,
            14.0,
            -1.0,
            300.0,
            MassCategory::Equipment,
        ),
    ];
    config.cabin.passenger.class_mix_mode = "count".to_owned();
    config.cabin.passenger.business = SeatClassConfig {
        count: 40,
        ..config.cabin.passenger.business.clone()
    };
    config.cabin.passenger.economy = SeatClassConfig {
        count: 60,
        ..config.cabin.passenger.economy.clone()
    };
    config.cabin.passenger.first.count = 0;
    assert!(alas_config::validate(&config)
        .iter()
        .all(|issue| issue.field_path != "cabin.payload_items"));

    let layout = build_payload_layout(&plane, &config, 0.0, 0.0).unwrap();
    let LayoutSummary::Passenger(summary) = &layout.summary else {
        panic!("a passenger cabin");
    };
    assert_eq!(summary.seated_pax, 100, "40 business + 60 economy seated");
    // Furnishing: 25*180 + 30*150 + 15*400 = 15,000 kg, plus the three
    // equipment point masses.
    let furnishing = 25.0 * 180.0 + 30.0 * 150.0 + 15.0 * 400.0;
    assert!((layout.equipment_mass() - (furnishing + 450.0 + 600.0)).abs() < 1.0);
    // Occupants: 19 people at the load-case mass join the payload.
    let occupants =
        19.0 * (config.requirements.passenger_mass_kg - config.cabin.passenger.checked_bag_mass_kg);
    let zone_payload: f64 = layout
        .items
        .iter()
        .filter(|item| item.kind == ItemKind::Zone && item.mass > 0.0)
        .filter(|item| item.label.ends_with("(occupants)"))
        .map(|item| item.mass)
        .sum();
    assert!((zone_payload - occupants).abs() < 1e-9);
    // No seat row inside any room.
    for row in layout
        .items
        .iter()
        .filter(|item| item.kind == ItemKind::SeatRow && item.deck == "main")
    {
        let (a, b) = (row.x - 0.5 * row.length, row.x + 0.5 * row.length);
        for z in &config.cabin.payload_items.zones {
            assert!(
                b <= z.x_start_m + 1e-6 || a >= z.x_end_m - 1e-6,
                "{}",
                z.name
            );
        }
    }
    let table = format_ledger(&layout);
    assert!(table.contains("Forward office") && table.contains("satcom"));
    println!("{table}");
    assert!(ledger(&layout)
        .iter()
        .any(|line| line.category == "equipment"));
}

/// C4: a 1,000 kg drone carried at x = 0.55 L below the keel, released later.
#[test]
fn c4_drone_is_a_releasable_payload_point_mass_with_a_cg_jump() {
    let (mut config, plane) = built("A220-300");
    let g = product_cabin_geometry(&plane, &config).unwrap();
    let x = 0.55 * g.fus_len;
    let keel = g.zc_at(x) - 0.5 * g.height_at(x);
    let base = build_payload_layout(&plane, &config, 0.0, 0.0).unwrap();
    let mut drone = point("drone", x, 0.0, keel - 0.25, 1_000.0, MassCategory::Payload);
    drone.releasable = true;
    drone.drag_area_m2 = Some(0.3);
    config.cabin.payload_items.point_masses = vec![drone];
    let carried = build_payload_layout(&plane, &config, 0.0, 0.0).unwrap();
    assert!((carried.total_mass - base.total_mass - 1_000.0).abs() < 1e-9);
    // CG jump at release, from the items themselves: the moment of every
    // payload item (the base cabin's plus the drone) over the mass, computed
    // here without the layout's own balance function.
    let moment: f64 = base.items.iter().map(|i| i.mass * i.x).sum::<f64>() + 1_000.0 * x;
    let mass: f64 = base.items.iter().map(|i| i.mass).sum::<f64>() + 1_000.0;
    assert!((carried.cg_x - moment / mass).abs() < 1e-9);
    let jump = carried.cg_x - base.cg_x;
    assert!(
        jump > 0.0,
        "a drone aft of the payload CG moves it aft: {jump}"
    );
    println!("C4 payload CG jump at release: {jump:.3} m (drone at x = {x:.2} m)");
}

/// C3: nine 1,000 kg pallets on the A400M
/// main deck (Jane's: 7 on the floor and 2 on the ramp), not the sixteen the
/// case first asked for, released in two airdrop events (4, then 5). No ramp
/// is modelled yet, so what is asserted is nine main-deck positions the loader
/// finds, not which two are on the ramp (active follow-up). The ULD
/// loader packs positions to capacity, so the case's 1,000 kg pallets are
/// declared as releasable point masses on the nine main-deck positions the
/// loader found; the two release events act on the ids.
#[test]
fn c3_a400m_nine_pallets_are_declared_for_two_staged_drops() {
    let (mut config, plane) = built("A400M");
    config.requirements.aircraft_type = "cargo".to_owned();
    config.requirements.cargo_payload_kg = 36_000.0;
    let positions: Vec<_> = build_payload_layout(&plane, &config, 0.0, 0.0)
        .unwrap()
        .items
        .iter()
        .filter(|item| item.kind == ItemKind::Uld)
        .map(|item| (item.x, item.y, item.z))
        .collect();
    assert_eq!(
        positions.len(),
        9,
        "nine main-deck positions found by the loader"
    );
    config.requirements.cargo_payload_kg = 0.0;
    config.cabin.payload_items.point_masses = positions
        .iter()
        .enumerate()
        .map(|(n, &(x, y, z))| {
            let drop = if n < 4 { 1 } else { 2 };
            let mut pallet = point(
                &format!("drop{drop}-pallet{}", n + 1),
                x,
                y,
                z,
                1_000.0,
                MassCategory::Payload,
            );
            pallet.releasable = true;
            pallet.drag_area_m2 = Some(0.5);
            pallet
        })
        .collect();
    let loaded = build_payload_layout(&plane, &config, 0.0, 0.0).unwrap();
    assert!(
        (loaded.total_mass - 9_000.0).abs() < 1e-6,
        "{}",
        loaded.total_mass
    );
    println!("{}", format_ledger(&loaded));
}

/// A freighter main deck on a 747-class body is a configuration (cargo
/// aircraft type with the main deck enabled), not a preset hack.
#[test]
fn a_747_class_body_becomes_a_main_deck_freighter_by_configuration() {
    let (mut config, plane) = built("B747-400");
    config.requirements.aircraft_type = "cargo".to_owned();
    config.cabin.cargo.use_main_deck = true;
    let g = product_cabin_geometry(&plane, &config).unwrap();
    let manager = alas_payload::CargoLoadManager::new(&g, config.cabin.cargo.clone());
    let main = manager
        .slots
        .iter()
        .filter(|slot| slot.deck == "main")
        .count();
    println!("747-class main-deck freight positions: {main}");
    for s in manager.slots.iter().filter(|s| s.deck == "main") {
        println!(
            "slot {} x {:.2} y {:.2} h {:.2}",
            s.sid, s.x, s.y, s.realized_height_m
        );
    }
    // Boeing 747-400 Airport Planning D6-58326-1 rev F, fig. 2.4.5 (p. 2-26) draws 29
    // ten-foot containers on the 747-400F main deck: 13 rows of two across, one centre
    // position aft, and two quarter-turned ones, one in the nose and one in the tailcone
    // curve. The drawing is a schematic (its fuselage outline is 59 m long against the
    // 70.7 m airplane), so only the count and the arrangement are taken from it. The model
    // stands 29: a turned pallet in the nose, 13 rows (the first slid aft to the station
    // where the nose section takes two across), and the two aft centre positions.
    assert_eq!(main, 29, "main-deck positions");
}

/// The DC-10-30 freighter's published main deck is 30 positions of 88 x 108 in pallets
/// (Boeing, *McDonnell Douglas DC-10-30 Freighters*, TBC-PS-2038 p.8; main-deck cargo
/// arrangements TBC-CA-2066 p.5, which draws them 15 rows of two, the 108-in side across).
/// The format is the aircraft's published one, so it is named; the automatic selector
/// would take the larger 125-in pallets and stand 23.
#[test]
fn a_dc10_main_deck_with_its_published_88_by_108_pallets_stands_30_positions() {
    let (mut config, plane) = built("DC-10");
    config.requirements.aircraft_type = "cargo".to_owned();
    config.cabin.cargo.use_main_deck = true;
    config.cabin.cargo.main_deck_uld = "P88x108".to_owned();
    let g = product_cabin_geometry(&plane, &config).unwrap();
    let manager = alas_payload::CargoLoadManager::new(&g, config.cabin.cargo.clone());
    let main = manager
        .slots
        .iter()
        .filter(|slot| slot.deck == "main")
        .count();
    assert_eq!(main, 30, "main-deck positions");
}
