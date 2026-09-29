// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The advanced walkthrough guide window.

use egui::{Context, RichText, ScrollArea, Window};

use super::tr;
use crate::state::AppState;
use crate::views::guide_data::CHAPTERS;

/// Reading measure of the Advanced Walkthrough body.
const GUIDE_MEASURE_WIDTH: f32 = 640.0;

/// Render the advanced walkthrough guide window, if it is open.
pub fn show_advanced_guide(state: &mut AppState, ctx: &Context) {
    if !state.show_advanced_guide {
        return;
    }
    let mut open = true;
    Window::new(tr("Advanced Walkthrough"))
        .open(&mut open)
        .default_size(egui::vec2(880.0, 620.0))
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(220.0);
                    ScrollArea::vertical().id_salt("guide_nav").show(ui, |ui| {
                        for (i, chapter) in CHAPTERS.iter().enumerate() {
                            let selected = state.guide_chapter == i;
                            if ui
                                .selectable_label(
                                    selected,
                                    format!("{}. {}", i + 1, tr(chapter.title)),
                                )
                                .on_hover_text(tr(chapter.blurb))
                                .clicked()
                            {
                                state.guide_chapter = i;
                            }
                        }
                    });
                });
                ui.separator();
                ui.vertical(|ui| {
                    let chapter = &CHAPTERS[state.guide_chapter.min(CHAPTERS.len() - 1)];
                    ScrollArea::vertical()
                        .id_salt("guide_content")
                        .show(ui, |ui| {
                            ui.set_max_width(GUIDE_MEASURE_WIDTH);
                            ui.label(RichText::new(tr(chapter.title)).strong().size(22.0))
                                .on_hover_text(tr(chapter.blurb));
                            for section in chapter.sections {
                                ui.add_space(14.0);
                                ui.label(RichText::new(tr(section.heading)).strong().size(16.0))
                                    .on_hover_ui(|ui| {
                                        ui.set_max_width(GUIDE_MEASURE_WIDTH);
                                        for para in section.body {
                                            ui.add(egui::Label::new(tr(para)).wrap());
                                            ui.add_space(6.0);
                                        }
                                    });
                            }
                            ui.add_space(16.0);
                            ui.horizontal(|ui| {
                                if state.guide_chapter > 0 && ui.button(tr("<- Back")).clicked() {
                                    state.guide_chapter -= 1;
                                }
                                ui.label(format!(
                                    "{} / {}",
                                    state.guide_chapter + 1,
                                    CHAPTERS.len()
                                ));
                                if state.guide_chapter + 1 < CHAPTERS.len()
                                    && ui.button(tr("Next ->")).clicked()
                                {
                                    state.guide_chapter += 1;
                                }
                            });
                        });
                });
            });
        });
    state.show_advanced_guide = open;
}
