// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The sandbox's detached windows: Discipline Windows, the run log, the Full
//! Analysis results, and the leave-sandbox prompt.
//!
//! A Discipline Window shows the same grouped editors as the Parameter
//! Panel; the fuselage window adds the section editor. Pressing the pointer
//! inside a window links the preview to that discipline, so several open
//! editors follow whichever one is being used. The run log starts hidden,
//! opens when a sandbox analysis starts, and can be minimized (collapsed),
//! closed and reopened without touching the run.

use egui::{vec2, Context, Id, RichText, ScrollArea, Vec2, ViewportBuilder};

use crate::native_viewport::{raise, show_native_viewport, show_native_viewport_anchored};
use crate::shell_confirm::{confirm_dialog, ConfirmAction, ConfirmAnswer};
use crate::state::AppState;
use crate::views::{show_results_view, show_run_log, tr};

use super::editors::show_group;
use super::fields::{grouped, Discipline};
use super::section_views::show_section_editor;
use super::session::ExitChoice;

/// Where a newly opened Discipline Window defaults to, in the fraction
/// [`crate::native_viewport::show_native_viewport_anchored`] takes: clear of
/// the design space's own centre, where the preview the window's own editors
/// say they "update" is drawn, and clear of the category stack on the left,
/// the camera row at the top and the action block at the bottom (see
/// `sandbox::viewport`'s module docs), so the editor and the component it
/// isolates are both on screen together at the moment it opens.
const DISCIPLINE_WINDOW_ANCHOR: Vec2 = Vec2::new(0.85, 0.08);

/// Render every open Discipline Window.
pub fn show_discipline_windows(state: &mut AppState, ctx: &Context) {
    let open_ids = state.sandbox.layout.open_disciplines.clone();
    for discipline in Discipline::ALL {
        if !open_ids.iter().any(|id| id.as_str() == discipline.id()) {
            ctx.data_mut(|d| {
                d.remove::<bool>(Id::new(("sandbox_window_seen", discipline.id().to_owned())))
            });
        }
    }
    let fields = state.sandbox.fields.clone();
    for id in open_ids {
        let Some(discipline) = Discipline::ALL.into_iter().find(|d| d.id() == id) else {
            state.sandbox.layout.open_disciplines.retain(|d| *d != id);
            continue;
        };
        let groups = grouped(&fields, discipline);
        // A window that was not open last frame starts at its top; a search jump
        // to a field scrolls to it afterwards, in the same frame.
        let opened_now = !ctx.data(|d| {
            d.get_temp::<bool>(Id::new(("sandbox_window_seen", id.clone())))
                .unwrap_or(false)
        });
        ctx.data_mut(|d| d.insert_temp(Id::new(("sandbox_window_seen", id.clone())), true));
        // Re-triggering an open window (a category button, a palette result)
        // brings it back from minimised or from behind the main window.
        if state.sandbox.raise_discipline.as_deref() == Some(discipline.id()) {
            state.sandbox.raise_discipline = None;
            raise(ctx, ("sandbox_discipline", discipline.id()));
        }
        let response = show_native_viewport_anchored(
            ctx,
            ("sandbox_discipline", discipline.id()),
            tr(discipline.title()),
            ViewportBuilder::default()
                .with_title(tr(discipline.title()))
                .with_inner_size(default_window_size(discipline))
                .with_min_inner_size(vec2(300.0, 260.0))
                .with_resizable(true),
            DISCIPLINE_WINDOW_ANCHOR,
            |_child_ctx, ui, _class| {
                let focused = state.sandbox.focus() == Some(discipline);
                ui.horizontal(|ui| {
                    if ui
                        .add(crate::theme::selectable_button(tr("Focus"), focused))
                        .on_hover_text(tr("Edits apply on commit and update the preview."))
                        .clicked()
                    {
                        state.set_sandbox_focus(if focused { None } else { Some(discipline) });
                    }
                });
                let mut scroll = ScrollArea::vertical()
                    .id_salt(("sandbox_discipline_scroll", discipline.id()))
                    .auto_shrink([false, false]);
                if opened_now {
                    scroll = scroll.vertical_scroll_offset(0.0);
                }
                scroll.show(ui, |ui| {
                    crate::theme::card_frame(ui).show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        show_section_editor(state, ui, discipline);
                    });
                    ui.add_space(4.0);
                    if discipline == Discipline::Wing {
                        crate::theme::card_frame(ui).show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.label(RichText::new(tr("Create wing sections")).strong());
                            crate::views::inputs_custom::show_custom_wing_sections(state, ui);
                        });
                        ui.add_space(4.0);
                    }
                    if discipline == Discipline::Fuselage {
                        crate::theme::card_frame(ui).show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.label(RichText::new(tr("Create fuselage sections")).strong());
                            crate::views::inputs_custom::show_custom_fuselage_sections(state, ui);
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
                    crate::search::flush_scroll(ui);
                });
            },
        );
        if response.pointer_pressed && state.sandbox.focus() != Some(discipline) {
            state.set_sandbox_focus(Some(discipline));
        }
        if response.close_requested {
            state.sandbox.layout.open_disciplines.retain(|d| *d != id);
        }
    }
}

