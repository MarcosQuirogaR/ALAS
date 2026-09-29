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
