// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Payload Release stage card: every payload item as a table row with a
//! "Released here" tick, the mass released at the stage, and the step that
//! marks a ticked point mass releasable. The profile plot marks the release.

use alas_config::{EventAction, MissionPlan};
use egui::{RichText, Ui};
use serde_json::Value;

use crate::state::AppState;
use crate::views::{tr_fields, tr_label};

/// A payload item a release may name: a declared point mass, or a store a
/// stage hangs on the aircraft.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct PayloadItem {
    pub id: String,
    pub name: String,
    pub mass_kg: f64,
    /// Whether a release may name it already. A declared point mass that is
    /// not yet releasable is marked so when the table ticks it.
    pub releasable: bool,
}

/// The point masses declared on the Payload page, in their order.
pub(super) fn payload_items(state: &AppState) -> Vec<PayloadItem> {
    state
        .typed_config_arc()
        .map(|config| {
            config
                .cabin
                .payload_items
                .point_masses
                .iter()
                .map(|item| PayloadItem {
                    id: item.id.clone(),
                    name: item.name.clone(),
                    mass_kg: item.mass_kg,
                    releasable: item.releasable,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The stores the stages of `plan` hang on the aircraft, in flight order.
pub(super) fn plan_stores(plan: &MissionPlan) -> Vec<PayloadItem> {
    plan.stages
        .iter()
        .flat_map(|stage| stage.events.iter())
        .filter_map(|event| match &event.action {
            EventAction::StoreAdd { id, mass_kg, .. } => Some(PayloadItem {
                id: id.clone(),
                name: id.clone(),
                mass_kg: *mass_kg,
                releasable: true,
            }),
            _ => None,
        })
        .collect()
}

/// Mark the declared point masses `ids` releasable, as a ticked row of the
/// Payload Release table does, so there is no trip back to the Payload page.
/// Returns whether any item changed.
pub(super) fn mark_releasable(state: &mut AppState, ids: &[String]) -> bool {
    let Some(masses) = state
        .config_values
        .pointer_mut("/cabin/payload_items/point_masses")
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    let mut changed = false;
    for item in masses.iter_mut().filter_map(Value::as_object_mut) {
        let named = item
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| ids.iter().any(|other| other == id));
        if named && item.get("releasable").and_then(Value::as_bool) != Some(true) {
            item.insert("releasable".to_owned(), Value::Bool(true));
            changed = true;
        }
    }
    if changed {
        state.on_config_modified();
    }
    changed
}

/// For each declared point mass a Payload Release stage drops, the 1-based
/// number of that stage in the plan. A release that names no items drops every
/// releasable item not named by another release, as the solver does.
pub(super) fn release_stages(
    plan: &MissionPlan,
    items: &[PayloadItem],
) -> std::collections::BTreeMap<String, usize> {
    let mut found = std::collections::BTreeMap::new();
    let releases = || {
        plan.stages.iter().enumerate().flat_map(|(index, stage)| {
            stage
                .events
                .iter()
                .filter_map(move |event| match &event.action {
                    EventAction::PayloadRelease { items } => Some((index + 1, items)),
                    _ => None,
                })
        })
    };
    for (number, ids) in releases() {
        for id in ids {
            found.entry(id.clone()).or_insert(number);
        }
    }
    for number in releases().filter(|(_, ids)| ids.is_empty()).map(|(n, _)| n) {
        for item in items.iter().filter(|item| item.releasable) {
            found.entry(item.id.clone()).or_insert(number);
        }
    }
    found
}

/// What the Payload Release table reports back to the editor.
#[derive(Debug, Default)]
pub(super) struct ReleaseEdit {
    /// The event's item list changed.
    pub changed: bool,
    /// Declared point masses that were not releasable and were just ticked.
    pub newly_releasable: Vec<String>,
}

/// Mass released by an event that names `ids`: those items, or, for an empty
/// list, every releasable item not yet released elsewhere.
pub(super) fn released_mass_kg(
    items: &[PayloadItem],
    ids: &[String],
    elsewhere: &[(String, String)],
) -> f64 {
    items
        .iter()
        .filter(|item| {
            if ids.is_empty() {
                item.releasable && !elsewhere.iter().any(|(id, _)| id == &item.id)
            } else {
                ids.contains(&item.id)
            }
        })
        .map(|item| item.mass_kg)
        .sum()
}

/// The Payload Release editor: one row per payload item (name, mass and a
/// "Released here" tick). Ticking an item names it in the event and marks it
/// releasable, so the release is defined entirely from this card.
///
/// `elsewhere` lists `(item id, stage label)` of items that other stages
/// already release: an item can leave the aircraft once.
pub(super) fn table(
    ui: &mut Ui,
    items: &[PayloadItem],
    ids: &mut Vec<String>,
    elsewhere: &[(String, String)],
) -> ReleaseEdit {
    let mut edit = ReleaseEdit::default();
    if items.is_empty() {
        ui.label(tr_label("No Payload Items"));
        ui.label(
            RichText::new(tr_label(
                "Add a point mass on the Payload page; it appears here to be released.",
            ))
            .small()
            .weak(),
        );
        return edit;
    }
    ui.label(
        RichText::new(tr_label(
            "Tick the items this stage drops. A ticked item is marked releasable automatically.",
        ))
        .small()
        .weak(),
    );
    egui::Grid::new("payload_release_table")
        .num_columns(3)
        .spacing(crate::theme::space::GRID_SPACING)
        .striped(true)
        .show(ui, |ui| {
            ui.label(RichText::new(tr_label("Item")).strong());
            ui.label(RichText::new(tr_label("Mass [kg]")).strong());
            ui.label(RichText::new(tr_label("Released Here")).strong());
            ui.end_row();
            for item in items {
                let other = elsewhere
                    .iter()
                    .find(|(id, _)| id == &item.id)
                    .map(|(_, stage)| stage.as_str());
                let name = if item.name.is_empty() || item.name == item.id {
                    item.id.clone()
                } else {
                    format!("{} ({})", item.name, item.id)
                };
                ui.label(name);
                ui.label(format!("{:.0}", item.mass_kg));
                let mut released = ids.contains(&item.id);
                ui.horizontal(|ui| {
                    let tick = ui
                        .add_enabled_ui(other.is_none() || released, |ui| {
                            crate::checkbox::checkbox(ui, &mut released, "")
                        })
                        .inner;
                    if tick.changed() {
                        edit.changed = true;
                        if released {
                            ids.push(item.id.clone());
                            if !item.releasable {
                                edit.newly_releasable.push(item.id.clone());
                            }
                        } else {
                            ids.retain(|other| other != &item.id);
                        }
                    }
                    if let Some(stage) = other {
                        ui.label(
                            RichText::new(tr_fields(
                                "Released at {stage}",
                                &[("stage", stage.to_owned())],
                            ))
                            .small()
                            .color(ui.visuals().warn_fg_color),
                        );
                    }
                });
                ui.end_row();
            }
        });
    let mass = released_mass_kg(items, ids, elsewhere);
    let text = if ids.is_empty() {
        tr_fields(
            "Nothing ticked: releases every releasable item still carried ({mass} kg)",
            &[("mass", format!("{mass:.0}"))],
        )
    } else {
        tr_fields(
            "Released at this stage: {mass} kg",
            &[("mass", format!("{mass:.0}"))],
        )
    };
    ui.label(RichText::new(text).strong());
    edit
}
