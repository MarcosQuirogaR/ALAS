// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native detached custom-airport editor.
//!
//! The editor is reached from the Route card's departure/arrival selectors:
//! picking [`CUSTOM_AIRPORT_OPTION`] opens this window for that field instead
//! of writing a value. Keeping the fields, import/export, and registry in a
//! separate window is what the Inputs page asked for: a route selector is a
//! one-line choice, while entering an aerodrome is a form.
//!
//! The editor accepts physical runway lengths only.  Field-performance code
//! uses the longest entered physical runway as its conservative available
//! distance when no externally declared operational distances are present.

use alas_config::airport_io::AirportProvenanceKind;
use egui::{vec2, Context, RichText, Ui, ViewportBuilder};

use crate::native_viewport::show_compact_native_viewport;
use crate::state::{AppState, LogKind};
use crate::views::{tr, tr_fields};

/// The route-selector entry that opens this editor.
///
/// It is a command, not an aerodrome: selecting it never reaches
/// `config_values`, so no route can be left pointing at a non-airport.
pub(crate) const CUSTOM_AIRPORT_OPTION: &str = "Custom airport...";

/// Which route field, if any, the editor was opened from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CustomAirportWindow {
    /// Whether the detached editor is shown.
    pub open: bool,
    /// The `config_values` key of the selector that opened it, with the label
    /// to acknowledge the change under. `None` when the editor was opened
    /// without a target, in which case saving only registers the airport.
    pub target: Option<(String, String)>,
}

/// Open the editor for the route selector identified by `key`.
pub(crate) fn open_for(state: &mut AppState, key: &str, label: &str) {
    state.custom_airport_window.open = true;
    state.custom_airport_window.target = Some((key.to_owned(), label.to_owned()));
    state.custom_airport_status = None;
}

/// Render the detached custom-airport editor when it is open.
pub(crate) fn show_custom_airport_window(state: &mut AppState, ctx: &Context) {
    if !state.custom_airport_window.open {
        return;
    }

    let response = show_compact_native_viewport(
        ctx,
        "custom_airport",
        tr("Custom airport"),
        ViewportBuilder::default()
            .with_title(tr("Custom airport"))
            .with_inner_size(vec2(780.0, 520.0))
            .with_min_inner_size(vec2(520.0, 420.0))
            .with_resizable(true),
        |_child_ctx, ui, _class| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| show_custom_airport_editor(state, ui));
        },
    );

    if response.close_requested {
        state.custom_airport_window.open = false;
        state.custom_airport_window.target = None;
    }
}

