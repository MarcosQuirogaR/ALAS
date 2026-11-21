// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The editors of the Sandbox fields that hold a list of numbers or of pairs.

use egui::{DragValue, RichText, Ui};
use serde_json::Value;

use crate::state::AppState;
use crate::views::tr;

use super::editors::{commit_field, gesture_of, Gesture};
use super::fields::{FieldKind, SandboxField};

pub(super) fn list_editor(
    state: &mut AppState,
    ui: &mut Ui,
    field: &SandboxField,
    current: &Value,
    labels: Option<[&str; 3]>,
) {
    let mut items: Vec<f64> = current
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_f64).collect())
        .unwrap_or_default();
    let mut gesture = None;
    ui.horizontal_wrapped(|ui| {
        for (index, item) in items.iter_mut().enumerate() {
            if let Some(labels) = labels {
                if let Some(label) = labels.get(index) {
                    ui.label(RichText::new(*label).weak().small());
                }
            }
            let response = ui.add(
                DragValue::new(item)
                    .range(field.min..=field.max)
                    .speed(10f64.powi(-(field.decimals as i32)))
                    .max_decimals(field.decimals),
            );
            if let Some(g) = gesture_of(&response) {
                gesture = Some(g);
            }
        }
        if field.kind == FieldKind::FloatList {
            if ui
                .small_button("+")
                .on_hover_text(tr("Add a mirrored engine pair"))
                .clicked()
            {
                let outermost = items.iter().fold(0.0f64, |m, v| m.max(v.abs()));
                items.push(outermost + 3.0);
                items.push(-(outermost + 3.0));
                gesture = Some(Gesture::Committed);
            }
            if items.len() >= 2
                && ui
                    .small_button("-")
                    .on_hover_text(tr("Remove the outermost engine pair"))
                    .clicked()
            {
                items.truncate(items.len() - 2);
                gesture = Some(Gesture::Committed);
            }
        }
    });
    if let Some(gesture) = gesture {
        commit_field(state, field, Value::from(items), gesture);
    }
}

pub(super) fn pair_list_editor(
    state: &mut AppState,
    ui: &mut Ui,
    field: &SandboxField,
    current: &Value,
) {
    let mut rows: Vec<[f64; 2]> = current
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let row = row.as_array()?;
                    Some([row.first()?.as_f64()?, row.get(1)?.as_f64()?])
                })
                .collect()
        })
        .unwrap_or_default();
    let mut gesture = None;
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("x-station [m]")).weak().small());
        ui.label(RichText::new(tr("radius fraction [0-1]")).weak().small());
    });
    for row in rows.iter_mut() {
        ui.horizontal(|ui| {
            for (index, value) in row.iter_mut().enumerate() {
                let (lo, hi) = if index == 0 {
                    (0.0, field.max)
                } else {
                    (0.0, 1.0)
                };
                let response = ui.add(
                    DragValue::new(value)
                        .range(lo..=hi)
                        .speed(0.01)
                        .max_decimals(3),
                );
                if let Some(g) = gesture_of(&response) {
                    gesture = Some(g);
                }
            }
        });
    }
    if let Some(gesture) = gesture {
        let value = Value::from(
            rows.into_iter()
                .map(|r| Value::from(r.to_vec()))
                .collect::<Vec<_>>(),
        );
        commit_field(state, field, value, gesture);
    }
}
