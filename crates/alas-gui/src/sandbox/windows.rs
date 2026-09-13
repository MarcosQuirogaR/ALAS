// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The sandbox's floating windows: Discipline Windows, the run log, the
//! Full Analysis results, and the leave-sandbox prompt.
//!
//! A Discipline Window shows the same grouped editors as the Parameter
//! Panel; the fuselage window adds the section editor. Pressing the pointer
//! inside a window links the preview to that discipline, so several open
//! editors follow whichever one is being used. The run log starts hidden,
//! opens when a sandbox analysis starts, and can be minimized (collapsed),
//! closed and reopened without touching the run.

use egui::{Context, Id, RichText, ScrollArea, Window};

use crate::state::AppState;
use crate::views::{show_results_view, show_run_log, tr};

use super::editors::show_group;
use super::fields::{grouped, Discipline};
use super::fuselage_editor::show_fuselage_editor;
use super::session::ExitChoice;

/// Render every open Discipline Window.
pub fn show_discipline_windows(state: &mut AppState, ctx: &Context) {
    let open_ids = state.sandbox.layout.open_disciplines.clone();
    let fields = state.sandbox.fields.clone();
    for id in open_ids {
        let Some(discipline) = Discipline::ALL.into_iter().find(|d| d.id() == id) else {
            state.sandbox.layout.open_disciplines.retain(|d| *d != id);
            continue;
        };
        let mut open = true;
        let groups = grouped(&fields, discipline);
        let response = Window::new(tr(discipline.title()))
            .id(Id::new(("sandbox_discipline_window", discipline.id())))
            .open(&mut open)
            .default_width(380.0)
            .default_height(460.0)
            .resizable(true)
            .show(ctx, |ui| {
                let focused = state.sandbox.focus() == Some(discipline);
                ui.horizontal(|ui| {
                    if ui
                        .add(crate::theme::selectable_button(tr("Focus"), focused))
                        .clicked()
                    {
                        state
                            .sandbox
                            .set_focus(if focused { None } else { Some(discipline) });
                        state.reproject_sandbox_scene();
                    }
                    ui.label(
                        RichText::new(tr("Edits apply on commit and update the preview."))
                            .weak()
                            .small(),
                    );
                });
                ScrollArea::vertical()
                    .id_salt(("sandbox_discipline_scroll", discipline.id()))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if discipline == Discipline::Fuselage {
                            crate::theme::card_frame(ui).show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                show_fuselage_editor(state, ui);
                            });
                            ui.add_space(4.0);
                        }
                        for (title, members) in &groups {
                            let members: Vec<_> = members.iter().map(|f| (*f).clone()).collect();
                            show_group(
                                state,
                                ui,
                                title,
                                &members,
                                &format!("sandbox_window::{}::{title}", discipline.id()),
                                true,
                            );
                        }
                    });
            });
        if let Some(response) = response {
            let pressed = ctx.input(|i| i.pointer.any_pressed());
            if pressed
                && response.response.contains_pointer()
                && state.sandbox.focus() != Some(discipline)
            {
                state.sandbox.set_focus(Some(discipline));
                state.reproject_sandbox_scene();
            }
        }
        if !open {
            state.sandbox.layout.open_disciplines.retain(|d| *d != id);
        }
    }
}

/// Render the leave-sandbox prompt.
pub fn show_exit_prompt(state: &mut AppState, ctx: &Context) {
    if !state.sandbox.exit_prompt {
        return;
    }
    let mut choice = None;
    Window::new(tr("Leave sandbox"))
        .id(Id::new("sandbox_exit_prompt"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.set_min_width(360.0);
            ui.label(tr(
                "Keep the sandbox aircraft as the guided workspace's custom baseline, discard it and return to the previous case, or stay in the sandbox.",
            ));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .button(RichText::new(tr("Promote to guided workspace")).strong())
                    .on_hover_text(tr("Returns to the guided pages with this aircraft as a custom baseline; results that do not match it are dropped."))
                    .clicked()
                {
                    choice = Some(ExitChoice::Promote);
                }
                if ui
                    .button(tr("Discard sandbox"))
                    .on_hover_text(tr("Restores the previous case and its results."))
                    .clicked()
                {
                    choice = Some(ExitChoice::Discard);
                }
                if ui.button(tr("Cancel")).clicked() {
                    choice = Some(ExitChoice::Cancel);
                }
            });
        });
    if let Some(choice) = choice {
        state.resolve_leave_sandbox(choice);
    }
}

/// Render the floating run log, opening it when a sandbox run starts.
pub fn show_log_window(state: &mut AppState, ctx: &Context) {
    if state.is_running && state.sandbox.log_auto_open_for_run != Some(state.run_identity) {
        state.sandbox.log_auto_open_for_run = Some(state.run_identity);
        state.sandbox.layout.log_window_open = true;
    }
    if !state.sandbox.layout.log_window_open {
        return;
    }
    let mut open = true;
    Window::new(tr("Run Log"))
        .id(Id::new("sandbox_run_log_window"))
        .open(&mut open)
        .collapsible(true)
        .resizable(true)
        .default_size(egui::vec2(640.0, 260.0))
        .show(ctx, |ui| {
            show_run_log(state, ui);
        });
    state.sandbox.layout.log_window_open = open;
}

/// Render the Full Analysis results window.
pub fn show_results_window(state: &mut AppState, ctx: &Context) {
    if !state.sandbox.results_window_open {
        return;
    }
    let mut open = true;
    Window::new(tr("Full Analysis results"))
        .id(Id::new("sandbox_results_window"))
        .open(&mut open)
        .resizable(true)
        .default_size(egui::vec2(900.0, 620.0))
        .show(ctx, |ui| {
            if state.sandbox.full_analysis_revision != Some(state.sandbox.revision) {
                ui.label(
                    RichText::new(tr(
                        "These results belong to an earlier revision of the sandbox aircraft.",
                    ))
                    .color(ui.visuals().warn_fg_color)
                    .small(),
                );
            }
            ScrollArea::vertical()
                .id_salt("sandbox_results_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| show_results_view(state, ui));
        });
    state.sandbox.results_window_open = open;
}
