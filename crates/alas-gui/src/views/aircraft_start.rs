// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The first choice of the Aircraft page: three equal pipeline cards,
//! Reference Aircraft, Clean Sheet and Sandbox. Reference Aircraft adapts a
//! registered aircraft; Clean Sheet starts from requirements alone; the
//! Sandbox card enters the Sandbox workspace (a separate tool, so it does not
//! change this page's mode). Opening a saved aircraft or any ALAS file is an
//! action of the Reference Aircraft card.

use egui::{ComboBox, DragValue, RichText, Sense, Ui};

use crate::state::AppState;
use crate::views::tr_label;

/// Which starting point the Aircraft page offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StartMode {
    /// Requirements only.
    CleanSheet,
    /// A registered aircraft.
    #[default]
    Reference,
}

/// Below this width the three cards stack instead of sharing a row.
const PIPELINE_ROW_MIN_WIDTH: f32 = 520.0;

/// Height every pipeline card keeps, so the three stay the same size.
const PIPELINE_CARD_HEIGHT: f32 = 58.0;

/// One pipeline card: a title and a one-line description. Returns whether it
/// was clicked.
fn pipeline_card(ui: &mut Ui, title: &str, description: &str, selected: bool) -> egui::Response {
    let stroke = if selected {
        egui::Stroke::new(1.5_f32, ui.visuals().hyperlink_color)
    } else {
        ui.visuals().widgets.noninteractive.bg_stroke
    };
    let shown = crate::theme::card_frame(ui).stroke(stroke).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.set_min_height(PIPELINE_CARD_HEIGHT);
        // Wrapped text can measure a fraction of a point wider than the width
        // it wrapped to; one point of slack keeps the card, and so the row
        // of three, inside the column instead of widening the whole page.
        // `ui.columns` lays its columns out top-down *justified*, and egui
        // justifies the words of a wrapped label in such a layout: a
        // description that wraps would be spread across the card with wide
        // gaps. Left-aligned, not justified.
        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_max_width((ui.available_width() - 1.0).max(1.0));
            ui.label(RichText::new(tr_label(title)).strong().size(15.0));
            ui.label(RichText::new(tr_label(description)).weak().small());
        });
    });
    ui.interact(
        shown.response.rect,
        ui.id().with(("pipeline_card", title)),
        Sense::click(),
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Render the choice and the card of the chosen starting point.
pub fn show_start(state: &mut AppState, ui: &mut Ui) {
    let mode = state.shell.start_mode;
    let mut chosen = None;
    let mut sandbox = false;
    let mut sandbox_rect = None;
    let mut cards = |ui: &mut Ui, index: usize| match index {
        0 => {
            if pipeline_card(
                ui,
                "Reference Aircraft",
                "Adapt a registered aircraft",
                mode == StartMode::Reference,
            )
            .clicked()
            {
                chosen = Some(StartMode::Reference);
            }
        }
        1 => {
            if pipeline_card(
                ui,
                "Clean Sheet",
                "Size a new aircraft from requirements",
                mode == StartMode::CleanSheet,
            )
            .clicked()
            {
                chosen = Some(StartMode::CleanSheet);
            }
        }
        _ => {
            let card = pipeline_card(ui, "Sandbox", "Shape an aircraft by hand, from AVE", false);
            sandbox_rect = Some(card.rect);
            if card.clicked() {
                sandbox = true;
            }
        }
    };
    if ui.available_width() >= PIPELINE_ROW_MIN_WIDTH {
        ui.columns(3, |columns| {
            for (index, column) in columns.iter_mut().enumerate() {
                cards(column, index);
            }
        });
    } else {
        for index in 0..3 {
            cards(ui, index);
            ui.add_space(4.0);
        }
    }
    if let Some(rect) = sandbox_rect {
        state.record_walkthrough_target(crate::views::tour_data::TourTarget::SandboxEntry, rect);
    }
    if let Some(mode) = chosen {
        state.shell.start_mode = mode;
    }
    if sandbox {
        state.enter_sandbox(false);
    }
    ui.add_space(8.0);
    match state.shell.start_mode {
        StartMode::CleanSheet => show_clean_sheet(state, ui),
        StartMode::Reference => crate::views::inputs_view::show_reference_card(state, ui),
    }
}

/// The configuration the clean-sheet derivation fixes, as one line.
fn locked_layout_row(ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        crate::theme::lock_icon(ui, &tr_label("Class Configuration"));
        ui.label(
            RichText::new(tr_label(
                "Low wing, conventional tail, 2 wing-mounted engines",
            ))
            .weak()
            .small(),
        );
    });
}

