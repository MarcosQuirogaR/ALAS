// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Sandbox navigation through the real workspace frame: wheel zoom towards
//! the cursor, pan, the pivot pick, and the ways back from an isolated
//! component. Pointer events are synthesised on an egui context; no window
//! is involved.

// A test asserts on values it constructed, so a failed unwrap there is the
// assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_gui::sandbox::scene::SANDBOX_CAMERA_ID;
use alas_gui::sandbox::viewport::{overlay_rect_tagged, viewport_rect};
use alas_gui::sandbox::workspace::show_sandbox_workspace;
use alas_gui::state::AppState;
use egui::{pos2, vec2, Context, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect};

const SCREEN: Rect = Rect::from_min_max(Pos2::ZERO, pos2(1280.0, 820.0));

fn frame(ctx: &Context, state: &mut AppState, events: Vec<Event>) {
    let input = RawInput {
        screen_rect: Some(SCREEN),
        events,
        ..Default::default()
    };
    let _ = ctx.run(input, |ctx| show_sandbox_workspace(state, ctx));
}

fn settle(ctx: &Context, state: &mut AppState) {
    for _ in 0..3 {
        frame(ctx, state, vec![]);
    }
}

fn button(pos: Pos2, button: PointerButton, pressed: bool, modifiers: Modifiers) -> Event {
    Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers,
    }
}

fn click(ctx: &Context, state: &mut AppState, pos: Pos2, which: PointerButton, times: usize) {
    frame(ctx, state, vec![Event::PointerMoved(pos)]);
    for _ in 0..times {
        frame(ctx, state, vec![button(pos, which, true, Modifiers::NONE)]);
        frame(ctx, state, vec![button(pos, which, false, Modifiers::NONE)]);
    }
    frame(ctx, state, vec![]);
}

fn drag_with(
    ctx: &Context,
    state: &mut AppState,
    from: Pos2,
    which: PointerButton,
    by: egui::Vec2,
) {
    frame(ctx, state, vec![Event::PointerMoved(from)]);
    frame(ctx, state, vec![button(from, which, true, Modifiers::NONE)]);
    for step in 1..=4 {
        frame(
            ctx,
            state,
            vec![Event::PointerMoved(from + by * (step as f32 / 4.0))],
        );
    }
    frame(
        ctx,
        state,
        vec![button(from + by, which, false, Modifiers::NONE)],
    );
    frame(ctx, state, vec![]);
}

fn opened() -> (Context, AppState) {
    let mut state = AppState::default();
    assert!(state.enter_sandbox(true));
    let ctx = Context::default();
    settle(&ctx, &mut state);
    (ctx, state)
}

/// A point of the viewport clear of every floating control.
fn free_point(ctx: &Context) -> Pos2 {
    let view = viewport_rect(ctx).expect("viewport");
    pos2(view.center().x + 120.0, view.center().y + 40.0)
}

#[test]
fn the_wheel_zooms_towards_the_cursor() {
    let (ctx, mut state) = opened();
    let cursor = free_point(&ctx);
    frame(&ctx, &mut state, vec![Event::PointerMoved(cursor)]);
    let (_, before) = state.sandbox.scene.clone().expect("scene");
    // The model point under the cursor, on the pivot's depth.
    let view = viewport_rect(&ctx).expect("viewport");
    let canvas_point = [
        f64::from(cursor.x - view.left()),
        f64::from(cursor.y - view.top()),
    ];
    let scale = before.scale();
    let (right, up) = before.camera.screen_axes();
    let (a, b) = (
        (canvas_point[0] - before.canvas.0 * 0.5) / scale,
        -(canvas_point[1] - before.canvas.1 * 0.5) / scale,
    );
    let under: [f64; 3] = std::array::from_fn(|k| before.center[k] + a * right[k] + b * up[k]);
    let zoom_before = state.preview_camera_mut(SANDBOX_CAMERA_ID).zoom;
    for _ in 0..6 {
        frame(
            &ctx,
            &mut state,
            vec![Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, 40.0),
                modifiers: Modifiers::NONE,
            }],
        );
    }
    settle(&ctx, &mut state);
    let zoom_after = state.preview_camera_mut(SANDBOX_CAMERA_ID).zoom;
    assert!(
        zoom_after > zoom_before * 1.2,
        "zoomed in: {zoom_before} -> {zoom_after}"
    );
    let (_, after) = state.sandbox.scene.clone().expect("scene");
    let [x, y] = after.project(under);
    assert!(
        (x - canvas_point[0]).abs() < 1.5 && (y - canvas_point[1]).abs() < 1.5,
        "the point under the cursor moved to ({x:.1}, {y:.1}) from {canvas_point:?}"
    );
}

