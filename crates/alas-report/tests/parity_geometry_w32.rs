// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! W3.2 geometry contracts: the fixture pins the reference figure inventory;
//! scene assertions pin the SVG-facing labels and physical-shape safeguards.

// Fixture construction failures are test-authoring errors, not runtime paths.
#![allow(clippy::expect_used)]

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_geom::asb::airfoil::Airfoil;
use alas_geom::asb::airplane::Airplane;
use alas_geom::asb::wing::{Wing, WingXSec};
use alas_geom::builder::AircraftBuilder;
use alas_payload::build::build_payload_layout;
use alas_report::families::geometry;
use alas_report::scene::SceneElement;
use alas_report::svg::render_svg;
use serde_json::Value;

fn plane() -> Airplane {
    Airplane {
        name: "W3.2 contract aircraft".to_owned(),
        xyz_ref: [0.0, 0.0, 0.0],
        wings: vec![
            Wing::new(
                "Main Wing",
                vec![
                    WingXSec::new(
                        [0.0, 0.0, 0.0],
                        4.0,
                        0.0,
                        Airfoil::from_name("naca2412").expect("fixture airfoil"),
                    ),
                    WingXSec::new(
                        [1.0, 10.0, 0.5],
                        1.5,
                        0.0,
                        Airfoil::from_name("naca2412").expect("fixture airfoil"),
                    ),
                ],
                true,
            ),
            Wing::new(
                "Horizontal Stabilizer",
                vec![
                    WingXSec::new(
                        [17.0, 0.0, 1.0],
                        2.0,
                        0.0,
                        Airfoil::from_name("naca0012").expect("fixture airfoil"),
                    ),
                    WingXSec::new(
                        [17.5, 4.0, 1.2],
                        1.0,
                        0.0,
                        Airfoil::from_name("naca0012").expect("fixture airfoil"),
                    ),
                ],
                true,
            ),
        ],
        fuselages: Vec::new(),
        s_ref: 40.0,
        c_ref: 2.0,
        b_ref: 20.0,
    }
}

#[test]
fn reference_fixture_covers_every_w32_figure_in_both_parity_themes() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../golden/report/reference_render_w32.json"
    ))
    .expect("W3.2 fixture is valid JSON");
    let figures = fixture["figures"].as_object().expect("figure map");
    for id in [
        "airfoil_evolution",
        "threeview",
        "cabin_payload",
        "design_evolution",
        "planform_comparison",
        "wireframe_wing",
        "wireframe_fuselage",
        "wireframe_empennage",
        "geometry",
    ] {
        for theme in ["light", "dark"] {
            assert!(figures.contains_key(&format!("{id}:{theme}")));
            let contract = &figures[&format!("{id}:{theme}")];
            assert_eq!(
                contract["available"], true,
                "{id}:{theme} unavailable: {}",
                contract["reason"]
            );
            assert!(contract["panel_count"].as_u64().unwrap_or(0) > 0);
            assert_eq!(contract["theme"], theme);
            assert!(contract["axes"].as_array().is_some());
        }
    }
}

#[test]
fn reference_contracts_preserve_panel_axes_series_annotations_and_reasons() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../golden/report/reference_render_w32.json"
    ))
    .expect("W3.2 fixture is valid JSON");
    let figures = fixture["figures"].as_object().expect("figure map");

    let evolution = &figures["design_evolution:light"];
    assert_eq!(evolution["panel_count"], 1);
    assert_eq!(evolution["axes"][0]["title"], "Design evolution (planform)");
    assert_eq!(evolution["axes"][0]["xlabel"], "span Y [m]");
    assert_eq!(evolution["axes"][0]["ylabel"], "longitudinal X [m]");
    assert_eq!(evolution["axes"][0]["aspect"], "1.0");
    assert!(evolution["axes"][0]["series"].is_array());
    assert!(evolution["axes"][0]["annotations"].is_array());

    let cabin = &figures["cabin_payload:light"];
    assert_eq!(
        cabin["panel_count"], 3,
        "side plus main and lower-deck panels"
    );
    assert!(cabin["suptitle"]
        .as_str()
        .unwrap_or("")
        .contains("Passenger cabin"));
    assert!(cabin["axes"]
        .as_array()
        .expect("cabin axes")
        .iter()
        .all(|axis| {
            axis["series"].is_array()
                && axis["patches"].is_array()
                && axis["annotations"].is_array()
        }));
    assert!(cabin["axes"][2]["legend"]
        .as_array()
        .expect("cabin side legend")
        .iter()
        .any(|label| label == "Payload CG  20.7% MAC"));

    let dark = &figures["cabin_payload:dark"];
    assert_ne!(cabin["facecolor"], dark["facecolor"]);
    assert_eq!(dark["available"], true);
    assert!(dark["reason"].is_null());
}