/// The editor body: entry fields, save actions, file import/export, and the
/// registry with its provenance and missing-declaration status.
fn show_custom_airport_editor(state: &mut AppState, ui: &mut Ui) {
    ui.heading(tr("Custom airport"));
    ui.add_space(6.0);
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        if ui.available_width() >= 600.0 {
            ui.columns(2, |columns| {
                airport_draft_text(
                    &mut columns[0],
                    "ICAO",
                    &mut state.custom_airport_draft.icao,
                );
                airport_draft_text(
                    &mut columns[0],
                    "Latitude deg",
                    &mut state.custom_airport_draft.latitude_deg,
                );
                airport_draft_text(
                    &mut columns[0],
                    "ISA delta C",
                    &mut state.custom_airport_draft.isa_delta_c,
                );
                airport_draft_text(
                    &mut columns[0],
                    "Runway lengths m",
                    &mut state.custom_airport_draft.runway_lengths_m,
                );

                airport_draft_text(
                    &mut columns[1],
                    "Name",
                    &mut state.custom_airport_draft.name,
                );
                airport_draft_text(
                    &mut columns[1],
                    "Longitude deg",
                    &mut state.custom_airport_draft.longitude_deg,
                );
                airport_draft_text(
                    &mut columns[1],
                    "Altitude m",
                    &mut state.custom_airport_draft.altitude_m,
                );
            });
        } else {
            airport_draft_text(ui, "ICAO", &mut state.custom_airport_draft.icao);
            airport_draft_text(ui, "Name", &mut state.custom_airport_draft.name);
            airport_draft_text(
                ui,
                "Latitude deg",
                &mut state.custom_airport_draft.latitude_deg,
            );
            airport_draft_text(
                ui,
                "Longitude deg",
                &mut state.custom_airport_draft.longitude_deg,
            );
            airport_draft_text(
                ui,
                "ISA delta C",
                &mut state.custom_airport_draft.isa_delta_c,
            );
            airport_draft_text(ui, "Altitude m", &mut state.custom_airport_draft.altitude_m);
            airport_draft_text(
                ui,
                "Runway lengths m",
                &mut state.custom_airport_draft.runway_lengths_m,
            );
        }
    });

    ui.add_space(8.0);
    ui.horizontal_wrapped(|ui| {
        if let Some((key, label)) = state.custom_airport_window.target.clone() {
            if ui
                .button(tr_fields(
                    "Save and use for {field}",
                    &[("field", label.clone())],
                ))
                .clicked()
            {
                save_and_assign(state, &key, &label);
            }
        }
        if ui
            .button(tr("Save custom airport"))
            .on_hover_text(tr("Registers the airport without changing the route."))
            .clicked()
        {
            state.save_custom_airport();
        }
    });
    ui.add_space(8.0);
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(tr("Import / export path")).strong());
        ui.add_sized(
            [ui.available_width(), ui.spacing().interact_size.y],
            egui::TextEdit::singleline(&mut state.custom_airport_file_path),
        );
        ui.horizontal_wrapped(|ui| {
            if ui.button(tr("Import airports")).clicked() {
                state.import_custom_airports();
            }
            if ui.button(tr("Export airports")).clicked() {
                state.export_custom_airports();
            }
        });
    });
    if let Some(status) = &state.custom_airport_status {
        ui.label(RichText::new(status).weak().small());
    }

    let custom = alas_config::airport_io::registered_custom_airports();
    if !custom.is_empty() {
        ui.separator();
        ui.label(RichText::new(tr("Registered custom airports")).strong());
        for airport in custom {
            let runway_length = airport
                .runway_lengths_m
                .iter()
                .copied()
                .filter(|length| length.is_finite() && *length > 0.0)
                .fold(0.0, f64::max);
            ui.label(format!(
                "{} - {} ({runway_length:.0} m) [{}]",
                airport.icao,
                airport.name,
                tr(airport_provenance_label(airport.provenance.kind))
            ));
        }
    }
}

/// Register the drafted airport and, only if the registry accepted it, select
/// it in the route field the editor was opened from.
///
/// A rejected draft leaves the route untouched: the selector keeps whatever
/// resolvable airport it had rather than pointing at an unregistered name.
fn save_and_assign(state: &mut AppState, key: &str, label: &str) {
    let Some(name) = state.save_custom_airport() else {
        return;
    };
    if let Some(object) = state.config_values.as_object_mut() {
        object.insert(key.to_owned(), serde_json::Value::String(name.clone()));
    }
    state.on_config_modified();
    state.note_parameter_modified(tr(label), name.clone());
    state.log(
        tr_fields(
            "Custom airport {airport} selected for {field}.",
            &[("airport", name), ("field", tr(label))],
        ),
        LogKind::Info,
    );
    state.custom_airport_window.open = false;
    state.custom_airport_window.target = None;
}

fn airport_provenance_label(kind: AirportProvenanceKind) -> &'static str {
    match kind {
        AirportProvenanceKind::UserEntered => "user entered",
        AirportProvenanceKind::DatImport => "DAT import",
        AirportProvenanceKind::JsonImport => "JSON import",
        AirportProvenanceKind::Workspace => "workspace",
    }
}

