// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;
use side_view::test_body;

fn context() -> egui::Context {
    egui::Context::default()
}

fn frame_ui<R>(ctx: &egui::Context, add: impl FnOnce(&mut Ui) -> R) -> R {
    let mut out = None;
    let mut add = Some(add);
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(1_200.0, 1_400.0),
            )),
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                if let Some(add) = add.take() {
                    out = Some(add(ui));
                }
            });
        },
    );
    out.expect("the closure ran")
}

#[test]
fn the_declared_items_round_trip_through_the_cabin_node_and_empty_removes_them() {
    let mut state = AppState::default();
    let view = test_body();
    let mut items = PayloadItemsConfig::default();
    placement::place_point_mass(&mut items, &view, 12.0, 0.5).unwrap();
    assert!(write_items(&mut state, &items));
    assert!(!write_items(&mut state, &items), "unchanged is no edit");
    assert_eq!(read_items(&state).point_masses.len(), 1);
    assert!(write_items(&mut state, &PayloadItemsConfig::default()));
    assert!(state
        .config_values
        .pointer("/cabin/payload_items")
        .is_none());
}

#[test]
fn the_geometry_key_ignores_the_declared_items_and_the_full_key_does_not() {
    let mut state = AppState::default();
    let (full, geometry) = fingerprints(&mut state);
    let mut items = PayloadItemsConfig::default();
    placement::place_point_mass(&mut items, &test_body(), 12.0, 0.5).unwrap();
    write_items(&mut state, &items);
    let (full_after, geometry_after) = fingerprints(&mut state);
    assert_ne!(full, full_after);
    assert_eq!(geometry, geometry_after);
    // The items are back in place after the key was taken.
    assert_eq!(read_items(&state).point_masses.len(), 1);
}

#[test]
fn the_built_side_view_covers_the_body_floor_and_holds_of_the_default_aircraft() {
    let state = AppState::default();
    let config = state.typed_config().expect("config");
    let view = side_view::build(&state, &config).expect("side view");
    let [x0, x1] = view.x_range();
    assert!(x1 - x0 > 10.0);
    assert!(view.cabin_x[0] >= x0 && view.cabin_x[1] <= x1);
    let mid = f64::midpoint(view.cabin_x[0], view.cabin_x[1]);
    assert!(
        view.ceiling_z(mid) > view.floor_z(mid),
        "headroom above the floor"
    );
    assert!(view.top_z(mid) > view.ceiling_z(mid) - 1e-6);
    assert!(view.bottom_z(mid) < view.floor_z(mid));
    assert!(
        view.items.iter().any(|item| item.mass > 0.0),
        "seats and bags"
    );
    assert!(view
        .region_at(mid, f64::midpoint(view.floor_z(mid), view.ceiling_z(mid)))
        .is_some());
}

#[test]
fn a_placed_item_reaches_the_built_layout_and_moves_the_payload_centre_of_gravity() {
    let mut state = AppState::default();
    let config = state.typed_config().expect("config");
    let before = side_view::build(&state, &config).expect("before");
    let mid = f64::midpoint(before.cabin_x[0], before.cabin_x[1]);
    let z = f64::midpoint(before.floor_z(mid), before.ceiling_z(mid));
    let mut items = PayloadItemsConfig::default();
    let index =
        placement::place_point_mass(&mut items, &before, before.cabin_x[1] - 1.0, z).unwrap();
    items.point_masses[index].mass_kg = 5_000.0;
    write_items(&mut state, &items);
    let config = state.typed_config().expect("config with item");
    assert!(
        alas_config::validate(&config)
            .iter()
            .all(|issue| issue.field_path != "cabin.payload_items"),
        "{:?}",
        alas_config::validate(&config)
    );
    let after = side_view::build(&state, &config).expect("after");
    assert!(
        after.cg_x > before.cg_x,
        "{} -> {}",
        before.cg_x,
        after.cg_x
    );
    assert!(after.items.iter().any(|item| item.declared == "mass-1"));
}

#[test]
fn a_refused_click_reports_and_adds_nothing_and_an_accepted_one_disarms() {
    let ctx = context();
    let view = test_body();
    let mut editor = EditorUi {
        mode: Mode::PointMass,
        ..EditorUi::default()
    };
    let mut items = PayloadItemsConfig::default();
    frame_ui(&ctx, |ui| {
        apply(
            Action::Click { x: 12.0, z: 9.0 },
            ui,
            &mut editor,
            &mut items,
            &view,
            false,
        );
    });
    assert!(items.is_empty());
    assert_eq!(editor.mode, Mode::PointMass, "still armed after a refusal");
    assert_eq!(
        editor.status.as_ref().map(|s| s.0.as_str()),
        Some(crate::views::tr_label("No Valid Station Here").as_str())
    );
    frame_ui(&ctx, |ui| {
        apply(
            Action::Click { x: 12.04, z: 0.5 },
            ui,
            &mut editor,
            &mut items,
            &view,
            false,
        );
    });
    assert_eq!(items.point_masses.len(), 1);
    assert_eq!(items.point_masses[0].x_m, 12.0);
    assert_eq!(editor.mode, Mode::Idle);
    assert_eq!(editor.selected, Some(Target::PointMass(0)));
}

