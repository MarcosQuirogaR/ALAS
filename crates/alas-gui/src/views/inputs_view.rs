// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Setup > Inputs page: preset + engine selectors, the mission-requirements
//! form, the route airports, and the per-run toggles.
//!
//! A port of the reference desktop app's `InputsScreen`.

use alas_config::airport_dataset::{self, FieldSource, RunwayDataKind};
use alas_config::{AlasConfig, DesignMode};
use alas_pipeline::{AerodynamicSolverMode, OptimizationSolverMode};
use egui::{CollapsingHeader, ComboBox, DragValue, RichText, ScrollArea, Ui};
use serde_json::Value;

use crate::sandbox::StartingDesign;
use crate::state::AppState;
use crate::views::form::dynamic_form;
use crate::views::tour_data::TourTarget;
use crate::views::{tr, tr_fields};

/// Render the Inputs page.
pub fn show_inputs_view(state: &mut AppState, ui: &mut Ui) {
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            show_starting_design_card(state, ui);
            ui.add_space(8.0);
            show_aircraft_card(state, ui);
            ui.add_space(8.0);
            show_requirements_card(state, ui);
            ui.add_space(8.0);
            show_route_card(state, ui);
            ui.add_space(8.0);
            show_run_options_card(state, ui);
        });
}

fn card(ui: &mut Ui, title: &str, body: impl FnOnce(&mut Ui)) -> egui::Response {
    crate::theme::card_frame(ui)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            CollapsingHeader::new(
                RichText::new(tr(title))
                    .strong()
                    .size(16.0)
                    .color(ui.visuals().hyperlink_color),
            )
            .default_open(true)
            .show(ui, body);
        })
        .response
}

/// The starting-design choice: a clean sheet opens the sandbox, a preset
/// aircraft keeps its geometry protected. Whether a run optimizes is a
/// separate toggle below.
fn show_starting_design_card(state: &mut AppState, ui: &mut Ui) {
    let _ = card(ui, "Starting design", |ui| {
        let choice = state.starting_design();
        ui.horizontal_wrapped(|ui| {
            if ui
                .add(crate::theme::selectable_button(
                    tr("Clean sheet design"),
                    choice == StartingDesign::CleanSheet,
                ))
                .on_hover_text(tr(
                    "Open the sandbox: the first time from the AVE reference, afterwards resuming the last sandbox or custom design.",
                ))
                .clicked()
            {
                state.enter_sandbox(false);
            }
            if ui
                .add(crate::theme::selectable_button(
                    tr("Preset aircraft"),
                    choice == StartingDesign::PresetAircraft,
                ))
                .on_hover_text(tr(
                    "Analyse or adapt a registered aircraft; its defining geometry stays protected from manual edits.",
                ))
                .clicked()
                && choice != StartingDesign::PresetAircraft
            {
                if let Some((first, _)) = state.preset_names.first().cloned() {
                    state.load_preset(&first);
                }
            }
            if ui
                .add(egui::Button::new(tr("New from AVE")).small())
                .on_hover_text(tr("Open a new sandbox from the AVE reference instead of resuming."))
                .clicked()
            {
                state.enter_sandbox(true);
            }
        });
        if choice == StartingDesign::CleanSheet && state.has_custom_design() {
            ui.label(
                RichText::new(tr("A custom baseline promoted from the sandbox is active."))
                    .weak()
                    .small(),
            );
        }
    });
}