fn airport_draft_text(ui: &mut Ui, label: &str, value: &mut String) {
    ui.label(tr(label));
    ui.add_sized(
        [ui.available_width(), ui.spacing().interact_size.y],
        egui::TextEdit::singleline(value),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_editor(size: egui::Vec2) -> egui::FullOutput {
        let context = Context::default();
        crate::theme::apply_theme(crate::theme::AppTheme::Dark, &context);
        let mut state = AppState::default();
        open_for(&mut state, "departure_airport", "Departure airport");
        drafted(&mut state, "LEMD", "Madrid Custom");
        let mut output = None;
        for _ in 0..2 {
            output = Some(context.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..egui::RawInput::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        show_custom_airport_editor(&mut state, ui);
                    });
                },
            ));
        }
        output.expect("two settled editor frames")
    }

    fn horizontally_clipped_text(output: &egui::FullOutput) -> Vec<String> {
        fn walk(shape: &egui::Shape, clip: egui::Rect, found: &mut Vec<String>) {
            match shape {
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        walk(shape, clip, found);
                    }
                }
                egui::Shape::Text(text) => {
                    let rect = text.galley.rect.translate(text.pos.to_vec2());
                    if rect.left() < clip.left() - 0.5 || rect.right() > clip.right() + 0.5 {
                        let text = text.galley.text().trim();
                        if !text.is_empty() {
                            found.push(text.to_owned());
                        }
                    }
                }
                _ => {}
            }
        }

        let mut found = Vec::new();
        for clipped in &output.shapes {
            walk(&clipped.shape, clipped.clip_rect, &mut found);
        }
        found
    }

    fn widest_painted_edge(output: &egui::FullOutput) -> f32 {
        fn walk(shape: &egui::Shape, widest: &mut f32) {
            match shape {
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        walk(shape, widest);
                    }
                }
                egui::Shape::Rect(rect) if rect.rect.width() > 10.0 => {
                    *widest = widest.max(rect.rect.right());
                }
                _ => {}
            }
        }

        let mut widest = 0.0_f32;
        for clipped in &output.shapes {
            walk(&clipped.shape, &mut widest);
        }
        widest
    }

    fn drafted(state: &mut AppState, icao: &str, name: &str) {
        state.custom_airport_draft = crate::airport_editor::CustomAirportDraft {
            icao: icao.to_owned(),
            name: name.to_owned(),
            latitude_deg: "40.47".to_owned(),
            longitude_deg: "-3.56".to_owned(),
            isa_delta_c: "0.0".to_owned(),
            altitude_m: "610.0".to_owned(),
            runway_lengths_m: "3500, 4100".to_owned(),
        };
    }

    #[test]
    fn the_selector_command_is_not_an_aerodrome_name() {
        // The sentinel must not collide with a database display name, or a
        // real airport could become unselectable.
        assert!(!alas_config::airports::database_with_custom()
            .iter()
            .any(|airport| airport.name == CUSTOM_AIRPORT_OPTION));
    }

    #[test]
    fn opening_from_a_selector_records_the_target_field() {
        let mut state = AppState::default();
        open_for(&mut state, "departure_airport", "Departure airport");

        assert!(state.custom_airport_window.open);
        assert_eq!(
            state.custom_airport_window.target,
            Some((
                "departure_airport".to_owned(),
                "Departure airport".to_owned()
            ))
        );
    }

    #[test]
    fn a_rejected_draft_never_reaches_the_route() {
        let mut state = AppState::default();
        let before = state.config_values.clone();
        open_for(&mut state, "departure_airport", "Departure airport");
        // No coordinates, no elevation: the registry rejects it.
        state.custom_airport_draft = crate::airport_editor::CustomAirportDraft::default();

        save_and_assign(&mut state, "departure_airport", "Departure airport");

        assert_eq!(state.config_values, before);
        assert!(state.custom_airport_window.open);
        assert!(state.custom_airport_status.is_some());
    }

    #[test]
    fn custom_airport_draft_keeps_physical_lengths_without_regulatory_fields() {
        let mut state = AppState::default();
        drafted(&mut state, "LEMD", "Madrid Custom");

        let airport = state
            .custom_airport_draft
            .to_airport()
            .expect("a complete physical record is valid");
        assert_eq!(airport.declared_toda_m, None);
        assert_eq!(airport.declared_lda_m, None);
        assert_eq!(airport.runway_lengths_m, vec![3500.0, 4100.0]);
    }

    #[test]
    fn editor_reflows_without_horizontal_overflow() {
        for size in [vec2(520.0, 900.0), vec2(754.0, 594.0), vec2(780.0, 520.0)] {
            let output = render_editor(size);
            let clipped = horizontally_clipped_text(&output);
            assert!(
                clipped.is_empty(),
                "custom-airport text is clipped at {size:?}: {clipped:?}"
            );
            assert!(
                widest_painted_edge(&output) <= size.x + 0.5,
                "custom-airport controls overflow a {size:?} viewport"
            );
        }
    }
}
