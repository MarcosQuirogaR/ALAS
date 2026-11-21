// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Sandbox's pointer input as the running application receives it: many
//! small per-frame steps, with the platform's raw device motion beside them.

use egui::{pos2, vec2, Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect};

use super::scene::SANDBOX_CAMERA_ID;
use super::viewport::viewport_rect;
use super::workspace::show_sandbox_workspace;
use crate::state::AppState;

const SCREEN: Rect = Rect::from_min_max(Pos2::ZERO, pos2(1280.0, 820.0));

fn run(ctx: &Context, state: &mut AppState, events: Vec<Event>) -> egui::FullOutput {
    ctx.run(
        RawInput {
            screen_rect: Some(SCREEN),
            events,
            ..Default::default()
        },
        |ctx| show_sandbox_workspace(state, ctx),
    )
}

fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

fn opened() -> (Context, AppState) {
    let mut state = AppState::default();
    assert!(state.enter_sandbox(true));
    let ctx = Context::default();
    for _ in 0..3 {
        run(&ctx, &mut state, vec![]);
    }
    (ctx, state)
}

/// `egui::Response::drag_motion` reports the platform's raw device counts
/// ("unspecified units") whenever the integration supplies them, and a
/// high-resolution mouse reports several counts per cursor pixel. The orbit
/// gain is degrees per logical point of cursor motion, so it must follow the
/// cursor: 100 points turn the camera about 38 degrees however many raw
/// counts come with them.
#[test]
fn a_hundred_point_orbit_drag_turns_about_38_degrees_whatever_the_raw_mouse_counts() {
    for counts_per_point in [1.0_f32, 3.5] {
        let (ctx, mut state) = opened();
        let view = viewport_rect(&ctx).expect("viewport");
        let start = pos2(view.center().x + 120.0, view.center().y + 40.0);
        let before = *state.preview_camera_mut(SANDBOX_CAMERA_ID);
        run(&ctx, &mut state, vec![Event::PointerMoved(start)]);
        run(&ctx, &mut state, vec![button(start, true)]);
        for step in 1..=100 {
            run(
                &ctx,
                &mut state,
                vec![
                    Event::MouseMoved(vec2(counts_per_point, 0.0)),
                    Event::PointerMoved(start + vec2(step as f32, 0.0)),
                ],
            );
        }
        let after = *state.preview_camera_mut(SANDBOX_CAMERA_ID);
        let turned = (after.yaw_deg - before.yaw_deg).rem_euclid(360.0);
        assert!(
            (turned - 38.0).abs() < 3.0,
            "{counts_per_point} counts/pt: turned {turned} deg"
        );
    }
}

/// "Advanced Settings" in the Sandbox's top bar is a button that opens the
/// detached settings window (not a drop-down): one click sets the flag the
/// window is shown from.
#[test]
fn the_advanced_settings_top_bar_button_opens_its_window() {
    fn centre_of(output: &egui::FullOutput, wanted: &str) -> Option<Pos2> {
        fn visit(shape: &egui::Shape, wanted: &str) -> Option<Pos2> {
            match shape {
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| visit(s, wanted)),
                egui::Shape::Text(text) if text.galley.text().eq_ignore_ascii_case(wanted) => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            }
        }
        output
            .shapes
            .iter()
            .find_map(|clipped| visit(&clipped.shape, wanted))
    }
    let (ctx, mut state) = opened();
    assert!(!state.sandbox.layout.advanced_settings_open);
    let output = run(&ctx, &mut state, vec![]);
    let at = centre_of(&output, "Advanced Settings")
        .expect("the top bar shows the Advanced Settings button");
    run(&ctx, &mut state, vec![Event::PointerMoved(at)]);
    run(&ctx, &mut state, vec![button(at, true)]);
    run(&ctx, &mut state, vec![button(at, false)]);
    assert!(
        state.sandbox.layout.advanced_settings_open,
        "the click opens the window"
    );
}
