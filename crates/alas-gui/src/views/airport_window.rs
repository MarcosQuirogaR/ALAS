// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native detached custom-airport editor.
//!
//! The editor is reached from the Route card's departure/arrival selectors:
//! picking [`CUSTOM_AIRPORT_OPTION`] opens this window for that field instead
//! of writing a value. Keeping the fields, import/export, and registry in a
//! separate window is what the Inputs page asked for — a route selector is a
//! one-line choice, while entering an aerodrome is a form.
//!
//! Nothing here invents operational data. Physical runway lengths are stored
//! as physical lengths; declared take-off and landing distances stay absent
//! unless the user supplies them, and the window says so rather than letting
//! a field-performance consumer read a fabricated declaration.

use alas_config::airport_io::AirportProvenanceKind;
use egui::{vec2, Context, RichText, Ui, ViewportBuilder};

use crate::native_viewport::show_native_viewport;
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

    let response = show_native_viewport(
        ctx,
        "custom_airport",
        tr("Custom airport"),
        ViewportBuilder::default()
            .with_title(tr("Custom airport"))
            .with_inner_size(vec2(860.0, 620.0))
            .with_min_inner_size(vec2(560.0, 420.0))
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
    if let Some((_, label)) = state.custom_airport_window.target.clone() {
        ui.label(
            RichText::new(tr_fields(
                "Opened from {field}. Saving with the route action registers the airport and selects it there.",
                &[("field", label)],
            ))
            .weak(),
        );
    }
    ui.add_space(6.0);
    ui.label(
        RichText::new(tr(
            "Enter an airport with an ICAO code, coordinates, ISA delta, elevation, and physical runway lengths. Physical lengths remain provenance-only unless declared TODA and LDA are supplied explicitly.",
        ))
        .weak()
        .small(),
    );

    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(tr("ICAO"));
        ui.text_edit_singleline(&mut state.custom_airport_draft.icao);
        ui.label(tr("Name"));
        ui.text_edit_singleline(&mut state.custom_airport_draft.name);
    });
    ui.horizontal_wrapped(|ui| {
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
    });
    ui.horizontal_wrapped(|ui| {
        airport_draft_text(
            ui,
            "Runway lengths m",
            &mut state.custom_airport_draft.runway_lengths_m,
        );
        airport_draft_text(
            ui,
            "Declared TODA m",
            &mut state.custom_airport_draft.declared_toda_m,
        );
        airport_draft_text(
            ui,
            "Declared LDA m",
            &mut state.custom_airport_draft.declared_lda_m,
        );
    });

    // The consequence of leaving the declarations empty is shown while the
    // draft is still editable, rather than after a route has silently lost
    // its field-performance basis.
    if declarations_incomplete(state) {
        ui.label(
            RichText::new(tr(
                "Declared TODA and LDA are empty. The airport is stored with physical runway lengths only, and declared field-performance checks will report missing data for it.",
            ))
            .color(ui.visuals().warn_fg_color)
            .small(),
        );
    }

    ui.add_space(6.0);
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
    ui.horizontal_wrapped(|ui| {
        ui.label(tr("Import / export path"));
        ui.text_edit_singleline(&mut state.custom_airport_file_path);
        if ui.button(tr("Import airports")).clicked() {
            state.import_custom_airports();
        }
        if ui.button(tr("Export airports")).clicked() {
            state.export_custom_airports();
        }
    });
    if let Some(status) = &state.custom_airport_status {
        ui.label(RichText::new(status).weak().small());
    }

    let custom = alas_config::airport_io::registered_custom_airports();
    if !custom.is_empty() {
        ui.separator();
        ui.label(RichText::new(tr("Registered custom airports")).strong());
        for airport in custom {
            let declared = airport.declared_toda_m.zip(airport.declared_lda_m);
            let runway_status = declared.map_or_else(
                || tr("physical runway lengths only; declared TODA/LDA missing"),
                |(toda, lda)| format!("declared TODA/LDA: {toda:.0}/{lda:.0} m"),
            );
            ui.label(format!(
                "{} - {} ({}) [{}]",
                airport.icao,
                airport.name,
                runway_status,
                tr(airport_provenance_label(airport.provenance.kind))
            ));
        }
    }
}

/// Whether the draft would register an airport with no declared operational
/// distances. Blank is the honest state for unknown data; this only decides
/// whether to say so.
fn declarations_incomplete(state: &AppState) -> bool {
    state.custom_airport_draft.declared_toda_m.trim().is_empty()
        || state.custom_airport_draft.declared_lda_m.trim().is_empty()
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
    ui.add_sized([110.0, 20.0], egui::TextEdit::singleline(value));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drafted(state: &mut AppState, icao: &str, name: &str) {
        state.custom_airport_draft = crate::airport_editor::CustomAirportDraft {
            icao: icao.to_owned(),
            name: name.to_owned(),
            latitude_deg: "40.47".to_owned(),
            longitude_deg: "-3.56".to_owned(),
            isa_delta_c: "0.0".to_owned(),
            altitude_m: "610.0".to_owned(),
            runway_lengths_m: "3500, 4100".to_owned(),
            declared_toda_m: String::new(),
            declared_lda_m: String::new(),
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
    fn missing_declarations_are_reported_rather_than_filled_in() {
        let mut state = AppState::default();
        drafted(&mut state, "LEMD", "Madrid Custom");
        assert!(declarations_incomplete(&state));

        let airport = state
            .custom_airport_draft
            .to_airport()
            .expect("a complete physical record is valid");
        assert_eq!(airport.declared_toda_m, None);
        assert_eq!(airport.declared_lda_m, None);
        assert_eq!(airport.runway_lengths_m, vec![3500.0, 4100.0]);

        state.custom_airport_draft.declared_toda_m = "3400".to_owned();
        state.custom_airport_draft.declared_lda_m = "3000".to_owned();
        assert!(!declarations_incomplete(&state));
    }
}
