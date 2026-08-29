// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Characterization contracts for the W3.6 propulsion figure family.
//!
//! These tests check the visible scientific contract rather than merely
//! asserting that an SVG string exists: each figure must expose its units,
//! active design context, and the overlays that explain the plotted data in
//! both approved themes.

use alas_config::AlasConfig;
use alas_report::families::propulsion;
use alas_report::scene::{Color, Scene, SceneElement};

fn text_values(scene: &Scene) -> Vec<&str> {
    scene
        .elements
        .iter()
        .filter_map(|element| match element {
            SceneElement::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn has_text(scene: &Scene, needle: &str) -> bool {
    text_values(scene).iter().any(|text| text.contains(needle))
}

fn assert_theme_background(scene: &Scene, expected: Color) {
    assert_eq!(scene.background, Some(expected));
}

#[test]
fn propulsion_figures_keep_units_and_design_context_in_both_themes() {
    let config = AlasConfig::default();
    for theme in [Some("light"), Some("dark")] {
        let carpet = propulsion::figure_propulsion_carpet_plot(&config, theme);
        assert!(carpet
            .title
            .as_deref()
            .is_some_and(|title| title.contains("BPR=") && title.contains("FPR=")));
        assert!(has_text(&carpet, "Specific thrust"));
        assert!(has_text(&carpet, "TSFC"));
        assert!(has_text(&carpet, "T4t ="));
        assert!(has_text(&carpet, "OPR ="));

        let efficiency = propulsion::figure_propulsion_efficiency_decomposition(&config, theme);
        assert!(efficiency
            .title
            .as_deref()
            .is_some_and(|title| title.contains("BPR=") && title.contains("TIT=")));
        assert!(has_text(&efficiency, "OPR"));
        assert!(has_text(&efficiency, "Efficiency"));

        let bpr = propulsion::figure_propulsion_bpr_sensitivity(&config, theme);
        assert!(bpr
            .title
            .as_deref()
            .is_some_and(|title| title.contains("OPR=") && title.contains("TIT=")));
        assert!(has_text(&bpr, "Bypass ratio"));
        assert!(has_text(&bpr, "TSFC"));

        let cycle = propulsion::figure_propulsion_cycle_summary(&config, theme);
        assert!(has_text(&cycle, "Stagnation temperature [K]"));
        assert!(has_text(&cycle, "T0 (static)"));
        assert!(has_text(&cycle, "Tt4 (TIT)"));
        assert!(!has_text(&cycle, "Per-engine thrust, this cruise pt"));
        let summary = propulsion::propulsion_cycle_summary(&config);
        assert!(summary
            .iter()
            .any(|line| line.contains("Per-engine thrust, this cruise pt")));

        let expected_background = if theme == Some("dark") {
            Color::from_hex("#1e1e1e")
        } else {
            Color::from_hex("#ffffff")
        };
        assert_theme_background(&cycle, expected_background);
    }
}

#[test]
fn altitude_sweep_has_labelled_isograms_and_a_readable_cruise_annotation() {
    let config = AlasConfig::default();
    for theme in [Some("light"), Some("dark")] {
        let scene = propulsion::figure_propulsion_altitude_sweep(&config, theme);
        let contour_lines = scene
            .elements
            .iter()
            .filter(|element| matches!(element, SceneElement::Polyline { .. }))
            .count();
        assert!(contour_lines >= 6, "the heatmaps need visible iso-lines");
        assert!(has_text(&scene, "Altitude [km]"));
        assert!(has_text(&scene, "Mach number"));
        assert!(has_text(&scene, "Thrust [kN]"));
        assert!(has_text(&scene, "TSFC [mg/(N.s)]"));
        assert!(has_text(&scene, "Cruise (M0.84 @ 11.9 km)"));

        let marker_radii: Vec<f64> = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Circle { radius, .. } => Some(*radius),
                _ => None,
            })
            .collect();
        assert_eq!(marker_radii.len(), 4);
        assert!(marker_radii.iter().all(|radius| *radius <= 5.0));
    }
}
