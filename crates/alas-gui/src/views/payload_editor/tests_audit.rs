// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Page behaviour tests: the page must not rewrite a case by being drawn, a
//! whole mouse drag is one undo step, and declared holds bound the placement.

use super::*;
use side_view::SideView;

fn draw(state: &mut AppState, ctx: &egui::Context, events: Vec<egui::Event>) {
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(1_300.0, 1_500.0),
        )),
        events,
        ..Default::default()
    };
    let _ = ctx.run(raw, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| show(state, ui));
    });
}

/// A movable point mass in the hold with a narrow declared balance range.
fn narrow_case(state: &mut AppState) -> (SideView, PayloadItemsConfig) {
    let config = state.typed_config().expect("config");
    let view = side_view::build(state, &config).expect("view");
    let x = view.cabin_x[1] - 3.0;
    let z = f64::midpoint(view.hold_floor_z(x), view.hold_ceiling_z(x));
    let mut items = PayloadItemsConfig::default();
    let index = placement::place_point_mass(&mut items, &view, x, z).unwrap();
    let mass = &mut items.point_masses[index];
    mass.mass_kg = 500.0;
    mass.movable = true;
    mass.x_range_m = Some([x - 0.1, x + 0.1]);
    state.config_values["cabin"]["payload_items"] = serde_json::to_value(&items).unwrap();
    (view, items)
}

#[test]
fn drawing_the_page_leaves_a_declared_narrow_range_and_the_undo_history_alone() {
    let mut state = AppState::default();
    let (_, items) = narrow_case(&mut state);
    state.reset_undo_baseline();
    let ctx = egui::Context::default();
    for _ in 0..3 {
        draw(&mut state, &ctx, Vec::new());
    }
    assert_eq!(read_items(&state), items, "viewing changes nothing");
    assert!(!state.can_shell_undo(), "no phantom undo step");
}

#[test]
fn a_whole_mouse_drag_on_the_canvas_is_one_undo_step() {
    let mut state = AppState::default();
    let (view, items) = narrow_case(&mut state);
    // A movable item with a wide range to drag along.
    let mut wide = items.clone();
    wide.point_masses[0].x_range_m = Some(
        view.region_at(wide.point_masses[0].x_m, wide.point_masses[0].z_m)
            .unwrap()
            .x,
    );
    state.config_values["cabin"]["payload_items"] = serde_json::to_value(&wide).unwrap();
    state.reset_undo_baseline();
    let start = wide.point_masses[0].clone();
    let ctx = egui::Context::default();
    draw(&mut state, &ctx, Vec::new());
    draw(&mut state, &ctx, Vec::new());
    let rect: egui::Rect = ctx
        .data(|data| data.get_temp(canvas::canvas_rect_id()))
        .expect("the canvas was drawn");
    let config = state.typed_config().unwrap();
    let view = side_view::build(&state, &config).unwrap();
    // The window the page fitted for this canvas (the whole airframe).
    let window = ctx
        .data(|data| data.get_temp::<EditorUi>(ui_id()))
        .unwrap_or_default()
        .window;
    let frame = canvas::Frame::windowed(rect, &view, window);
    let at = |x: f64| frame.to_screen(x, start.z_m);
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    draw(
        &mut state,
        &ctx,
        vec![egui::Event::PointerMoved(at(start.x_m))],
    );
    draw(&mut state, &ctx, vec![button(at(start.x_m), true)]);
    for step in 1..=20 {
        let x = start.x_m - 0.2 * f64::from(step);
        draw(&mut state, &ctx, vec![egui::Event::PointerMoved(at(x))]);
    }
    assert_eq!(
        read_items(&state).point_masses[0].x_m,
        start.x_m,
        "nothing is committed while the button is held"
    );
    let end = at(start.x_m - 4.0);
    draw(
        &mut state,
        &ctx,
        vec![egui::Event::PointerMoved(end), button(end, false)],
    );
    draw(&mut state, &ctx, Vec::new());
    let moved = read_items(&state).point_masses[0].x_m;
    assert!(
        moved < start.x_m - 1.0,
        "the item followed the mouse: {moved}"
    );
    assert!(state.shell_undo(), "one step back");
    assert_eq!(read_items(&state).point_masses[0].x_m, start.x_m);
    assert!(!state.can_shell_undo(), "and only one");
}