#[test]
fn a_right_drag_pans_and_a_left_drag_orbits_about_the_pivot() {
    let (ctx, mut state) = opened();
    let start = free_point(&ctx);
    let pivot = state.sandbox.pivot.expect("pivot");
    let (_, before) = state.sandbox.scene.clone().expect("scene");
    let pivot_before = before.project(pivot);
    let camera_before = *state.preview_camera_mut(SANDBOX_CAMERA_ID);

    drag_with(
        &ctx,
        &mut state,
        start,
        PointerButton::Secondary,
        vec2(60.0, 24.0),
    );
    let (_, panned) = state.sandbox.scene.clone().expect("scene");
    let pivot_panned = panned.project(pivot);
    assert!(
        (pivot_panned[0] - pivot_before[0] - 60.0).abs() < 1.5
            && (pivot_panned[1] - pivot_before[1] - 24.0).abs() < 1.5,
        "the pivot follows the pointer: {pivot_before:?} -> {pivot_panned:?}"
    );
    assert_eq!(
        *state.preview_camera_mut(SANDBOX_CAMERA_ID),
        camera_before,
        "a pan never turns the camera"
    );

    drag_with(
        &ctx,
        &mut state,
        start,
        PointerButton::Primary,
        vec2(80.0, 30.0),
    );
    let (_, orbited) = state.sandbox.scene.clone().expect("scene");
    let pivot_orbited = orbited.project(pivot);
    assert_ne!(*state.preview_camera_mut(SANDBOX_CAMERA_ID), camera_before);
    assert!(
        (pivot_orbited[0] - pivot_panned[0]).abs() < 1e-6
            && (pivot_orbited[1] - pivot_panned[1]).abs() < 1e-6,
        "an orbit leaves the pivot where it is on screen"
    );
}

#[test]
fn a_double_click_sets_the_pivot_on_the_surface_without_moving_the_picture() {
    let (ctx, mut state) = opened();
    let centre_pivot = state.sandbox.pivot.expect("pivot");
    let (_, before) = state.sandbox.scene.clone().expect("scene");
    let view = viewport_rect(&ctx).expect("viewport");
    // The fuselage cabin centre, on screen.
    let plane = state.sandbox.airplane.as_ref().expect("plane");
    let xsecs = &plane.fuselages[0].xsecs;
    let mid = xsecs[xsecs.len() / 2].xyz_c;
    let [px, py] = before.project(mid);
    let target = pos2(view.left() + px as f32, view.top() + py as f32);
    click(&ctx, &mut state, target, PointerButton::Primary, 2);
    let pivot = state.sandbox.pivot.expect("pivot");
    assert_ne!(
        pivot, centre_pivot,
        "the pivot moved to the clicked surface point"
    );
    let (_, after) = state.sandbox.scene.clone().expect("scene");
    let [x, y] = after.project(pivot);
    assert!(
        (x - f64::from(target.x - view.left())).abs() < 1.5
            && (y - f64::from(target.y - view.top())).abs() < 1.5,
        "the picked point stays under the pointer"
    );
}

