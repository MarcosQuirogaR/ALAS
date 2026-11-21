// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Payload page's toolbar: the view switch, the add actions, the layout
//! settings and the layout file actions with their status pill.

use egui::{RichText, Ui};

use super::side_view::SideView;
use super::{say, EditorUi, Mode, ViewMode, PILL_SECONDS};
use crate::file_library::{self, ObjectKind};
use crate::state::AppState;
use crate::views::mission_stage_editor::pill;
use crate::views::tr_label;

pub(super) fn toolbar(
    state: &mut AppState,
    ui: &mut Ui,
    editor: &mut EditorUi,
    locked: &mut bool,
    target: &mut Option<f64>,
    view: Option<&SideView>,
) {
    ui.spacing_mut().interact_size.y = 26.0;
    ui.spacing_mut().item_spacing = egui::vec2(4.0, 6.0);
    // A button moves to the next line whole; its label is never wrapped.
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
    // One left-aligned row of groups that wraps whole buttons when narrow.
    ui.horizontal_wrapped(|ui| {
        toolbar_left(ui, editor, locked, target, view, cabin_target_pct(state));
        group_gap(ui);
        custom_payload_toggle(state, ui);
        toolbar_files(state, ui, editor, *locked, view);
    });
}

/// The cabin's target CG, % MAC: the balance target the toolbar starts from, so the page and the
/// cabin agree until the layout sets its own.
fn cabin_target_pct(state: &AppState) -> f64 {
    state
        .typed_config_arc()
        .map(|config| config.cabin.cargo.target_cg_pct_mac)
        .filter(|pct| *pct > 0.0)
        .unwrap_or(25.0)
}

/// The custom-payload switch: no seats and no cargo, only the zones and point
/// masses placed on this page. With nothing placed the run does not size.
fn custom_payload_toggle(state: &mut AppState, ui: &mut Ui) {
    let mut items = super::read_items(state);
    let mut custom = items.custom_only;
    if toggle(ui, &mut custom, "Custom Payload") {
        items.custom_only = custom;
        super::write_items(state, &items);
    }
}

/// A separator with room on both sides, between toolbar groups.
fn group_gap(ui: &mut Ui) {
    ui.add_space(2.0);
    ui.separator();
    ui.add_space(2.0);
}

/// A toggle drawn as a button of the toolbar's height.
fn toggle(ui: &mut Ui, on: &mut bool, label: &str) -> bool {
    let clicked = ui
        .add(crate::theme::selectable_button(tr_label(label), *on))
        .clicked();
    if clicked {
        *on = !*on;
    }
    clicked
}

/// The view switch, the add actions and the layout settings, in groups.
fn toolbar_left(
    ui: &mut Ui,
    editor: &mut EditorUi,
    locked: &mut bool,
    target: &mut Option<f64>,
    view: Option<&SideView>,
    cabin_target: f64,
) {
    let can_edit = !*locked && view.is_some();
    for (mode, label) in [(ViewMode::Side, "Side"), (ViewMode::Front, "Front")] {
        if ui
            .add(crate::theme::selectable_button(
                tr_label(label),
                editor.view_mode == mode,
            ))
            .clicked()
        {
            editor.view_mode = mode;
        }
    }
    group_gap(ui);
    ui.add_enabled_ui(can_edit, |ui| {
        for (mode, label) in [(Mode::Zone, "+ Zone"), (Mode::PointMass, "+ Point Mass")] {
            if mode == Mode::Zone && editor.view_mode == ViewMode::Front {
                continue;
            }
            let armed = editor.mode == mode;
            if ui
                .add(crate::theme::selectable_button(tr_label(label), armed))
                .clicked()
            {
                editor.mode = if armed { Mode::Idle } else { mode };
            }
        }
    });
    group_gap(ui);
    toggle(ui, locked, "Lock Layout");
    let mut stated = target.is_some();
    if toggle(ui, &mut stated, "Balance Target") {
        *target = stated.then_some(cabin_target);
    }
    if let Some(pct) = target {
        ui.add_enabled(
            !*locked,
            egui::DragValue::new(pct).speed(0.1).suffix(" % MAC"),
        );
    }
}

/// The layout file actions and the status pill, at the right.
pub(super) fn toolbar_files(
    state: &mut AppState,
    ui: &mut Ui,
    editor: &mut EditorUi,
    locked: bool,
    view: Option<&SideView>,
) {
    ui.menu_button(format!("{} \u{25be}", tr_label("Layout")), |ui| {
        if ui.button(tr_label("Save Layout")).clicked() {
            let name = state.library_default_name(ObjectKind::PayloadLayout);
            if state.library_save(ObjectKind::PayloadLayout, &name, "") {
                say(editor, ui, tr_label("Layout Saved"), true);
            }
            ui.close_menu();
        }
        load_menu(state, ui);
    });
    status(ui, editor, locked, view);
}

fn load_menu(state: &mut AppState, ui: &mut Ui) {
    ui.menu_button(tr_label("Load"), |ui| {
        let entries = file_library::list(&state.data_dir(), ObjectKind::PayloadLayout);
        if entries.is_empty() {
            ui.label(RichText::new(tr_label("No Saved Layouts")).weak());
        }
        for entry in entries {
            if ui.button(&entry.name).clicked() {
                state.guarded(crate::shell_confirm::Pending::Library(entry));
                ui.close_menu();
            }
        }
    });
}

fn status(ui: &mut Ui, editor: &EditorUi, locked: bool, view: Option<&SideView>) {
    let now = ui.input(|input| input.time);
    if let Some((text, ok, since)) = &editor.status {
        if now - since < PILL_SECONDS {
            pill(ui, text, *ok);
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs(1));
            return;
        }
    }
    if let Some(warning) = &editor.warning {
        pill(ui, warning, false);
        return;
    }
    if locked {
        pill(ui, &tr_label("Layout Locked"), true);
    } else if let Some(balance) = view.and_then(|view| view.balance) {
        if balance.saturated {
            pill(ui, &tr_label("Out Of Range"), false);
        } else if balance.settled {
            pill(ui, &tr_label("Balanced"), true);
        }
    }
}
