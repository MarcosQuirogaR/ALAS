// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Test fixtures deliberately use infallible constructors to keep setup readable.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::figure::figure_control_surfaces;
use super::geometry::{
    cs_surface_area, cs_surface_patch, le_chord_at_span, span_stations, TopAxes,
};
use crate::families::stability::test_support::{probe_airplane, probe_report};
use crate::scene::SceneElement;
use alas_config::AlasConfig;
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::wing::WingXSec;
fn naca() -> Airfoil {
    Airfoil::from_name("naca0012").expect("valid NACA name")
}

#[test]
fn le_chord_at_span_interpolates_linearly_between_two_tapered_stations() {
    let xs = vec![
        WingXSec::new([0.0, 0.0, 0.0], 4.0, 0.0, naca()),
        WingXSec::new([2.0, 10.0, 0.0], 2.0, 0.0, naca()),
    ];
    let (x_le, chord) = le_chord_at_span(&xs, 5.0, 1);
    assert!((x_le - 1.0).abs() < 1e-12);
    assert!((chord - 3.0).abs() < 1e-12);
}

#[test]
fn le_chord_at_span_falls_back_to_the_nearest_edge_outside_the_range() {
    let xs = vec![
        WingXSec::new([0.0, 0.0, 0.0], 4.0, 0.0, naca()),
        WingXSec::new([2.0, 10.0, 0.0], 2.0, 0.0, naca()),
    ];
    let (x_le, chord) = le_chord_at_span(&xs, 50.0, 1);
    assert_eq!((x_le, chord), (2.0, 2.0));
}

#[test]
fn span_stations_includes_a_break_strictly_between_the_endpoints() {
    let xs = vec![
        WingXSec::new([0.0, 0.0, 0.0], 4.0, 0.0, naca()),
        WingXSec::new([1.0, 5.0, 0.0], 3.0, 0.0, naca()),
        WingXSec::new([2.0, 10.0, 0.0], 2.0, 0.0, naca()),
    ];
    let stations = span_stations(&xs, 1, 0.0, 10.0);
    assert_eq!(stations, vec![0.0, 5.0, 10.0]);
    let reversed = span_stations(&xs, 1, 10.0, 0.0);
    assert_eq!(reversed, vec![10.0, 5.0, 0.0]);
}

#[test]
fn top_planform_maps_one_metre_equally_in_span_and_longitudinal_directions() {
    let axes = TopAxes::new((0.0, 0.0, 200.0, 100.0), (-10.0, 10.0), (0.0, 100.0));
    let span_step = (axes.point(1.0, 0.0)[0] - axes.point(0.0, 0.0)[0]).abs();
    let longitudinal_step = (axes.point(0.0, 1.0)[1] - axes.point(0.0, 0.0)[1]).abs();
    assert!((span_step - longitudinal_step).abs() < 1e-9);
}

#[test]
fn cs_surface_patch_returns_a_closed_front_and_back_loop() {
    let xs = vec![
        WingXSec::new([0.0, 0.0, 0.0], 4.0, 0.0, naca()),
        WingXSec::new([0.0, 10.0, 0.0], 4.0, 0.0, naca()),
    ];
    // A flap at the trailing 25% of chord across the whole span.
    let poly = cs_surface_patch(&xs, 1, 0.0, 10.0, 0.75, 1.0);
    assert_eq!(poly.len(), 4);
    // Front edge (frac_lo=0.75): x = 0 + 4*0.75 = 3.0.
    assert!((poly[0].1 - 3.0).abs() < 1e-12);
    assert!((poly[1].1 - 3.0).abs() < 1e-12);
    // Back edge (frac_hi=1.0), visited in reverse span order.
    assert!((poly[2].1 - 4.0).abs() < 1e-12);
    assert!((poly[3].1 - 4.0).abs() < 1e-12);
}

#[test]
fn cs_surface_area_doubles_when_mirrored() {
    let xs = vec![
        WingXSec::new([0.0, 0.0, 0.0], 4.0, 0.0, naca()),
        WingXSec::new([0.0, 10.0, 0.0], 4.0, 0.0, naca()),
    ];
    let one_side = cs_surface_area(&xs, 1, 0.0, 10.0, 0.75, 1.0, false);
    let mirrored = cs_surface_area(&xs, 1, 0.0, 10.0, 0.75, 1.0, true);
    assert!((mirrored - 2.0 * one_side).abs() < 1e-12);
    // 0.25 chord fraction * 4 m chord * 10 m span = 10 m^2.
    assert!((one_side - 10.0).abs() < 1e-9);
}