#[test]
fn escape_and_the_visible_toggle_return_from_an_isolated_component() {
    let (ctx, mut state) = opened();
    let wing = overlay_rect_tagged(&ctx, "category:wing").expect("wing button");
    click(&ctx, &mut state, wing.center(), PointerButton::Primary, 1);
    assert_eq!(state.sandbox.focus().map(|d| d.id()), Some("wing"));
    settle(&ctx, &mut state);

    // The labelled toggle is in the camera row while a component is isolated.
    assert!(
        overlay_rect_tagged(&ctx, "camera").is_some(),
        "the camera row is drawn"
    );
    let key = |pressed| Event::Key {
        key: Key::Escape,
        physical_key: Some(Key::Escape),
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    frame(&ctx, &mut state, vec![key(true)]);
    frame(&ctx, &mut state, vec![key(false)]);
    assert_eq!(
        state.sandbox.focus(),
        None,
        "Escape shows the whole aircraft"
    );

    // Clicking the isolated component's button again also returns.
    click(&ctx, &mut state, wing.center(), PointerButton::Primary, 1);
    assert_eq!(state.sandbox.focus().map(|d| d.id()), Some("wing"));
    settle(&ctx, &mut state);
    let wing = overlay_rect_tagged(&ctx, "category:wing").expect("wing button");
    click(&ctx, &mut state, wing.center(), PointerButton::Primary, 1);
    assert_eq!(
        state.sandbox.focus(),
        None,
        "a second click shows the whole aircraft"
    );
}

/// The real input path at 125 % display scaling: an 80 px drag is 64 points,
/// turns the camera by the sandbox gain (0.38 degrees per logical point), and leaves
/// the pivot where it is on screen.
#[test]
fn an_orbit_drag_at_125_percent_scaling_turns_gently_about_the_pivot() {
    let mut state = AppState::default();
    assert!(state.enter_sandbox(true));
    let ctx = Context::default();
    ctx.set_pixels_per_point(1.25);
    settle(&ctx, &mut state);
    let start = free_point(&ctx);
    let pivot = state.sandbox.pivot.expect("pivot");
    let (_, before) = state.sandbox.scene.clone().expect("scene");
    let anchor = before.project(pivot);
    let camera_before = *state.preview_camera_mut(SANDBOX_CAMERA_ID);
    // Held, not released: a release may refit an orbit that left the free area.
    frame(&ctx, &mut state, vec![Event::PointerMoved(start)]);
    frame(
        &ctx,
        &mut state,
        vec![button(start, PointerButton::Primary, true, Modifiers::NONE)],
    );
    for step in 1..=4 {
        frame(
            &ctx,
            &mut state,
            vec![Event::PointerMoved(start + vec2(16.0 * step as f32, 0.0))],
        );
    }
    let camera = *state.preview_camera_mut(SANDBOX_CAMERA_ID);
    let turned = (camera.yaw_deg - camera_before.yaw_deg).rem_euclid(360.0);
    assert!(
        (turned - 64.0 * 0.38).abs() < 3.0,
        "turned {turned} degrees"
    );
    let (_, after) = state.sandbox.scene.clone().expect("scene");
    let moved = after.project(pivot);
    assert!((moved[0] - anchor[0]).abs() < 1e-6 && (moved[1] - anchor[1]).abs() < 1e-6);
}

/// A double-click on a wingtip, where the span handle sits, still sets the
/// pivot there, and an orbit afterwards leaves the tip where it is.
#[test]
fn a_double_click_on_a_wingtip_handle_pivots_about_the_tip() {
    let (ctx, mut state) = opened();
    let (_, before) = state.sandbox.scene.clone().expect("scene");
    let view = viewport_rect(&ctx).expect("viewport");
    let plane = state.sandbox.airplane.as_ref().expect("plane");
    let wing = &plane.wings[0];
    let tip = wing.xsecs.last().expect("tip").xyz_le;
    let [px, py] = before.project(tip);
    let target = pos2(view.left() + px as f32, view.top() + py as f32);
    click(&ctx, &mut state, target, PointerButton::Primary, 2);
    let pivot = state.sandbox.pivot.expect("pivot");
    assert!(
        (pivot[1] - tip[1]).abs() < 3.0,
        "the pivot is at the wingtip: {pivot:?} vs {tip:?}"
    );
    assert!(state.sandbox.pivot_marker_at.is_some(), "a marker is shown");
    let (_, set) = state.sandbox.scene.clone().expect("scene");
    let anchor = set.project(pivot);
    drag_with(
        &ctx,
        &mut state,
        free_point(&ctx),
        PointerButton::Primary,
        vec2(64.0, 0.0),
    );
    let (_, after) = state.sandbox.scene.clone().expect("scene");
    let moved = after.project(pivot);
    assert!((moved[0] - anchor[0]).abs() < 1e-6 && (moved[1] - anchor[1]).abs() < 1e-6);
}

/// Choosing a preset refits it at zoom 1, whatever zoom the previous view had.
#[test]
fn a_preset_refits_at_unit_zoom() {
    let mut state = AppState::default();
    assert!(state.enter_sandbox(true));
    state.set_sandbox_viewport_size((1280.0, 800.0));
    state.preview_camera_mut(SANDBOX_CAMERA_ID).zoom = 3.0;
    *state.preview_camera_mut(SANDBOX_CAMERA_ID) = alas_gui::state::PreviewCamera::top();
    state.refit_sandbox_framing();
    assert_eq!(state.preview_camera_mut(SANDBOX_CAMERA_ID).zoom, 1.0);
    state.show_whole_aircraft();
    assert_eq!(
        *state.preview_camera_mut(SANDBOX_CAMERA_ID),
        alas_gui::state::PreviewCamera::isometric()
    );
}
