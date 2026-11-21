// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// These regression helpers assert that their constructed fixtures are valid.
#![allow(clippy::expect_used)]

use super::*;
use alas_geom::builder::AircraftBuilder;
use alas_payload::build::build_payload_layout;

fn preset_case(name: &str) -> (AlasConfig, Airplane, PayloadLayout) {
    let preset = alas_config::presets::get(name).expect("registered preset");
    let config =
        AlasConfig::from_value(&serde_json::json!({ "preset": name })).expect("preset config");
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("preset geometry");
    let layout = build_payload_layout(&plane, &config, 0.0, 0.0).expect("preset layout");
    (config, plane, layout)
}

fn texts(scene: &Scene) -> Vec<&str> {
    scene
        .elements
        .iter()
        .filter_map(|element| match element {
            SceneElement::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn section_contains_shell_seats_floors_and_labels() {
    let config = AlasConfig::default();
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(None, true)
        .expect("default aircraft builds");
    let layout =
        build_payload_layout(&plane, &config, 0.0, 0.0).expect("default passenger layout builds");
    let scene = figure_cabin_cross_section(&layout, &plane, &config, Some("dark"));

    assert!(scene.elements.len() > 20);
    assert!(texts(&scene).contains(&"Cabin Cross-Section"));
    assert!(scene
        .elements
        .iter()
        .any(|element| matches!(element, SceneElement::Polygon { .. })));
}

/// Every registered body is drawn on the model's own floor: no figure shows the
/// invalid-floor error, no seat or freight item crosses the lining, and the
/// number of items drawn equals the number of items of the payload loading
/// that the section plane cuts.
#[test]
fn every_preset_draws_the_loading_the_model_placed() {
    for name in alas_config::presets::available() {
        let (config, plane, layout) = preset_case(name);
        let (scene, drawn) = build_section(&layout, &plane, &config, None);
        let all = texts(&scene);
        assert!(
            !all.iter()
                .any(|t| t.starts_with("Cabin section cannot be drawn")),
            "{name}: {all:?}"
        );
        assert!(
            !all.iter().any(|t| t.starts_with("WARNING")),
            "{name}: {all:?}"
        );
        let station = drawn.station;
        let cut = |item: &&DeckItem| cut_by(item, station);
        let freight = layout
            .items
            .iter()
            .filter(|i| i.kind == ItemKind::Uld && i.deck != LOWER)
            .filter(cut)
            .count();
        let hold: Vec<&DeckItem> = layout
            .items
            .iter()
            .filter(|i| i.deck == LOWER && matches!(i.kind, ItemKind::Bag | ItemKind::Uld))
            .filter(cut)
            .collect();
        let rigid = hold
            .iter()
            .filter(|i| matches!(&i.meta, ItemMeta::Container(m) if m.uld != "BLK"))
            .count();
        let bulk_only = matches!(
            &layout.summary,
            LayoutSummary::Passenger(s) if s.hold_ulds == 0
        );
        assert_eq!(drawn.deck_freight, freight, "{name}: freight drawn");
        assert_eq!(drawn.hold_items, hold.len(), "{name}: hold items drawn");
        assert_eq!(
            drawn.hold_ulds,
            if bulk_only { 0 } else { rigid },
            "{name}: hold containers drawn"
        );
    }
}

/// The ULDs the payload loading names are the ULDs the sections can show:
/// the containers of a passenger layout's hold are counted by the summary, and
/// every freighter pallet is a layout item.
#[test]
fn the_layout_items_equal_the_payload_loading() {
    for name in alas_config::presets::available() {
        let (_, _, layout) = preset_case(name);
        match &layout.summary {
            LayoutSummary::Passenger(summary) => {
                let in_layout = layout
                    .items
                    .iter()
                    .filter(|i| {
                        i.deck == LOWER
                            && matches!(&i.meta, ItemMeta::Container(m) if m.uld != "BLK")
                    })
                    .count() as i64;
                assert_eq!(in_layout, summary.hold_ulds, "{name}");
            }
            LayoutSummary::Cargo(summary) => {
                let in_layout = layout
                    .items
                    .iter()
                    .filter(|i| i.kind == ItemKind::Uld)
                    .count() as i64;
                assert_eq!(in_layout, summary.n_ulds, "{name}");
            }
            #[allow(unreachable_patterns)]
            _ => {}
        }
    }
}

#[test]
fn the_463l_stands_on_its_footprint_and_its_load_follows_the_lining() {
    let (config, plane, layout) = preset_case("C295");
    let pallet = layout
        .items
        .iter()
        .find(|i| matches!(&i.meta, ItemMeta::Container(m) if m.uld == "463"))
        .expect("the C-295 carries 463L pallets");
    let base = hold::pallet_base_ring(pallet);
    assert_eq!(base.len(), 4);
    let ys: Vec<f64> = base.iter().map(|p| p[0]).collect();
    let span = ys.iter().copied().fold(f64::NEG_INFINITY, f64::max)
        - ys.iter().copied().fold(f64::INFINITY, f64::min);
    assert!((span - pallet.width).abs() < 1e-9);
    let cabin = product_cabin_geometry(&plane, &config).expect("cabin");
    let hull = Hull::new(&cabin, pallet.x).expect("lining");
    let load = hold::built_up_load(hull, pallet);
    let top = load.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max);
    assert!(top <= pallet.z + 0.5 * pallet.height + 1e-9);
    for [y, z] in load {
        assert!(y.abs() <= hull.chord_half(z) - 0.049, "({y}, {z})");
        assert!(y.abs() <= 0.5 * pallet.width + 1e-9);
    }
}

#[test]
fn an_invalid_floor_is_reported_with_its_values_and_not_replaced() {
    let (config, plane, _) = preset_case("A320-200");
    let cabin = product_cabin_geometry(&plane, &config).expect("cabin");
    let midpoint = 0.5 * (cabin.cabin_start_x + cabin.cabin_end_x);
    let hull = Hull::new(&cabin, midpoint).expect("lining");
    let keel = hull.zc - hull.half_h;
    let message = hull
        .floor_problem(keel - 0.1)
        .expect("a floor below the keel is invalid");
    assert!(message.contains("Model floor z"), "{message}");
    assert!(message.contains(&format!("{:.3}", keel - 0.1)), "{message}");
    assert!(hull.floor_problem(keel + 0.5 * hull.half_h).is_none());
}

#[test]
fn the_lining_chord_matches_the_models_chord_above_the_floor() {
    let (config, plane, _) = preset_case("A320-200");
    let cabin = product_cabin_geometry(&plane, &config).expect("cabin");
    let midpoint = 0.5 * (cabin.cabin_start_x + cabin.cabin_end_x);
    let hull = Hull::new(&cabin, midpoint).expect("lining");
    let main = cabin.passenger_decks.first().expect("main deck");
    let floor = cabin.floor_z(main, midpoint);
    for above in [0.1, 0.635, 1.034, 1.5] {
        let model = cabin.lining_chord_at_z(midpoint, floor + above);
        let drawn = 2.0 * hull.chord_half(floor + above);
        assert!((model - drawn).abs() < 1e-6, "{above}: {model} vs {drawn}");
    }
}
