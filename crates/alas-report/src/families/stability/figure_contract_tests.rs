// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Characterization checks for the existing stability metrics and side-view
//! presentation contract. These tests guard data association and annotations;
//! they do not approve a new stability presentation.

// Test code: a failed unwrap on a fixture it builds is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::figure_stability_side_view;
use super::scalars::stability_scalars;
use super::side_view::WING_MAC_COLOR;
use super::test_support::{probe_airplane, probe_report};
use crate::scene::SceneElement;

fn text_values(scene: &crate::scene::Scene) -> Vec<&str> {
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
fn stability_side_view_keeps_marker_names_and_metre_associations() {
    let report = probe_report(probe_airplane(true, true));
    let scene = figure_stability_side_view(&report, Some("light"));
    let texts = text_values(&scene);

    for label in ["Wing AC", "Phys CG", "Aero CG", "Neutral Pt", "H-Stab AC"] {
        assert!(
            texts.iter().any(|text| text.starts_with(label)),
            "missing side-view marker: {label}"
        );
    }
    assert!(texts.iter().any(|text| text.contains("% MAC")));
    assert!(texts.iter().any(|text| text.starts_with("Wing MAC")));
    let polygons = scene
        .elements
        .iter()
        .filter(|element| matches!(element, SceneElement::Polygon { .. }))
        .count();
    assert!(
        polygons >= 3,
        "fuselage and airfoil section outlines are visible"
    );
}

#[test]
fn stability_side_view_draws_the_mac_from_the_same_lemac_and_chord_scalars() {
    let report = probe_report(probe_airplane(true, true));
    let scalars = stability_scalars(&report).expect("probe has a main wing");
    let scene = figure_stability_side_view(&report, Some("light"));
    let mac_color = crate::scene::Color::from_hex(WING_MAC_COLOR);
    let mac = scene.elements.iter().find_map(|element| match element {
        SceneElement::Polygon {
            points,
            stroke: Some(stroke),
            ..
        } if stroke.color == mac_color => Some(points),
        _ => None,
    });
    let mac = mac.expect("side view contains a visible MAC airfoil outline");
    let wing_ac_x = scene.elements.iter().find_map(|element| match element {
        SceneElement::Line { p1, p2, stroke }
            if stroke.color == crate::scene::Color::from_hex("#c0392b")
                && (p2[1] - p1[1]).abs() > 100.0 =>
        {
            Some(p1[0])
        }
        _ => None,
    });
    let wing_ac_x = wing_ac_x.expect("side view contains the wing AC marker");

    let mac_start_x = mac
        .iter()
        .map(|point| point[0])
        .fold(f64::INFINITY, f64::min);
    let mac_end_x = mac
        .iter()
        .map(|point| point[0])
        .fold(f64::NEG_INFINITY, f64::max);
    let mac_vertical_extent = mac
        .iter()
        .map(|point| point[1])
        .fold(f64::NEG_INFINITY, f64::max)
        - mac
            .iter()
            .map(|point| point[1])
            .fold(f64::INFINITY, f64::min);
    assert!(mac_end_x > mac_start_x);
    assert!(mac_vertical_extent > 1.0, "MAC has an airfoil thickness");
    let quarter_mac_x = mac_start_x + 0.25 * (mac_end_x - mac_start_x);
    assert!((quarter_mac_x - wing_ac_x).abs() < 1e-8);
    assert!((mac_end_x - mac_start_x).abs() > 0.0);
    assert!(
        (scalars.x_lemac + scalars.c_ref - scalars.x_wing_ac - 0.75 * scalars.c_ref).abs() < 1e-12
    );
    for theme in ["light", "dark"] {
        let background = crate::scene::Color::from_hex(crate::theme::get_palette(Some(theme)).bg);
        assert!(mac_color.contrast_against(background) >= 3.0);
    }
}
