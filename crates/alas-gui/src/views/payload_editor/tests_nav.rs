// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Navigation of the two canvases through the page: the wheel zooms, a drag on
//! empty canvas pans, and dragging a point mass still moves it, in both views.

use super::*;

fn draw(state: &mut AppState, ctx: &egui::Context, events: Vec<egui::Event>) {
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(1_400.0, 1_000.0),
        )),
        events,
        ..Default::default()
    };
    let _ = ctx.run(raw, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| show(state, ui));
    });
}

fn editor(ctx: &egui::Context) -> EditorUi {
    ctx.data(|data| data.get_temp(ui_id())).unwrap_or_default()
}

fn set_editor(ctx: &egui::Context, edit: impl FnOnce(&mut EditorUi)) {
    let mut current = editor(ctx);
    edit(&mut current);
    ctx.data_mut(|data| data.insert_temp(ui_id(), current));
}

fn canvas_rect(ctx: &egui::Context) -> egui::Rect {
    ctx.data(|data| data.get_temp(canvas::canvas_rect_id()))
        .expect("the canvas was drawn")
}

fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

fn wheel(delta: f32) -> egui::Event {
    egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, delta),
        modifiers: egui::Modifiers::NONE,
    }
}

/// A page with one movable-anywhere point mass in the hold and its view.
fn with_mass(state: &mut AppState) -> (SideView, usize) {
    let config = state.typed_config().expect("config");
    let view = side_view::build(state, &config).expect("view");
    let x = view.cabin_x[1] - 3.0;
    let z = f64::midpoint(view.hold_floor_z(x), view.hold_ceiling_z(x));
    let mut items = PayloadItemsConfig::default();
    let index = placement::place_point_mass(&mut items, &view, x, z).unwrap();
    write_items(state, &items);
    (view, index)
}

#[test]
fn the_wheel_over_the_side_view_zooms_in_and_fit_returns_to_the_whole_body() {
    let mut state = AppState::default();
    let ctx = egui::Context::default();
    draw(&mut state, &ctx, Vec::new());
    draw(&mut state, &ctx, Vec::new());
    let rect = canvas_rect(&ctx);
    let over = rect.center();
    draw(&mut state, &ctx, vec![egui::Event::PointerMoved(over)]);
    assert!((editor(&ctx).window.zoom - 1.0).abs() < 1e-9);
    draw(&mut state, &ctx, vec![wheel(300.0)]);
    let zoom = editor(&ctx).window.zoom;
    assert!(zoom > 1.3, "the wheel zoomed in: {zoom}");
    set_editor(&ctx, |editor| editor.window = canvas::Window::default());
    assert!((editor(&ctx).window.zoom - 1.0).abs() < 1e-9);
}

#[test]
fn a_drag_on_empty_canvas_pans_and_a_drag_on_an_item_moves_the_item() {
    let mut state = AppState::default();
    let (_view, index) = with_mass(&mut state);
    let mass = read_items(&state).point_masses[index].clone();
    let ctx = egui::Context::default();
    set_editor(&ctx, |editor| {
        editor.window = canvas::Window {
            zoom: 3.0,
            pan: 0.5,
            centre: None,
            // As a wheel zoom would leave it: a window nobody moved is refitted.
            touched: true,
            ..canvas::Window::default()
        };
    });
    draw(&mut state, &ctx, Vec::new());
    draw(&mut state, &ctx, Vec::new());
    let rect = canvas_rect(&ctx);
    let window = editor(&ctx).window;
    // Empty canvas: well above the fuselage.
    let empty = egui::pos2(rect.center().x, rect.top() + 4.0);
    draw(&mut state, &ctx, vec![egui::Event::PointerMoved(empty)]);
    draw(&mut state, &ctx, vec![button(empty, true)]);
    let to = empty + egui::vec2(-120.0, 0.0);
    for step in 1..=6 {
        let at = empty + egui::vec2(-20.0 * step as f32, 0.0);
        draw(&mut state, &ctx, vec![egui::Event::PointerMoved(at)]);
    }
    draw(
        &mut state,
        &ctx,
        vec![egui::Event::PointerMoved(to), button(to, false)],
    );
    draw(&mut state, &ctx, Vec::new());
    let panned = editor(&ctx).window;
    assert!(panned.pan > window.pan, "dragging left moves the view on");
    assert_eq!(
        read_items(&state).point_masses[index],
        mass,
        "no item moved"
    );
}

#[test]
fn a_point_mass_is_dragged_in_the_front_view_and_the_wheel_zooms_it() {
    let mut state = AppState::default();
    let (view, index) = with_mass(&mut state);
    let mass = read_items(&state).point_masses[index].clone();
    let ctx = egui::Context::default();
    set_editor(&ctx, |editor| {
        editor.view_mode = ViewMode::Front;
        editor.section_x = Some(mass.x_m);
    });
    draw(&mut state, &ctx, Vec::new());
    draw(&mut state, &ctx, Vec::new());
    let rect = canvas_rect(&ctx);
    let camera = editor(&ctx).front;
    let at = |y: f64, z: f64| front_view::screen_of(rect, &view, mass.x_m, camera, y, z);
    draw(
        &mut state,
        &ctx,
        vec![egui::Event::PointerMoved(at(mass.y_m, mass.z_m))],
    );
    draw(&mut state, &ctx, vec![button(at(mass.y_m, mass.z_m), true)]);
    for step in 1..=50 {
        let y = mass.y_m + 0.02 * f64::from(step);
        draw(
            &mut state,
            &ctx,
            vec![egui::Event::PointerMoved(at(y, mass.z_m))],
        );
    }
    let end = at(mass.y_m + 1.0, mass.z_m);
    draw(
        &mut state,
        &ctx,
        vec![egui::Event::PointerMoved(end), button(end, false)],
    );
    draw(&mut state, &ctx, Vec::new());
    let moved = read_items(&state).point_masses[index].clone();
    assert!(moved.y_m > mass.y_m + 0.3, "{} -> {}", mass.y_m, moved.y_m);
    assert_eq!(editor(&ctx).front, camera, "the view did not pan");
    // Wheel.
    draw(
        &mut state,
        &ctx,
        vec![egui::Event::PointerMoved(rect.center())],
    );
    draw(&mut state, &ctx, vec![wheel(-400.0)]);
    assert!(editor(&ctx).front.zoom < 1.0, "zoomed out");
}