fn show_aircraft_card(state: &mut AppState, ui: &mut Ui) {
    let response = card(ui, "Aircraft Configuration", |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(tr("Preset:"));
            let names = state.preset_names.clone();
            let current_display = names
                .iter()
                .find(|(n, _)| *n == state.active_preset)
                .map(|(_, d)| d.clone())
                .unwrap_or_else(|| tr("Choose a preset..."));
            let mut chosen = None;
            let preset_mode = state.starting_design() == StartingDesign::PresetAircraft;
            ui.add_enabled_ui(preset_mode, |ui| {
                ComboBox::from_id_salt("inputs_preset_combo")
                    .selected_text(current_display)
                    .show_ui(ui, |ui| {
                        for (name, display) in &names {
                            if ui
                                .selectable_label(*name == state.active_preset, display)
                                .clicked()
                            {
                                chosen = Some(name.clone());
                            }
                        }
                    });
            });
            if let Some(name) = chosen {
                state.load_preset(&name);
            }

            ui.add_space(16.0);
            ui.label(tr("Engine:"));
            let current_engine = state
                .config_values
                .get("geometry")
                .and_then(|g| g.get("engine"))
                .and_then(|e| e.get("engine_name"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let mut chosen_engine = None;
            ComboBox::from_id_salt("inputs_engine_combo")
                .selected_text(&current_engine)
                .show_ui(ui, |ui| {
                    for name in &state.engine_names {
                        if ui.selectable_label(*name == current_engine, name).clicked() {
                            chosen_engine = Some(name.clone());
                        }
                    }
                });
            if let Some(name) = chosen_engine {
                state.set_engine(&name);
            }
        });
    });
    if state.walkthrough_targets(TourTarget::AircraftConfig) {
        response.scroll_to_me(Some(egui::Align::Center));
    }
    if ui.clip_rect().intersects(response.rect) {
        state.record_walkthrough_target(TourTarget::AircraftConfig, response.rect);
    }
}

fn show_requirements_card(state: &mut AppState, ui: &mut Ui) {
    let _ = card(ui, "TLAR / service requirements", |ui| {
        let fields = state
            .schema
            .field("requirements")
            .and_then(|f| match &f.entry {
                alas_config::Entry::Node(n) => Some(n.fields.clone()),
                _ => None,
            })
            .unwrap_or_default()
            .into_iter()
            .filter(|field| !field.advanced)
            .collect::<Vec<_>>();
        let error_fields: std::collections::HashSet<String> = state
            .validation_findings
            .iter()
            .filter(|f| f.field_path.starts_with("requirements"))
            .map(|f| {
                f.field_path
                    .rsplit('.')
                    .next()
                    .unwrap_or(&f.field_path)
                    .to_owned()
            })
            .collect();
        let lang = Some(state.language.code());
        let show_help = state.help_verbose;
        if let Some(values) = state.group_mut("requirements") {
            let edits = dynamic_form(ui, &fields, values, &error_fields, lang, show_help);
            if !edits.is_empty() {
                state.on_config_modified();
                for edit in edits {
                    state.note_parameter_modified(edit.label, edit.value);
                }
            }
        }
        show_custom_cabin_passenger_target(state, ui);
    });
}

/// Whether the passenger-count input below is meaningful for `config`.
///
/// Passenger capacity is always dynamic: resolved from the cabin class-mix
/// percentages and the candidate's actual geometry, for every study,
/// registered aircraft or clean-sheet alike. A `Custom` cabin's starting
/// count is the one passenger value a person can still hand-edit, exactly
/// like `cargo_payload_kg` and each class's `share_pct` are only editable
/// while `cabin_preset` is `Custom`.
fn custom_cabin_passenger_target_eligible(config: &AlasConfig) -> bool {
    config.requirements.aircraft_type == "passenger" && config.requirements.cabin_preset == "Custom"
}