#[test]
fn declared_hold_compartments_are_the_only_hold_regions_and_are_drawn() {
    let mut view = side_view::test_body();
    view.declared_holds = true;
    view.hold_spans = vec![[8.0, 14.0], [24.0, 30.0]];
    assert_eq!(view.region_at(10.0, -1.0).map(|r| r.x), Some([8.0, 14.0]));
    assert_eq!(view.region_at(27.0, -1.0).map(|r| r.x), Some([24.0, 30.0]));
    assert!(view.region_at(16.0, -1.0).is_none(), "between compartments");
    assert!(view.region_at(33.0, -1.0).is_none(), "past the last one");
    let mut items = PayloadItemsConfig::default();
    assert!(placement::place_point_mass(&mut items, &view, 10.04, -1.0).is_ok());
    // A point mass is not tied to a compartment: the envelope takes it.
    assert!(placement::place_point_mass(&mut items, &view, 16.0, -1.0).is_ok());
    placement::drag_point_mass(&mut items.point_masses[0], &view, 25.0, -1.0);
    assert_eq!(items.point_masses[0].x_m, 25.0);
}

#[test]
fn the_declared_compartments_of_a_case_reach_the_view() {
    let mut state = AppState::default();
    state.config_values["cabin"]["cargo"]["hold_compartments"] = serde_json::json!([
        {"name": "Fwd", "x_start_m": 8.0, "x_end_m": 12.0, "deck": "lower"},
        {"name": "Aft", "x_start_m": 22.0, "x_end_m": 26.0, "deck": "lower"},
    ]);
    let config = state.typed_config().expect("config");
    let view = side_view::build(&state, &config).expect("view");
    assert!(view.declared_holds);
    assert_eq!(view.hold_spans, vec![[8.0, 12.0], [22.0, 26.0]]);
}

#[test]
fn one_side_view_rebuild_is_timed_and_bounded() {
    // Measured: the rebuild runs on the interface thread on
    // a configuration change, never per frame (the view is cached by the
    // configuration's fingerprint and reused while a gesture is held).
    // Needs data: the timing is not printed or recorded; the figure of
    // record (31 ms, optimized test profile) is not re-measured in this sprint.
    let state = AppState::default();
    let config = state.typed_config().unwrap();
    let started = std::time::Instant::now();
    let view = side_view::build(&state, &config).expect("view");
    let elapsed = started.elapsed();
    assert!(!view.items.is_empty());
    assert!(elapsed.as_secs_f64() < 2.0, "{elapsed:?}");
}

#[test]
fn drawing_the_page_leaves_a_declared_range_wider_than_its_region_alone_and_reports_it() {
    let mut state = AppState::default();
    let (view, mut items) = narrow_case(&mut state);
    // A declared range that crosses out of the hold region (the case says so).
    let x = items.point_masses[0].x_m;
    let region = view.region_at(x, items.point_masses[0].z_m).unwrap().x;
    items.point_masses[0].x_range_m = Some([region[0] - 3.0, region[1] + 3.0]);
    state.config_values["cabin"]["payload_items"] = serde_json::to_value(&items).unwrap();
    state.reset_undo_baseline();
    let ctx = egui::Context::default();
    for _ in 0..3 {
        draw(&mut state, &ctx, Vec::new());
    }
    assert_eq!(read_items(&state), items, "drawing rewrote nothing");
    assert!(!state.can_shell_undo());
    assert!(placement::ranges_leaving_regions(&items, &view).is_some());
}

#[test]
fn a_side_view_is_rebuilt_when_a_run_arrives_or_the_design_values_change() {
    let mut state = AppState::default();
    let before = fingerprints(&mut state);
    state.pipeline_result_complete = true;
    let after_run = fingerprints(&mut state);
    assert_ne!(before.0, after_run.0, "a completed run changes the key");
    state.pipeline_result_complete = false;
    if let Some(value) = state.design_values.values_mut().next() {
        *value += 1.0;
    }
    let after_design = fingerprints(&mut state);
    assert_ne!(before.0, after_design.0, "design values change the key");
    assert_ne!(before.1, after_design.1);
}