/// Default inner size of a Discipline Window, points. The Wing window carries
/// the station table (eight columns), so it opens wide enough to show the
/// whole table, Airfoil included, without a sideways scroll.
fn default_window_size(discipline: Discipline) -> egui::Vec2 {
    if discipline == Discipline::Wing {
        vec2(820.0, 520.0)
    } else {
        vec2(440.0, 600.0)
    }
}

/// Render the one leave-sandbox dialog: promote, discard or cancel. Promoting
/// replaces the guided case the Sandbox holds, so the consequence is stated
/// here instead of in a second prompt.
pub fn show_exit_prompt(state: &mut AppState, ctx: &Context) {
    if !state.sandbox.exit_prompt {
        return;
    }
    let mut body = tr("Promote makes this aircraft the guided design; discard drops it and restores the previous design.");
    if state.is_running {
        body.push('\n');
        body.push_str(&tr(
            "A sandbox analysis is still running; it is being stopped.",
        ));
    } else if state.sandbox.dirty() || state.shell.undo.is_dirty() {
        body.push('\n');
        body.push_str(&tr("Discarding loses the unsaved edits."));
    }
    let actions = [
        ConfirmAction::plain(tr("Promote to guided design")),
        ConfirmAction::destructive(tr("Discard sandbox")),
    ];
    let choice = match confirm_dialog(
        ctx,
        "sandbox_exit_prompt",
        &tr("Exit Sandbox mode"),
        Some(&body),
        &actions,
    ) {
        Some(ConfirmAnswer::Action(0)) => ExitChoice::Promote,
        Some(ConfirmAnswer::Action(_)) => ExitChoice::Discard,
        Some(ConfirmAnswer::Cancel) => ExitChoice::Cancel,
        None => return,
    };
    state.resolve_leave_sandbox(choice);
}

/// Ask before switching a lofted fuselage to parametric so the hump can apply.
pub fn show_hump_confirm(state: &mut AppState, ctx: &Context) {
    if !state.sandbox.hump_confirm {
        return;
    }
    let body = tr("This fuselage is lofted from an embedded surface, which ignores the hump. Switching clears that loft and uses the parametric fuselage with the cabin, nose and tail settings; the change can be undone.");
    let actions = [ConfirmAction::primary(tr("Switch to parametric"))];
    match confirm_dialog(
        ctx,
        "sandbox_hump_confirm",
        &tr("Switch the fuselage to parametric so the hump can be applied?"),
        Some(&body),
        &actions,
    ) {
        Some(ConfirmAnswer::Action(_)) => {
            state.sandbox.hump_confirm = false;
            super::hump_defaults::enable(state, true);
        }
        Some(ConfirmAnswer::Cancel) => state.sandbox.hump_confirm = false,
        None => {}
    }
}

/// Render the detached native run log, opening it when a sandbox run starts.
pub fn show_log_window(state: &mut AppState, ctx: &Context) {
    if state.is_running && state.sandbox.log_auto_open_for_run != Some(state.run_identity) {
        state.sandbox.log_auto_open_for_run = Some(state.run_identity);
        state.sandbox.layout.log_window_open = true;
    }
    if !state.sandbox.layout.log_window_open {
        return;
    }
    let response = show_native_viewport(
        ctx,
        "sandbox_run_log",
        tr("Run Log"),
        ViewportBuilder::default()
            .with_title(tr("Run Log"))
            .with_inner_size(vec2(640.0, 260.0))
            .with_min_inner_size(vec2(420.0, 180.0))
            .with_resizable(true),
        |_child_ctx, ui, _class| {
            show_run_log(state, ui);
        },
    );
    if response.close_requested {
        state.sandbox.layout.log_window_open = false;
    }
}

/// Render the Full Analysis results window.
pub fn show_results_window(state: &mut AppState, ctx: &Context) {
    if !state.sandbox.results_window_open {
        return;
    }
    let response = show_native_viewport(
        ctx,
        "sandbox_results",
        tr("Full Analysis results"),
        ViewportBuilder::default()
            .with_title(tr("Full Analysis results"))
            .with_inner_size(vec2(900.0, 620.0))
            .with_min_inner_size(vec2(620.0, 420.0))
            .with_resizable(true),
        |_child_ctx, ui, _class| {
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
        },
    );
    if response.close_requested {
        state.sandbox.results_window_open = false;
    }
}