/// Render the one passenger count that is a real user input. Exposing a
/// named aircraft's or a percentage-mix clean sheet's copied planning count
/// would make a discrete row shortfall look like a failed requirement, since
/// that count is recomputed from geometry regardless of what is shown here.
fn show_custom_cabin_passenger_target(state: &mut AppState, ui: &mut Ui) {
    let eligible = state
        .typed_config()
        .is_some_and(|config| custom_cabin_passenger_target_eligible(&config));
    if !eligible {
        return;
    }

    let mut target = state
        .config_values
        .get("requirements")
        .and_then(|values| values.get("num_passengers"))
        .and_then(Value::as_i64)
        .unwrap_or_default()
        .clamp(1, 5_000);
    ui.separator();
    ui.label(RichText::new(tr("Custom cabin passenger count")).strong());
    ui.label(
        RichText::new(tr(
            "Starting passenger count for a hand-edited Custom cabin. Every other cabin preset resolves its own capacity from the class-mix percentages and the candidate's geometry.",
        ))
        .weak()
        .small(),
    );
    let changed = ui
        .add(
            DragValue::new(&mut target)
                .range(1..=5_000)
                .speed(1.0)
                .prefix(format!("{}: ", tr("Passenger count"))),
        )
        .changed();
    if changed {
        if let Some(values) = state.group_mut("requirements") {
            values["num_passengers"] = Value::from(target);
        }
        state.on_config_modified();
        state.note_parameter_modified(tr("Passenger count"), target.to_string());
    }
}

fn show_route_card(state: &mut AppState, ui: &mut Ui) {
    let _ = card(ui, "Route", |ui| {
        let route_fields = [
            ("Departure airport", "departure_airport"),
            ("Arrival airport", "arrival_airport"),
        ];
        let columns = route_column_count(ui.available_width());
        if columns == 1 {
            for (label, key) in route_fields {
                show_route_field(state, ui, label, key);
                ui.add_space(6.0);
            }
        } else {
            ui.columns(columns, |columns| {
                for (index, (label, key)) in route_fields.iter().enumerate() {
                    show_route_field(state, &mut columns[index], label, key);
                }
            });
        }
    });
}

fn show_route_field(state: &mut AppState, ui: &mut Ui, label: &str, key: &str) {
    let airports = state.airport_names.clone();
    let localized_label = tr(label);
    ui.label(RichText::new(&localized_label).strong());
    let current = state
        .config_values
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let mut chosen = None;
    let combo_response = ComboBox::from_id_salt(format!("inputs_{key}_combo"))
        .width(ui.available_width())
        .selected_text(&current)
        .show_ui(ui, |ui| {
            for name in &airports {
                if ui.selectable_label(*name == current, name).clicked() {
                    chosen = Some(name.clone());
                }
            }
        });
    combo_response
        .response
        .on_hover_text(airport_resolution_tooltip(&current));
    if let Some(name) = chosen {
        let feedback_value = name.clone();
        if let Some(obj) = state.config_values.as_object_mut() {
            obj.insert(key.to_owned(), Value::String(name));
        }
        state.on_config_modified();
        state.note_parameter_modified(localized_label, feedback_value);
    }
}