fn show_clean_sheet(state: &mut AppState, ui: &mut Ui) {
    let airports = state.airport_names.clone();
    let engines = state.engine_names.clone();
    let mut brief = state.shell.clean_sheet_draft.clone();
    if brief.departure.is_empty() {
        brief.departure = state
            .config_values
            .get("departure_airport")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_owned();
    }
    if brief.arrival.is_empty() {
        brief.arrival = state
            .config_values
            .get("arrival_airport")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_owned();
    }
    if brief.engine_name.is_empty() {
        brief.engine_name = state
            .config_values
            .pointer("/geometry/engine/engine_name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_owned();
    }
    let mut create = false;
    crate::views::inputs_view::card(ui, "Aircraft Configuration", |ui| {
        // One grid whose column count comes from the width: label above a
        // full-width value box in every cell, like the schema forms.
        use crate::views::form::{drag_box, label_row, CellGrid};
        let label_of = |label: &str, unit: &str| {
            if unit.is_empty() {
                tr_label(label)
            } else {
                format!("{} ({unit})", tr_label(label))
            }
        };
        let mut grid = CellGrid::new(ui, "clean_sheet_grid");
        grid.cell(ui, |ui, width| {
            label_row(ui, label_of("Aircraft Type", ""), |_| {});
            ComboBox::from_id_salt("clean_sheet_type")
                .icon(crate::combo_icon::down_arrow)
                .width(width)
                .selected_text(tr_label(if brief.aircraft_type == "cargo" {
                    "Cargo"
                } else {
                    "Passenger"
                }))
                .show_ui(ui, |ui| {
                    for (id, label) in [("passenger", "Passenger"), ("cargo", "Cargo")] {
                        if ui
                            .selectable_label(brief.aircraft_type == id, tr_label(label))
                            .clicked()
                        {
                            brief.aircraft_type = id.to_owned();
                        }
                    }
                });
        });
        if brief.aircraft_type == "cargo" {
            grid.cell(ui, |ui, width| {
                label_row(ui, label_of("Cargo Payload", "kg"), |_| {});
                drag_box(
                    ui,
                    width,
                    DragValue::new(&mut brief.cargo_payload_kg)
                        .speed(100.0)
                        .range(100.0..=500_000.0),
                );
            });
        } else {
            grid.cell(ui, |ui, width| {
                label_row(ui, label_of("Passengers", "pax"), |_| {});
                drag_box(
                    ui,
                    width,
                    DragValue::new(&mut brief.passengers).range(1..=900),
                );
            });
        }
        grid.cell(ui, |ui, width| {
            label_row(ui, label_of("Design Range", "nmi"), |_| {});
            drag_box(
                ui,
                width,
                DragValue::new(&mut brief.design_range_nmi)
                    .speed(25.0)
                    .range(50.0..=9_000.0),
            );
        });
        grid.cell(ui, |ui, width| {
            label_row(ui, label_of("Cruise Mach", ""), |_| {});
            drag_box(
                ui,
                width,
                DragValue::new(&mut brief.cruise_mach)
                    .speed(0.005)
                    .range(0.3..=0.95)
                    .fixed_decimals(2),
            );
        });
        grid.cell(ui, |ui, width| {
            label_row(ui, label_of("Cruise Altitude", "m"), |_| {});
            drag_box(
                ui,
                width,
                DragValue::new(&mut brief.cruise_altitude_m)
                    .speed(50.0)
                    .range(1_000.0..=15_000.0),
            );
        });
        grid.cell(ui, |ui, width| {
            label_row(ui, label_of("Take-off Mass Estimate", "kg"), |_| {});
            drag_box(
                ui,
                width,
                DragValue::new(&mut brief.mtow_estimate_kg)
                    .speed(500.0)
                    .range(1_000.0..=700_000.0),
            );
        });
        grid.cell(ui, |ui, width| {
            label_row(ui, label_of("Engine", ""), |_| {});
            if let Some(name) = crate::selector_search::named_list_combo(
                ui,
                "clean_sheet_engine",
                Some(width),
                &brief.engine_name,
                &engines,
                &brief.engine_name,
            ) {
                brief.engine_name = name;
            }
        });
        for (label, id, current) in [
            ("Departure", "clean_sheet_departure", &mut brief.departure),
            ("Arrival", "clean_sheet_arrival", &mut brief.arrival),
        ] {
            grid.cell(ui, |ui, width| {
                label_row(ui, label_of(label, ""), |_| {});
                let shown = current.clone();
                if let Some(name) = crate::selector_search::named_list_combo(
                    ui,
                    id,
                    Some(width),
                    &shown,
                    &airports,
                    &shown,
                ) {
                    *current = name;
                }
            });
        }
        grid.finish(ui);
        locked_layout_row(ui);
        ui.add_space(6.0);
        if ui
            .add(crate::theme::selectable_button(
                RichText::new(tr_label("Create Clean Sheet")).size(15.0),
                true,
            ))
            .clicked()
        {
            create = true;
        }
    });
    state.shell.clean_sheet_draft = brief.clone();
    if create {
        state.guarded(crate::shell_confirm::Pending::CleanSheet(brief));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn painted(mode: StartMode, width: f32) -> Vec<String> {
        fn collect(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, out)),
                egui::Shape::Text(text) => out.push(text.galley.text().to_owned()),
                _ => {}
            }
        }
        let mut state = AppState::default();
        state.shell.start_mode = mode;
        let ctx = egui::Context::default();
        let mut texts = Vec::new();
        for _ in 0..2 {
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 1_400.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| show_start(&mut state, ui));
                },
            );
            texts.clear();
            for clipped in &output.shapes {
                collect(&clipped.shape, &mut texts);
            }
        }
        texts
    }

    #[test]
    fn the_three_starting_points_are_offered_whatever_the_width() {
        for width in [520.0, 900.0, 1_400.0] {
            let texts = painted(StartMode::Reference, width);
            for label in ["Clean Sheet", "Reference Aircraft", "Sandbox"] {
                let shown = tr_label(label);
                assert!(texts.contains(&shown), "{label} at {width}");
            }
        }
    }

    #[test]
    fn the_clean_sheet_card_asks_for_requirements_and_offers_to_create() {
        let texts = painted(StartMode::CleanSheet, 1_000.0);
        for label in [
            "Passengers",
            "Design Range",
            "Cruise Mach",
            "Create Clean Sheet",
        ] {
            // A field label carries its unit: "Passengers (pax)".
            let shown = tr_label(label);
            assert!(
                texts
                    .iter()
                    .any(|text| text == &shown || text.starts_with(&format!("{shown} ("))),
                "{label}: {texts:?}"
            );
        }
    }
}
