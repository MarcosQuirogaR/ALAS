// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The airfoil outline preview beside a selected screening candidate.

use egui::{vec2, RichText, ScrollArea, Ui};

use crate::views::tr;

pub(super) fn show_screening_preview(
    screening: &mut crate::screening::ScreeningState,
    ui: &mut Ui,
) {
    screening
        .preview
        .update_filter(&screening.options.name_filter);
    if screening.preview.selected().is_none() {
        if let Some(name) = screening.preview.filtered_names().first().cloned() {
            screening.preview.select(&name);
        }
    }
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.label(RichText::new(tr("Airfoil outline")).strong());
        egui::ComboBox::from_id_salt("screening_preview_airfoil")
            .width(ui.available_width().min(320.0))
            .selected_text(screening.preview.selected().unwrap_or("-"))
            .show_ui(ui, |ui| {
                let row_height = ui
                    .text_style_height(&egui::TextStyle::Button)
                    .max(ui.spacing().interact_size.y);
                let mut selected = None;
                let library_rows = screening.preview.filtered_names().len();
                // Keep the importer and the virtualized library in one
                // scroll model.  The previous layout put the importer after
                // a nested library scroll area, which made the popup expose
                // two independent vertical tracks and hid the action at the
                // bottom of a long library.
                let import_rows = 1;
                ScrollArea::vertical()
                    .id_salt("screening_preview_airfoil_scroll")
                    .max_height(240.0)
                    .show_rows(ui, row_height, library_rows + import_rows, |ui, range| {
                        for row in range {
                            if row == 0 {
                                if ui
                                    .selectable_label(false, tr("Import custom airfoil .dat..."))
                                    .clicked()
                                {
                                    screening.custom_airfoil_import_open = true;
                                }
                                continue;
                            }
                            let index = row - import_rows;
                            let name = &screening.preview.filtered_names()[index];
                            if ui
                                .selectable_label(
                                    screening.preview.selected() == Some(name.as_str()),
                                    name,
                                )
                                .clicked()
                            {
                                selected = Some(name.clone());
                            }
                        }
                    });
                if let Some(name) = selected {
                    screening.preview.select(&name);
                }
            });
        if screening.preview.filtered_names().is_empty() {
            ui.label(tr("No library sections match the name filter."));
        }
        let Some(points) = screening.preview.coordinates() else {
            ui.colored_label(
                ui.visuals().error_fg_color,
                tr("Airfoil coordinates unavailable."),
            );
            return;
        };
        let width = ui.available_width().max(1.0);
        let (rect, _) = ui.allocate_exact_size(
            vec2(width, (width * 0.28).clamp(100.0, 220.0)),
            egui::Sense::hover(),
        );
        let points = screening_outline_points(points, rect.shrink(12.0));
        ui.painter().add(egui::Shape::line(
            points,
            egui::Stroke::new(2.0_f32, ui.visuals().text_color()),
        ));
    });
}

/// Preserve the supplied physical aspect ratio; +y/c is up on screen.
pub(super) fn screening_outline_points(points: &[(f64, f64)], rect: egui::Rect) -> Vec<egui::Pos2> {
    let (mut xmin, mut xmax, mut ymin, mut ymax) = (
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    );
    for &(x, y) in points {
        xmin = xmin.min(x);
        xmax = xmax.max(x);
        ymin = ymin.min(y);
        ymax = ymax.max(y);
    }
    let scale = (rect.width().max(0.0) as f64 / (xmax - xmin).max(1e-9))
        .min(rect.height().max(0.0) as f64 / (ymax - ymin).max(1e-9));
    points
        .iter()
        .map(|&(x, y)| {
            egui::pos2(
                rect.center().x + ((x - (xmin + xmax) * 0.5) * scale) as f32,
                rect.center().y - ((y - (ymin + ymax) * 0.5) * scale) as f32,
            )
        })
        .collect()
}
