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

/// How long the "Saved" confirmation stays, in seconds.
const SAVED_NOTICE_SECONDS: f64 = 4.0;

/// The editor body: entry fields with inline errors, the save actions, and
/// the link to the Library, where airports are loaded, renamed, exported and
/// deleted like every other saved item.
fn show_custom_airport_editor(state: &mut AppState, ui: &mut Ui) {
    use crate::airport_editor::DraftField as F;

    ui.heading(tr("Custom airport"));
    ui.add_space(6.0);
    // A form nobody has typed into yet shows no errors; once any field has
    // text, every empty or invalid field is outlined with its message.
    let show_errors = !state.custom_airport_draft.is_pristine();
    let error = |field: F| {
        show_errors
            .then(|| state.custom_airport_draft.field_error(field))
            .flatten()
    };
    let [icao, name, lat, lon, isa, alt, runways] = [
        F::Icao,
        F::Name,
        F::Latitude,
        F::Longitude,
        F::IsaDelta,
        F::Altitude,
        F::Runways,
    ]
    .map(error);
    let valid = state.custom_airport_draft.is_valid();
    let draft = &mut state.custom_airport_draft;
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        if ui.available_width() >= 600.0 {
            ui.columns(2, |columns| {
                airport_draft_text(&mut columns[0], "ICAO", &mut draft.icao, icao.as_deref());
                airport_draft_text(
                    &mut columns[0],
                    "Latitude deg",
                    &mut draft.latitude_deg,
                    lat.as_deref(),
                );
                airport_draft_text(
                    &mut columns[0],
                    "ISA delta C",
                    &mut draft.isa_delta_c,
                    isa.as_deref(),
                );
                airport_draft_text(
                    &mut columns[0],
                    "Runway lengths m",
                    &mut draft.runway_lengths_m,
                    runways.as_deref(),
                );

                airport_draft_text(&mut columns[1], "Name", &mut draft.name, name.as_deref());
                airport_draft_text(
                    &mut columns[1],
                    "Longitude deg",
                    &mut draft.longitude_deg,
                    lon.as_deref(),
                );
                airport_draft_text(
                    &mut columns[1],
                    "Altitude m",
                    &mut draft.altitude_m,
                    alt.as_deref(),
                );
            });
        } else {
            airport_draft_text(ui, "ICAO", &mut draft.icao, icao.as_deref());
            airport_draft_text(ui, "Name", &mut draft.name, name.as_deref());
            airport_draft_text(ui, "Latitude deg", &mut draft.latitude_deg, lat.as_deref());
            airport_draft_text(
                ui,
                "Longitude deg",
                &mut draft.longitude_deg,
                lon.as_deref(),
            );
            airport_draft_text(ui, "ISA delta C", &mut draft.isa_delta_c, isa.as_deref());
            airport_draft_text(ui, "Altitude m", &mut draft.altitude_m, alt.as_deref());
            airport_draft_text(
                ui,
                "Runway lengths m",
                &mut draft.runway_lengths_m,
                runways.as_deref(),
            );
        }
    });

    ui.add_space(8.0);
    let now = ui.input(|input| input.time);
    let saved_id = egui::Id::new("custom_airport_saved_at");
    ui.horizontal_wrapped(|ui| {
        let disabled_hint = tr("Fix the highlighted fields to enable saving.");
        if let Some((key, label)) = state.custom_airport_window.target.clone() {
            if ui
                .add_enabled(
                    valid,
                    egui::Button::new(tr_fields(
                        "Save and use for {field}",
                        &[("field", label.clone())],
                    )),
                )
                .on_disabled_hover_text(disabled_hint.clone())
                .clicked()
            {
                save_and_assign(state, &key, &label);
            }
        }
        if ui
            .add_enabled(valid, egui::Button::new(tr("Save custom airport")))
            .on_hover_text(tr("Registers the airport without changing the route."))
            .on_disabled_hover_text(disabled_hint)
            .clicked()
            && state.save_custom_airport().is_some()
        {
            ui.ctx().data_mut(|data| data.insert_temp(saved_id, now));
        }
        let saved_at: Option<f64> = ui.ctx().data(|data| data.get_temp(saved_id));
        if saved_at.is_some_and(|at| now - at < SAVED_NOTICE_SECONDS) {
            ui.label(
                RichText::new(format!("{} \u{2713}", tr("Saved")))
                    .color(egui::Color32::from_rgb(63, 185, 80))
                    .strong(),
            );
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(500));
        }
        if ui
            .button(tr("Open Library..."))
            .on_hover_text(tr(
                "Load, rename, show in folder, export, import or delete saved airports.",
            ))
            .clicked()
        {
            crate::library_window::open_raised(
                state,
                ui.ctx(),
                crate::library_window::LibraryTab::Airports,
            );
        }
    });
    if let Some(status) = &state.custom_airport_status {
        ui.label(RichText::new(status).weak().small());
    }
    if let Some(note) = &state.files.airport_import_note {
        ui.label(RichText::new(note).strong().small());
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

fn airport_draft_text(ui: &mut Ui, label: &str, value: &mut String, error: Option<&str>) {
    ui.label(tr(label));
    let response = ui.add_sized(
        [ui.available_width(), ui.spacing().interact_size.y],
        egui::TextEdit::singleline(value),
    );
    if let Some(message) = error {
        let red = egui::Color32::from_rgb(229, 83, 75);
        ui.painter().rect_stroke(
            response.rect.expand(1.0),
            3.0,
            egui::Stroke::new(1.5_f32, red),
        );
        ui.label(RichText::new(message).color(red).small());
    }
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
            origin: None,
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
