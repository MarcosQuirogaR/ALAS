// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;
use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_payload::build::build_payload_layout;
use alas_pipeline::cabin_scene::{OrientationStatus, Point2, UldDefinition};
use alas_pipeline::CabinSceneInputs;

fn scene_for(name: &str) -> CabinScene {
    let preset = presets::get(name).expect("registered preset");
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
        .expect("preset configuration");
    let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("preset geometry");
    let layout = build_payload_layout(&airplane, &config, 0.0, 0.0).expect("payload layout");
    CabinScene::from_parts(
        &config,
        CabinSceneInputs {
            design: preset.design_vector,
            airplane: &airplane,
            layout: &layout,
            source: "test fixture",
        },
    )
    .expect("cabin scene")
}

fn cargo_item(center_y_m: f64) -> CargoItem {
    CargoItem {
        id: "test-cargo".to_owned(),
        slot_id: "test-slot".to_owned(),
        envelope: Box3 {
            center_x_m: 0.0,
            center_y_m,
            center_z_m: 0.5,
            length_m: 1.56,
            width_m: 2.0,
            height_m: 1.0,
        },
        mass_kg: 1.0,
        fill_fraction: Some(0.5),
        net_load_kg: Some(1.0),
        uld: Some(UldDefinition {
            key: "LD3".to_owned(),
            code: "AKE".to_owned(),
            name: "LD3 Container".to_owned(),
            dimensions_m: [1.56, 1.53, 1.63],
            normalized_contour_yz: vec![
                Point2 { y: -1.0, z: 0.0 },
                Point2 { y: 0.56, z: 0.0 },
                Point2 { y: 1.0, z: 0.44 },
                Point2 { y: 1.0, z: 1.0 },
                Point2 { y: 0.0, z: 1.0 },
                Point2 { y: -1.0, z: 1.0 },
                Point2 { y: -1.0, z: 0.5 },
            ],
            contour_source: "test E contour".to_owned(),
            contour_fidelity: "visualization_only".to_owned(),
            mirrorable: true,
        }),
        orientation: OrientationStatus {
            value: None,
            status: "missing".to_owned(),
            reason: "test fixture".to_owned(),
        },
        fidelity: "test".to_owned(),
        source: "test".to_owned(),
    }
}

#[test]
fn containment_and_half_width_agree_with_a_known_square() {
    let square = rectangle(0.0, 2.0, -1.0, 1.0);
    assert!(contains_point(&square, [0.0, 0.0]));
    assert!(!contains_point(&square, [1.5, 0.0]));
    assert!(ring_within(&rectangle(0.0, 1.0, -0.5, 0.5), &square));
    assert!(!ring_within(&rectangle(0.0, 3.0, -0.5, 0.5), &square));
    assert!((ring_area(&square) - 4.0).abs() < 1e-12);
    assert!((half_width_at(&square, 0.0, 1.0).expect("crossing") - 1.0).abs() < 1e-12);
    assert!(half_width_at(&square, 5.0, 1.0).is_none());
}

#[test]
fn cargo_section_mirrors_a_contoured_uld_for_the_port_side() {
    let (starboard, from_uld) = cargo_ring(&cargo_item(1.0));
    let (port, port_from_uld) = cargo_ring(&cargo_item(-1.0));
    assert!(from_uld && port_from_uld);

    let starboard_bounds = bounds(&starboard);
    let port_bounds = bounds(&port);
    let starboard_bottom_y = starboard
        .iter()
        .filter(|point| point[1].abs() < 1e-12)
        .map(|point| point[0])
        .fold(f64::NEG_INFINITY, f64::max);
    let port_bottom_y = port
        .iter()
        .filter(|point| point[1].abs() < 1e-12)
        .map(|point| point[0])
        .fold(f64::INFINITY, f64::min);

    assert!(starboard_bottom_y < starboard_bounds[2]);
    assert!(port_bottom_y > port_bounds[0]);
    assert!((starboard_bounds[2] + port_bounds[0]).abs() < 1e-12);
}

#[test]
fn the_scale_figure_keeps_its_stated_height() {
    let extent = bounds(&occupant_ring(0.4, 1.0));
    assert!((extent[1] - 1.0).abs() < 1e-12);
    assert!(extent[3] < 1.0 + OCCUPANT_HEIGHT_M);
    assert!((extent[2] - extent[0] - 2.0 * OCCUPANT_HALF_WIDTH_M).abs() < 1e-12);
}

#[test]
fn every_drawn_part_belongs_to_the_one_selected_station() {
    let cabin = scene_for("A320-200");
    let slice = slice_section(&cabin).expect("sectionable station");
    let x = slice.choice.x_m;
    for deck in &slice.decks {
        if let Some(row) = deck.row {
            assert!(spans(&row.envelope, x), "row {} is not cut at x", row.id);
        }
    }
    let cut_items = cabin
        .cargo
        .items
        .iter()
        .filter(|item| spans(&item.envelope, x))
        .count();
    assert_eq!(slice.cargo.len(), cut_items);
}

#[test]
fn a_double_deck_section_cuts_a_seat_row_on_both_decks() {
    let cabin = scene_for("A380-800");
    let slice = slice_section(&cabin).expect("sectionable station");
    assert_eq!(slice.choice.passenger_decks, 2);
    assert_eq!(slice.choice.decks_with_rows, 2);
    assert!(slice
        .decks
        .iter()
        .filter(|deck| deck.deck.passenger)
        .all(|deck| !deck.seats.is_empty()));
}

#[test]
fn a_cabin_too_shallow_for_a_standing_figure_reports_it_instead_of_shrinking_one() {
    let cabin = scene_for("ATR72-600");
    let slice = slice_section(&cabin).expect("sectionable station");
    assert!(slice
        .decks
        .iter()
        .filter(|deck| deck.deck.passenger)
        .all(|deck| deck.occupants.is_empty()));
    assert!(slice
        .findings
        .iter()
        .any(|finding| finding.contains("standing figure")));
}

#[test]
fn every_registered_preset_renders_a_titled_section() {
    for name in presets::available() {
        let cabin = scene_for(name);
        let figure = figure_cabin_section(&cabin, Some("light"));
        assert!(
            figure.elements.len() > 40,
            "{name}: only {} element(s)",
            figure.elements.len()
        );
        assert!(figure.elements.iter().any(|element| matches!(
            element,
            SceneElement::Text { text, .. } if text.contains("cabin cross-section")
        )));
        let repeat = figure_cabin_section(&cabin, Some("light"));
        assert_eq!(figure.elements.len(), repeat.elements.len());
    }
}