/// Build the airport and runway details shown when the user hovers a selector.
/// The resolver still reads the same source records used by route and field
/// performance checks; the Inputs page keeps those details out of the normal
/// layout so the two airport controls stay compact.
fn airport_resolution_tooltip(name: &str) -> String {
    let details = match airport_resolution(name) {
        AirportResolution::Missing => {
            tr("No airport selected; route and field-performance data are unavailable.")
        }
        AirportResolution::Unknown => tr(
            "No matching airport record; routing and declared field-performance data remain unresolved.",
        ),
        AirportResolution::Resolved {
            source,
            runway_kind,
            elevation_m,
            latitude_deg,
            longitude_deg,
            toda_m,
            lda_m,
        } => {
            let mut lines = vec![
                tr("Resolved airport data"),
                tr_fields(
                    "Source: {source}",
                    &[("source", source_label(source))],
                ),
                format_optional_metric("Elevation", elevation_m, "m"),
                format_coordinates(latitude_deg, longitude_deg),
            ];
            lines.push(match runway_kind {
                RunwayDataKind::DeclaredOperationalDistance => tr_fields(
                    "Declared take-off / landing distances: {toda} / {lda} m",
                    &[
                        ("toda", format_optional_number(toda_m)),
                        ("lda", format_optional_number(lda_m)),
                    ],
                ),
                RunwayDataKind::PhysicalRunwayLength => tr(
                    "Physical runway length only; declared take-off and landing distances are missing.",
                ),
                RunwayDataKind::Missing => tr(
                    "Runway distance data are missing; declared field-performance checks cannot use this airport.",
                ),
            });
            lines.join("\n")
        }
    };

    format!(
        "{details}\n\n{}",
        tr("Overridden by a configured SimBrief flight plan (Mission Advanced Settings) whenever its origin/destination match.")
    )
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum AirportResolution {
    Missing,
    Unknown,
    Resolved {
        source: FieldSource,
        runway_kind: RunwayDataKind,
        elevation_m: Option<f64>,
        latitude_deg: Option<f64>,
        longitude_deg: Option<f64>,
        toda_m: Option<f64>,
        lda_m: Option<f64>,
    },
}

fn airport_resolution(name: &str) -> AirportResolution {
    if name.trim().is_empty() {
        return AirportResolution::Missing;
    }
    let Ok(record) = airport_dataset::resolve(name) else {
        return AirportResolution::Unknown;
    };
    AirportResolution::Resolved {
        source: record.name.source,
        runway_kind: record.runway_data_kind,
        elevation_m: record.elevation_m.value,
        latitude_deg: record.latitude_deg.value,
        longitude_deg: record.longitude_deg.value,
        toda_m: record.toda_m.value,
        lda_m: record.lda_m.value,
    }
}

fn source_label(source: FieldSource) -> String {
    tr(match source {
        FieldSource::CuratedChartTable => "Curated chart table",
        FieldSource::OurAirportsPublicDomain => "OurAirports public-domain snapshot",
        FieldSource::UserOverride => "User override",
        FieldSource::Missing => "Missing",
    })
}

fn format_optional_metric(label: &str, value: Option<f64>, unit: &str) -> String {
    tr_fields(
        "{label}: {value} {unit}",
        &[
            ("label", tr(label)),
            ("value", format_optional_number(value)),
            ("unit", unit.to_owned()),
        ],
    )
}

fn format_optional_number(value: Option<f64>) -> String {
    value
        .filter(|value| value.is_finite())
        .map(|value| format!("{value:.1}"))
        .unwrap_or_else(|| tr("missing"))
}

fn format_coordinates(latitude_deg: Option<f64>, longitude_deg: Option<f64>) -> String {
    tr_fields(
        "Coordinates: {latitude}, {longitude}",
        &[
            ("latitude", format_coordinate(latitude_deg, "N", "S")),
            ("longitude", format_coordinate(longitude_deg, "E", "W")),
        ],
    )
}

fn format_coordinate(value: Option<f64>, positive: &str, negative: &str) -> String {
    value
        .filter(|value| value.is_finite())
        .map(|value| {
            let hemisphere = if value >= 0.0 { positive } else { negative };
            format!("{:.4} deg {hemisphere}", value.abs())
        })
        .unwrap_or_else(|| tr("missing"))
}

fn route_column_count(available_width: f32) -> usize {
    if available_width >= 680.0 {
        2
    } else {
        1
    }
}

fn show_run_options_card(state: &mut AppState, ui: &mut Ui) {
    let _ = card(ui, "Run options", |ui| {
        show_run_content_options(state, ui);
    });
}

/// Apply the optimization toggle to the typed design mode: a preset
/// aircraft optimizes inside its reference envelope or is analysed as a
/// fixed aircraft; a custom design keeps the clean-sheet mode either way.
fn apply_optimize_choice(state: &mut AppState, optimize: bool) {
    state.run_options.optimize = optimize;
    if state.starting_design() == StartingDesign::PresetAircraft {
        state.set_design_mode(if optimize {
            DesignMode::ReferenceAdaptation
        } else {
            DesignMode::BaselineSandbox
        });
        state.run_options.optimize = optimize;
    }
    if !optimize {
        state.run_options.compare_baseline = false;
    }
}

fn show_run_content_options(state: &mut AppState, ui: &mut Ui) {
    ui.label(RichText::new(tr("Run contents")).strong());
    let mut optimize =
        state.run_options.optimize && state.design_mode() != DesignMode::BaselineSandbox;
    if ui
        .checkbox(&mut optimize, tr("Optimize design space"))
        .on_hover_text(tr(
            "On: the optimizer searches the design space before analysis. Off: the current design is analysed as drawn.",
        ))
        .changed()
    {
        apply_optimize_choice(state, optimize);
    }
    ui.add_enabled_ui(optimize, |ui| {
        ui.checkbox(
            &mut state.run_options.compare_baseline,
            tr("Compare against baseline design"),
        );
    });
    ui.checkbox(
        &mut state.run_options.write_outputs,
        tr("Write CPACS aircraft and output files"),
    );
    let mut output_dir = state
        .pipeline_options
        .output_dir
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "outputs".to_owned());
    ui.horizontal(|ui| {
        ui.label(tr("Output directory"));
        let response = ui.text_edit_singleline(&mut output_dir);
        if response.changed() && !output_dir.trim().is_empty() {
            state.pipeline_options.output_dir = Some(output_dir.trim().into());
        }
    });
    ui.label(
        RichText::new(tr(
            "Choose a separate writable folder when a previous CPACS export is open in another program.",
        ))
        .weak()
        .small(),
    );
}

