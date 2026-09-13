// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Parameter Panel that replaces navigation in the sandbox.
//!
//! Disciplines are collapsible headers, each with a focus toggle that
//! isolates the component in the preview and an action that opens its
//! Discipline Window. Groups inside a discipline are collapsible cards of
//! the shared editors. A search box filters fields by label, group or
//! discipline across the whole tree.

use egui::{RichText, ScrollArea, TextEdit, Ui};

use crate::state::AppState;
use crate::views::tr;

use super::editors::show_group;
use super::fields::{grouped, Discipline, SandboxField};

fn matches(field: &SandboxField, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let needle = needle.to_ascii_lowercase();
    tr(&field.label).to_ascii_lowercase().contains(&needle)
        || field.id.to_ascii_lowercase().contains(&needle)
        || tr(field.group).to_ascii_lowercase().contains(&needle)
        || tr(field.discipline.title())
            .to_ascii_lowercase()
            .contains(&needle)
}

/// Open a discipline window and focus its component.
pub fn open_discipline_window(state: &mut AppState, discipline: Discipline) {
    let id = discipline.id().to_owned();
    if !state.sandbox.layout.open_disciplines.contains(&id) {
        state.sandbox.layout.open_disciplines.push(id);
    }
    state.sandbox.set_focus(Some(discipline));
    state.reproject_sandbox_scene();
}

/// Render the Parameter Panel.
pub fn show_parameter_panel(state: &mut AppState, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(tr("Parameters"))
                .strong()
                .color(ui.visuals().hyperlink_color),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::new(tr("Advanced Settings...")).small())
                .on_hover_text(tr(
                    "Open the detached Advanced Settings window with discipline tabs.",
                ))
                .clicked()
            {
                state.sandbox.layout.advanced_settings_open = true;
            }
        });
    });
    ui.add(
        TextEdit::singleline(&mut state.sandbox.search)
            .hint_text(tr("Search parameters"))
            .desired_width(ui.available_width()),
    );
    ui.add_space(4.0);
    let needle = state.sandbox.search.trim().to_owned();
    let fields = state.sandbox.fields.clone();
    ScrollArea::vertical()
        .id_salt("sandbox_parameter_panel")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for discipline in Discipline::ALL {
                let visible: Vec<(&str, Vec<SandboxField>)> = grouped(&fields, discipline)
                    .into_iter()
                    .map(|(title, members)| {
                        (
                            title,
                            members
                                .into_iter()
                                .filter(|f| matches(f, &needle))
                                .cloned()
                                .collect::<Vec<_>>(),
                        )
                    })
                    .filter(|(_, members)| !members.is_empty())
                    .collect();
                if visible.is_empty() {
                    continue;
                }
                let focused = state.sandbox.focus() == Some(discipline);
                egui::CollapsingHeader::new(
                    RichText::new(tr(discipline.title())).strong().size(14.0),
                )
                .id_salt(("sandbox_discipline", discipline.id()))
                .default_open(!needle.is_empty() || discipline == Discipline::Wing)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .add(crate::theme::selectable_button(tr("Focus"), focused))
                            .on_hover_text(tr(
                                "Show only this component in the preview and fit the camera to it.",
                            ))
                            .clicked()
                        {
                            state
                                .sandbox
                                .set_focus(if focused { None } else { Some(discipline) });
                            state.reproject_sandbox_scene();
                        }
                        if ui
                            .add(egui::Button::new(tr("Open editor")).small())
                            .on_hover_text(tr("Open this discipline in its own window."))
                            .clicked()
                        {
                            open_discipline_window(state, discipline);
                        }
                    });
                    for (title, members) in &visible {
                        let open = !needle.is_empty() || *title == "Planform" || *title == "Body";
                        show_group(
                            state,
                            ui,
                            title,
                            members,
                            &format!("sandbox_panel::{}::{title}", discipline.id()),
                            open,
                        );
                    }
                });
                ui.add_space(2.0);
            }
        });
}