#[test]
fn a_real_probe_aircraft_with_a_vstab_draws_the_expected_polygon_count() {
    let report = probe_report(probe_airplane(true, true));
    let config = AlasConfig::default();
    let scene = figure_control_surfaces(&report, &config, Some("light"));
    let polygons = scene
        .elements
        .iter()
        .filter(|e| matches!(e, SceneElement::Polygon { .. }))
        .count();
    // Background planform: main(sym)=2 + hstab(sym)=2 + vstab(not)=1 = 5.
    // Patches: 4 main-wing surfaces mirrored = 8, elevator mirrored = 2,
    // v-stab fill + rudder = 2. Total 5 + 8 + 2 + 2 = 17.
    assert_eq!(polygons, 17);
    let texts: Vec<String> = scene
        .elements
        .iter()
        .filter_map(|e| match e {
            SceneElement::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert!(texts.iter().any(|t| t.starts_with("V_H")));
    assert!(texts.iter().any(|t| t.starts_with("V_V")));
}

#[test]
fn control_surface_figure_has_metre_axes_and_a_surface_legend() {
    let report = probe_report(probe_airplane(true, true));
    let config = AlasConfig::default();
    let scene = figure_control_surfaces(&report, &config, Some("light"));
    let texts: Vec<&str> = scene
        .elements
        .iter()
        .filter_map(|e| match e {
            SceneElement::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    for label in [
        "Span Y [m]",
        "Longitudinal X [m]",
        "Height Z [m]",
        "Slat",
        "Flap",
        "Aileron",
        "Spoiler",
        "Elevator",
        "Rudder",
    ] {
        assert!(
            texts.contains(&label),
            "missing figure contract text: {label}"
        );
    }
    let legend_bottom = scene.elements.iter().filter_map(|element| match element {
        SceneElement::Text { text, pos, .. }
            if ["Slat", "Flap", "Aileron", "Spoiler", "Elevator", "Rudder"]
                .contains(&text.as_str()) =>
        {
            Some(pos[1])
        }
        _ => None,
    });
    assert!(legend_bottom.clone().all(|y| y < scene.height - 10.0));
}

#[test]
fn without_a_vstab_no_rudder_legend_entry_appears() {
    let report = probe_report(probe_airplane(true, false));
    let config = AlasConfig::default();
    let scene = figure_control_surfaces(&report, &config, None);
    let texts: Vec<String> = scene
        .elements
        .iter()
        .filter_map(|e| match e {
            SceneElement::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert!(!texts.iter().any(|t| t == "Rudder"));
    assert!(!texts.iter().any(|t| t == "V-stab side view"));
}

#[test]
fn no_wings_renders_a_status_message_not_a_panic() {
    let mut airplane = probe_airplane(true, true);
    airplane.wings.clear();
    let report = probe_report(airplane);
    let config = AlasConfig::default();
    let scene = figure_control_surfaces(&report, &config, None);
    assert!(scene
        .elements
        .iter()
        .any(|e| matches!(e, SceneElement::Text { text, .. } | SceneElement::TextBlock { text, .. } if text.contains("geometry"))));
}

#[test]
fn the_control_surface_top_view_labels_the_longitudinal_coordinate_not_its_negative() {
    let axes = TopAxes::new((60.0, 70.0, 300.0, 460.0), (-10.0, 10.0), (2.0, 30.0));
    assert!(axes.0.y_tick_negated);
    // The nose (smaller X) is drawn above the tail.
    assert!(axes.point(0.0, 2.0)[1] < axes.point(0.0, 30.0)[1]);
}

#[test]
fn plot_data_names_every_surface_polygon() {
    let report = probe_report(probe_airplane(true, true));
    let config = AlasConfig::default();
    let data = crate::plot_data::with_capture(|| {
        let scene = figure_control_surfaces(&report, &config, Some("light"));
        crate::plot_data::PlotData::from_scene(&scene, "control_surfaces", None).unwrap()
    });
    let names: Vec<&str> = data
        .panels
        .iter()
        .flat_map(|p| p.series.iter().map(|s| s.name.as_str()))
        .collect();
    assert!(names.iter().all(|n| !n.starts_with("polygon")), "{names:?}");
    assert!(
        names.iter().any(|n| n.ends_with("planform +Y")),
        "{names:?}"
    );
    assert!(names.contains(&"Rudder"), "{names:?}");
}