/// The solver backend selections, shown in the Advanced Settings window.
pub(crate) fn show_run_evaluation_options(state: &mut AppState, ui: &mut Ui) {
    ui.label(RichText::new(tr("Aerodynamic solvers")).strong());
    ui.add_space(4.0);
    ui.label(RichText::new(tr("Aero evaluation backend")).weak().small());
    ComboBox::from_id_salt("alas_optimization_solver")
        .width(ui.available_width())
        .selected_text(match state.pipeline_options.optimization_solver {
            OptimizationSolverMode::Vlm => "ALAS VLM",
            OptimizationSolverMode::Avl => "Athena AVL",
            OptimizationSolverMode::Both => "VLM + AVL",
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(
                &mut state.pipeline_options.optimization_solver,
                OptimizationSolverMode::Vlm,
                tr("ALAS VLM"),
            );
            ui.selectable_value(
                &mut state.pipeline_options.optimization_solver,
                OptimizationSolverMode::Avl,
                tr("Athena AVL"),
            );
            ui.selectable_value(
                &mut state.pipeline_options.optimization_solver,
                OptimizationSolverMode::Both,
                tr("VLM + AVL"),
            );
        });
    ui.add_space(4.0);
    ui.label(RichText::new(tr("Aerodynamic results")).weak().small());
    ComboBox::from_id_salt("alas_aerodynamic_solver")
        .width(ui.available_width())
        .selected_text(match state.pipeline_options.aerodynamic_solver {
            AerodynamicSolverMode::Vlm => "ALAS VLM",
            AerodynamicSolverMode::Avl => "Athena AVL",
            AerodynamicSolverMode::Both => "VLM + AVL comparison",
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(
                &mut state.pipeline_options.aerodynamic_solver,
                AerodynamicSolverMode::Vlm,
                tr("ALAS VLM"),
            );
            ui.selectable_value(
                &mut state.pipeline_options.aerodynamic_solver,
                AerodynamicSolverMode::Avl,
                tr("Athena AVL"),
            );
            ui.selectable_value(
                &mut state.pipeline_options.aerodynamic_solver,
                AerodynamicSolverMode::Both,
                tr("VLM + AVL comparison"),
            );
        });
    ui.add_space(4.0);
    ui.checkbox(
        &mut state.run_options.parallel,
        tr("Run independent VLM and AVL stages in parallel"),
    )
    .on_hover_text(tr(
        "Parallel mode isolates each solver's retained artifacts and keeps a usable VLM result when AVL is unavailable.",
    ));
}