#[test]
fn lock_layout_blocks_placing_and_dragging() {
    let ctx = context();
    let view = test_body();
    let mut editor = EditorUi {
        mode: Mode::Zone,
        ..EditorUi::default()
    };
    let mut items = PayloadItemsConfig::default();
    placement::place_point_mass(&mut items, &view, 30.0, -1.0).unwrap();
    let before = items.clone();
    frame_ui(&ctx, |ui| {
        apply(
            Action::Click { x: 12.0, z: 0.5 },
            ui,
            &mut editor,
            &mut items,
            &view,
            true,
        );
        apply(
            Action::DragStart {
                target: Target::PointMass(0),
                x: 30.0,
            },
            ui,
            &mut editor,
            &mut items,
            &view,
            true,
        );
        apply(
            Action::DragTo { x: 25.0, z: -1.0 },
            ui,
            &mut editor,
            &mut items,
            &view,
            true,
        );
    });
    assert_eq!(items, before);
    assert!(editor.drag.is_none());
    assert_eq!(
        editor.status.as_ref().map(|s| s.0.as_str()),
        Some(crate::views::tr_label("Layout Locked").as_str())
    );
}

#[test]
fn a_drag_from_press_to_release_moves_the_item_inside_the_envelope() {
    let ctx = context();
    let view = test_body();
    let mut editor = EditorUi::default();
    let mut items = PayloadItemsConfig::default();
    placement::place_point_mass(&mut items, &view, 30.0, -1.0).unwrap();
    frame_ui(&ctx, |ui| {
        apply(
            Action::DragStart {
                target: Target::PointMass(0),
                x: 30.0,
            },
            ui,
            &mut editor,
            &mut items,
            &view,
            false,
        );
        apply(
            Action::DragTo { x: 33.33, z: -0.9 },
            ui,
            &mut editor,
            &mut items,
            &view,
            false,
        );
        apply(
            Action::DragTo { x: 5.0, z: -0.9 },
            ui,
            &mut editor,
            &mut items,
            &view,
            false,
        );
    });
    assert_eq!(items.point_masses[0].x_m, 5.0, "followed the pointer");
    frame_ui(&ctx, |ui| {
        apply(Action::DragEnd, ui, &mut editor, &mut items, &view, false);
    });
    assert!(editor.drag.is_none());
}

#[test]
fn the_page_draws_the_toolbar_the_view_and_the_items_table() {
    fn collect(shape: &egui::Shape, out: &mut Vec<String>) {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, out)),
            egui::Shape::Text(text) => out.push(text.galley.text().to_owned()),
            _ => {}
        }
    }
    let mut state = AppState::default();
    let config = state.typed_config().unwrap();
    let view = side_view::build(&state, &config).unwrap();
    let mid = f64::midpoint(view.cabin_x[0], view.cabin_x[1]);
    let z = f64::midpoint(view.floor_z(mid), view.ceiling_z(mid));
    let mut items = PayloadItemsConfig::default();
    placement::place_point_mass(&mut items, &view, mid, z).unwrap();
    write_items(&mut state, &items);
    let ctx = context();
    let mut texts = Vec::new();
    for _ in 0..2 {
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    egui::vec2(1_200.0, 1_400.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| show(&mut state, ui));
            },
        );
        texts.clear();
        for clipped in &output.shapes {
            collect(&clipped.shape, &mut texts);
        }
    }
    for expected in ["+ Zone", "+ Point Mass", "Lock Layout", "Point Masses"] {
        assert!(
            texts.iter().any(|t| *t == crate::views::tr_label(expected)),
            "{expected} in {texts:?}"
        );
    }
    // Save and Load sit in the Layout menu of the toolbar.
    let layout_menu = format!("{} \u{25be}", crate::views::tr_label("Layout"));
    assert!(texts.contains(&layout_menu), "{layout_menu} in {texts:?}");
    assert!(texts.iter().any(|t| t == "mass-1"), "{texts:?}");
    assert!(texts.iter().any(|t| t.starts_with("Zero-fuel CG")));
}
