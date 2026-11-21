// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// These tests intentionally panic if their constructed fixture violates its precondition.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

#[test]
fn linspace_int_indices_truncates_and_spans_the_full_range() {
    assert_eq!(linspace_int_indices(10, 1), vec![0]);
    assert_eq!(linspace_int_indices(5, 5), vec![0, 1, 2, 3, 4]);
    let idxs = linspace_int_indices(10, 4);
    assert_eq!(idxs[0], 0);
    assert_eq!(*idxs.last().expect("non-empty"), 9);
}

#[test]
fn linspace_int_indices_of_an_empty_history_is_empty() {
    assert!(linspace_int_indices(0, 5).is_empty());
}

#[test]
fn progress_colorbar_is_horizontal_and_labeled_below_the_planform() {
    let mut scene = Scene::new(760.0, 580.0, None);
    draw_horizontal_progress_bar(
        &mut scene,
        (270.0, 520.0, 220.0, 14.0),
        Colormap::Turbo,
        0.0,
        1.0,
        "Evaluation progress [-]",
        &crate::theme::PALETTE_LIGHT,
    );
    let label = scene.elements.iter().find_map(|element| match element {
        SceneElement::Text {
            text,
            pos,
            angle_deg,
            ..
        } if text == "Evaluation progress [-]" => Some((*pos, *angle_deg)),
        _ => None,
    });
    assert_eq!(label, Some(([380.0, 559.0], 0.0)));
    assert_eq!(
        scene
            .elements
            .iter()
            .filter(|element| matches!(element, SceneElement::Rect { fill: Some(_), .. }))
            .count(),
        64
    );
}

fn probe_plane() -> Airplane {
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec, DEFAULT_SHAPE};
    use alas_geom::aircraft::wing::{Wing, WingXSec};
    let foil = || Airfoil::from_name("naca2412").expect("naca");
    Airplane {
        name: "probe".to_owned(),
        xyz_ref: [0.0, 0.0, 0.0],
        wings: vec![Wing::new(
            "Main Wing",
            vec![
                WingXSec::new([8.0, 0.0, 0.0], 4.0, 0.0, foil()),
                WingXSec::new([12.0, 10.0, 0.5], 1.5, 0.0, foil()),
            ],
            true,
        )],
        fuselages: vec![Fuselage::new(
            "Fuselage",
            vec![
                FuselageXSec::new([0.0, 0.0, 0.0], Some(0.1), None, None, DEFAULT_SHAPE)
                    .expect("xsec"),
                FuselageXSec::new([30.0, 0.0, 0.0], Some(0.1), None, None, DEFAULT_SHAPE)
                    .expect("xsec"),
            ],
        )],
        s_ref: 40.0,
        c_ref: 2.0,
        b_ref: 20.0,
    }
}

/// Tick labels left of a panel's frame.
fn left_tick_labels(scene: &Scene, panel_left: f64, rows: (f64, f64)) -> Vec<String> {
    scene
        .elements
        .iter()
        .filter_map(|element| match element {
            SceneElement::Text { text, pos, .. }
                if pos[0] < panel_left
                    && (rows.0..=rows.1).contains(&pos[1])
                    && text.parse::<f64>().is_ok() =>
            {
                Some(text.clone())
            }
            _ => None,
        })
        .collect()
}

/// A planform with the nose at the top plots the negated longitudinal
/// coordinate; its ticks must still read the coordinate itself, so the top
/// view and the side view (X horizontal) agree on the sign of X.
#[test]
fn nose_up_planform_axes_label_the_longitudinal_coordinate_not_its_negative() {
    let mut scene = Scene::new(400.0, 300.0, None);
    let axes = Axes2D::new((60.0, 20.0, 300.0, 250.0), (-10.0, 10.0), (-30.0, -2.0))
        .with_negated_y_tick_labels();
    axes.draw_frame(&mut scene, &crate::theme::PALETTE_LIGHT);
    let labels = left_tick_labels(&scene, 60.0, (20.0, 270.0));
    assert!(!labels.is_empty());
    assert!(
        labels.iter().all(|label| !label.starts_with('-')),
        "{labels:?}"
    );
    // The nose (smaller X) is plotted above the tail.
    assert!(axes.map_point(0.0, -2.0)[1] < axes.map_point(0.0, -30.0)[1]);
}

#[test]
fn the_geometry_and_comparison_top_views_read_positive_longitudinal_ticks() {
    let plane = probe_plane();
    let geometry = figure_geometry(&plane, None);
    // The top panel's frame starts at x = 50.
    let labels = left_tick_labels(&geometry, 50.0, (50.0, 260.0));
    assert!(!labels.is_empty());
    assert!(
        labels.iter().all(|label| !label.starts_with('-')),
        "{labels:?}"
    );
    let comparison = figure_planform_comparison(&plane, &plane, ("a", "b"), None);
    let labels = left_tick_labels(&comparison, 137.5, (88.0, 704.0));
    assert!(!labels.is_empty());
    assert!(
        labels.iter().all(|label| !label.starts_with('-')),
        "{labels:?}"
    );
}