#[cfg(test)]
mod tests {
    use super::{
        airport_resolution, airport_resolution_tooltip, custom_cabin_passenger_target_eligible,
        route_column_count, AirportResolution,
    };
    use crate::state::AppState;
    use alas_config::{DesignMode, FieldSource, RunwayDataKind};

    #[test]
    fn input_cards_add_columns_only_when_the_window_has_room() {
        assert_eq!(route_column_count(679.0), 1);
        assert_eq!(route_column_count(680.0), 2);
    }

    #[test]
    fn airport_inputs_keep_declared_and_physical_runway_evidence_distinct() {
        assert!(matches!(
            airport_resolution("London Heathrow (EGLL)"),
            AirportResolution::Resolved {
                source: FieldSource::CuratedChartTable,
                runway_kind: RunwayDataKind::DeclaredOperationalDistance,
                ..
            }
        ));
        assert!(matches!(
            airport_resolution("LEBL"),
            AirportResolution::Resolved {
                source: FieldSource::OurAirportsPublicDomain,
                runway_kind: RunwayDataKind::PhysicalRunwayLength,
                ..
            }
        ));
        assert_eq!(airport_resolution("ZZZZ"), AirportResolution::Unknown);
        assert_eq!(airport_resolution(""), AirportResolution::Missing);
    }

    #[test]
    fn airport_details_are_available_as_hover_text_without_a_persistent_row() {
        let tooltip = airport_resolution_tooltip("London Heathrow (EGLL)");
        assert!(tooltip.contains("Resolved airport data"));
        assert!(tooltip.contains("Declared take-off / landing distances"));
        assert!(tooltip.contains("SimBrief"));
    }

    #[test]
    fn baseline_mode_is_a_fixed_aircraft_run_state() {
        let mut state = AppState::default();
        state.set_design_mode(DesignMode::BaselineSandbox);
        assert_eq!(state.design_mode(), DesignMode::BaselineSandbox);
        assert!(!state.run_options.optimize);
        assert!(state
            .typed_config()
            .expect("typed config")
            .optimizer
            .design_space
            .envelope(&state.current_design().expect("design"))
            .iter()
            .all(|variable| variable.fixed));
    }

    #[test]
    fn passenger_count_input_is_only_available_for_a_custom_cabin() {
        // Eligibility tracks the cabin scheme, not clean-sheet-vs-registered
        // status: the AVE default (a clean-sheet-eligible synthetic
        // aircraft) still carries the "Ryanair" percentage-mix cabin, so the
        // hand-edit input stays hidden even once the study goes clean-sheet.
        let mut state = AppState::default();
        assert_eq!(state.active_preset, "AVE");
        state.set_design_mode(DesignMode::CleanSheet);
        assert!(state.active_preset.is_empty());
        assert_eq!(
            state
                .typed_config()
                .expect("default config")
                .requirements
                .cabin_preset,
            "Ryanair"
        );
        assert!(!custom_cabin_passenger_target_eligible(
            &state.typed_config().expect("default config")
        ));

        // Switching the cabin scheme to Custom makes the input available.
        if let Some(values) = state.group_mut("requirements") {
            values["cabin_preset"] = serde_json::Value::String("Custom".to_owned());
        }
        assert!(custom_cabin_passenger_target_eligible(
            &state.typed_config().expect("custom cabin config")
        ));

        // A registered aircraft whose own preset seeds a Custom cabin (e.g.
        // the A380-800) is eligible too, even though it is not clean-sheet.
        state.load_preset("A380-800");
        assert_eq!(
            state
                .typed_config()
                .expect("preset config")
                .requirements
                .cabin_preset,
            "Custom"
        );
        assert!(custom_cabin_passenger_target_eligible(
            &state.typed_config().expect("preset config")
        ));
    }
}
