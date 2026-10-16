// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The on/off editor of `FieldKind::Bool` fields (the dorsal-fin-in-aero
//! switch) and its validity domain.

use alas_config::ConfigNode;
use egui::{Context, Event, PointerButton, Pos2, RawInput, Rect};
use serde_json::{json, Value};

use super::bool_editor;
use crate::sandbox::fields::{inventory, read_value, value_in_domain, FieldKind, SandboxField};
use crate::state::AppState;

const DORSAL_ID: &str = "geometry.empennage.vstab_dorsal_in_aero";

fn dorsal_field() -> SandboxField {
    let schema = alas_config::AlasConfig::default().schema();
    inventory(&schema)
        .into_iter()
        .find(|field| field.id == DORSAL_ID)
        .expect("the dorsal-fin switch is in the sandbox inventory")
}

fn frame(ctx: &Context, state: &mut AppState, field: &SandboxField, events: Vec<Event>) {
    let current = read_value(field, &state.config_values, &state.design_values);
    let _ = ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 200.0))),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| bool_editor(state, ui, field, &current));
        },
    );
}

/// Press and release the primary button over the checkbox, which the central
/// panel places at its top-left corner.
fn click_checkbox(ctx: &Context, state: &mut AppState, field: &SandboxField) {
    let at = Pos2::new(20.0, 20.0);
    for pressed in [true, false] {
        frame(
            ctx,
            state,
            field,
            vec![
                Event::PointerMoved(at),
                Event::PointerButton {
                    pos: at,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
}

#[test]
fn the_dorsal_switch_is_a_bool_field_accepting_only_booleans_or_unset() {
    let field = dorsal_field();
    assert_eq!(field.kind, FieldKind::Bool);
    assert!(value_in_domain(&field, &Value::Bool(true)));
    assert!(value_in_domain(&field, &Value::Bool(false)));
    assert!(value_in_domain(&field, &Value::Null));
    assert!(!value_in_domain(&field, &json!(1.0)));
    assert!(!value_in_domain(&field, &json!("on")));
}

#[test]
fn clicking_the_bool_editor_commits_each_toggle_to_the_configuration() {
    let field = dorsal_field();
    let mut state = AppState::default();
    let ctx = Context::default();
    frame(&ctx, &mut state, &field, Vec::new());
    let read = |state: &AppState| read_value(&field, &state.config_values, &state.design_values);
    let initial = read(&state).as_bool().unwrap_or(false);

    click_checkbox(&ctx, &mut state, &field);
    assert_eq!(read(&state), Value::Bool(!initial));

    click_checkbox(&ctx, &mut state, &field);
    assert_eq!(read(&state), Value::Bool(initial));
}