#[test]
fn cabin_scene_matches_the_reference_payload_structure_and_both_themes() {
    let config = AlasConfig::default();
    let builder = AircraftBuilder::new(Some(config.geometry.clone()));
    let aircraft = builder
        .build(Some(&DesignVector::default()), true)
        .expect("default aircraft builder input");
    let layout = build_payload_layout(&aircraft, &config, 0.0, 0.0)
        .expect("default aircraft has a representative payload layout");

    for theme in ["light", "dark"] {
        let scene = geometry::figure_cabin_payload(&layout, &aircraft, &config, Some(theme));
        assert!(alas_report::scene::visual_title(&scene).is_none());
        let texts: Vec<&str> = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(texts.iter().any(|text| text.contains("MAIN deck")));
        assert!(texts.iter().any(|text| text.contains("LOWER deck")));
        for label in ["Seat row", "Galley / lav", "Emergency exit", "Payload CG"] {
            assert!(
                texts.iter().any(|text| text.contains(label)),
                "missing {label}"
            );
        }
        assert!(
            scene
                .elements
                .iter()
                .filter(|element| matches!(element, SceneElement::Rect { .. }))
                .count()
                > 100
        );
        assert!(
            scene
                .elements
                .iter()
                .filter(|element| matches!(element, SceneElement::Polygon { .. }))
                .count()
                >= 2
        );
        let seat_labels = texts
            .iter()
            .filter(|text| {
                let mut chars = text.chars().rev();
                chars.next().is_some_and(|last| last.is_ascii_uppercase())
                    && chars.clone().all(|character| character.is_ascii_digit())
                    && chars.next().is_some()
            })
            .count() as i64;
        let expected_seats = match &layout.summary {
            alas_payload::layout::LayoutSummary::Passenger(summary) => summary.seated_pax,
            alas_payload::layout::LayoutSummary::Cargo(_) => 0,
        };
        assert_eq!(seat_labels, expected_seats);
        assert!(texts.contains(&"12A"));
        assert!(!texts.iter().any(|text| text.starts_with("13")));
        assert!(texts.iter().any(|text| text.starts_with("14")));
    }
}

#[test]
fn generated_main_deck_map_labels_every_placed_main_deck_seat() {
    let config = AlasConfig::default();
    let aircraft = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&DesignVector::default()), true)
        .expect("default aircraft builder input");
    let layout = build_payload_layout(&aircraft, &config, 0.0, 0.0)
        .expect("default aircraft has a representative payload layout");
    let scene = geometry::figure_main_deck_seat_map(&layout, &aircraft, &config, Some("dark"));
    assert!(alas_report::scene::visual_title(&scene).is_none());
    let seat_labels = scene
        .elements
        .iter()
        .filter_map(|element| match element {
            SceneElement::Text { text, .. } => Some(text),
            _ => None,
        })
        .filter(|text| {
            let mut chars = text.chars().rev();
            chars.next().is_some_and(|last| last.is_ascii_uppercase())
                && chars.clone().all(|character| character.is_ascii_digit())
                && chars.next().is_some()
        })
        .count() as i64;
    let expected = layout
        .items
        .iter()
        .filter_map(|item| match &item.meta {
            alas_payload::layout::ItemMeta::Seat(meta) if item.deck == "main" => Some(meta.filled),
            _ => None,
        })
        .sum::<i64>();
    assert_eq!(seat_labels, expected);
    assert!(scene.elements.iter().any(|element| matches!(
        element,
        SceneElement::Line { stroke, .. } if stroke.color == alas_report::scene::Color::from_hex("#e74c3c")
    )));
    for landmark in ["G", "L"] {
        assert!(scene.elements.iter().any(|element| matches!(
            element,
            SceneElement::Text { text, .. } if text == landmark
        )));
    }
    assert!(scene.elements.iter().any(|element| matches!(
        element,
        SceneElement::Text { text, .. }
            if text == "Generated sizing layout - not a published airline seat map"
    )));
}

#[test]
fn geometry_figures_keep_reference_titles_axes_units_and_legends() {
    let aircraft = plane();
    let airfoil = render_svg(&geometry::figure_airfoil_evolution(&aircraft, Some("dark")));
    for text in [
        "Wing cross-sections",
        "x/c [-]",
        "y/c [-]",
        "Spanwise station y [m]",
    ] {
        assert!(airfoil.contains(text), "missing {text}");
    }

    let planform = render_svg(&geometry::figure_planform_comparison(
        &aircraft,
        &aircraft,
        ("baseline", "optimized"),
        Some("light"),
    ));
    for text in [
        "Planform Comparison",
        "span Y [m]",
        "longitudinal X [m]",
        "Baseline",
        "Optimized",
    ] {
        assert!(planform.contains(text), "missing {text}");
    }

    let threeview = render_svg(&geometry::figure_asb_threeview(&aircraft, Some("dark")));
    for text in [
        "Three-View Drawing",
        "Top view",
        "Front view",
        "Side view",
        "Isometric",
        "Main wing",
        "Fuselage",
    ] {
        assert!(threeview.contains(text), "missing {text}");
    }
    assert!(threeview.contains("[m]"));
    assert!(!threeview.contains("#00d8ff"));
}

#[test]
fn isolated_wireframes_include_thickness_without_a_secondary_axes_caption() {
    let aircraft = plane();
    for scene in [
        geometry::figure_wireframe_wing(&aircraft, Some("light")),
        geometry::figure_wireframe_empennage(&aircraft, Some("dark")),
    ] {
        let svg = render_svg(&scene);
        assert!(!svg.contains("axes: metres"));
        assert!(svg.matches("<polyline").count() >= 4);
    }
}
