// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The default and Fit framings keep the whole aircraft (or the focused
//! component) inside the free part of the viewport at every window size, not
//! only at one large size.

use super::super::fields::Discipline;
use crate::state::AppState;
use alas_report::scene::SceneElement;

/// The extent of every polygon the scene draws, in canvas points.
fn drawn_extent(scene: &alas_report::scene::Scene) -> (f64, f64, f64, f64) {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for element in &scene.elements {
        if let SceneElement::Polygon { points, .. } = element {
            for point in points {
                x0 = x0.min(point[0]);
                x1 = x1.max(point[0]);
                y0 = y0.min(point[1]);
                y1 = y1.max(point[1]);
            }
        }
    }
    (x0, y0, x1, y1)
}

/// Viewport sizes (points) of the review's window sizes and the sizes between.
fn viewports() -> Vec<(f32, f32)> {
    let mut sizes = vec![
        (868.0, 640.0),
        (1084.0, 721.0),
        (1560.0, 860.0),
        (1280.0, 800.0),
    ];
    for width in (700..=2400).step_by(100) {
        for height in [480, 600, 760, 900] {
            sizes.push((width as f32, height as f32));
        }
    }
    sizes
}

#[test]
fn every_default_and_fit_framing_stays_inside_the_free_viewport() {
    let inset = (167.0_f32, 44.0_f32, 12.0_f32, 64.0_f32);
    let mut failures = Vec::new();
    for (width, height) in viewports() {
        let mut state = AppState::default();
        assert!(state.enter_sandbox(true));
        state.set_sandbox_fit_inset(inset);
        state.set_sandbox_viewport_size((width, height));
        for focus in [None, Some(Discipline::Wing), Some(Discipline::Fuselage)] {
            state.set_sandbox_focus(focus);
            let (scene, _) = state.sandbox.scene.clone().expect("scene");
            let (x0, y0, x1, y1) = drawn_extent(&scene);
            let (w, h) = (f64::from(width), f64::from(height));
            let free = (
                f64::from(inset.0) - 2.0,
                f64::from(inset.1) - 2.0,
                w - f64::from(inset.2) + 2.0,
                h - f64::from(inset.3) + 2.0,
            );
            if x0 < free.0 || y0 < free.1 || x1 > free.2 || y1 > free.3 {
                failures.push(format!(
                    "{width}x{height} {focus:?}: drawn {x0:.0}..{x1:.0} x {y0:.0}..{y1:.0}, free {:.0}..{:.0} x {:.0}..{:.0}",
                    free.0, free.2, free.1, free.3
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} framings leave the free viewport:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// A window resized after the first fit (the Sandbox opened at one size, then
/// the window became another) still shows the whole aircraft: the framing the
/// first size gave follows the new viewport instead of clipping a wing tip.
#[test]
fn a_resized_window_still_contains_the_aircraft() {
    let inset = (167.0_f32, 44.0_f32, 12.0_f32, 64.0_f32);
    let mut failures = Vec::new();
    for start in [(1560.0_f32, 860.0_f32), (1100.0, 760.0), (2300.0, 1200.0)] {
        for (width, height) in viewports() {
            let mut state = AppState::default();
            assert!(state.enter_sandbox(true));
            state.set_sandbox_fit_inset(inset);
            state.set_sandbox_viewport_size(start);
            for focus in [None, Some(Discipline::Wing)] {
                state.set_sandbox_focus(focus);
                state.set_sandbox_viewport_size((width, height));
                let (scene, _) = state.sandbox.scene.clone().expect("scene");
                let (x0, y0, x1, y1) = drawn_extent(&scene);
                let (w, h) = (f64::from(width), f64::from(height));
                if x0 < f64::from(inset.0) - 2.0
                    || y0 < f64::from(inset.1) - 2.0
                    || x1 > w - f64::from(inset.2) + 2.0
                    || y1 > h - f64::from(inset.3) + 2.0
                {
                    failures.push(format!(
                        "{start:?} -> {width}x{height} {focus:?}: drawn {x0:.0}..{x1:.0} x {y0:.0}..{y1:.0}"
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} resized framings clip:\n{}",
        failures.len(),
        failures[..failures.len().min(12)].join("\n")
    );
}
