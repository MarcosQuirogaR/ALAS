// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Aircraft page: preset + engine selectors, the mission-requirements
//! form, the route airports, and the per-run toggles.

use alas_config::airport_dataset::{self, FieldSource, RunwayDataKind};
use alas_config::{AlasConfig, DesignMode};
use alas_pipeline::{AerodynamicSolverMode, OptimizationSolverMode};
use egui::{ComboBox, RichText, ScrollArea, Ui};
use serde_json::Value;

use crate::sandbox::StartingDesign;
use crate::state::AppState;
use crate::views::airport_window;
use crate::views::form::dynamic_form;
use crate::views::inputs_selectors::{
    show_engine_selector, show_preset_selector, show_save_engine_button,
};
use crate::views::tour_data::TourTarget;
use crate::views::{tr_fields, tr_label};

/// Render the Inputs page.
pub fn show_inputs_view(state: &mut AppState, ui: &mut Ui) {
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            show_aircraft_body(state, ui);
            ui.add_space(8.0);
            crate::views::mission_profile_inputs::show_mission_profile_inputs(state, ui);
            ui.add_space(8.0);
            show_run_options_card(state, ui);
        });
}

/// The Aircraft step's cards, without a scroll area so the flow page can append
/// its Advanced sections inside one scroll.
pub(crate) fn show_aircraft_body(state: &mut AppState, ui: &mut Ui) {
    crate::views::aircraft_start::show_start(state, ui);
    ui.add_space(8.0);
    show_requirements_card(state, ui);
    ui.add_space(8.0);
    crate::views::inputs_mtow::show_mtow_card(state, ui);
    show_route_card(state, ui);
}

pub(crate) fn card(ui: &mut Ui, title: &str, body: impl FnOnce(&mut Ui)) -> egui::Response {
    crate::views::focus_card::card_for(ui, title, &[], body)
}

pub(crate) fn show_reference_card(state: &mut AppState, ui: &mut Ui) {
    let response = card(ui, "Aircraft Configuration", |ui| {
        // Two cells of the form grid, each combo filling its cell.
        let mut grid = crate::views::form::CellGrid::new(ui, "aircraft_configuration_grid");
        grid.cell(ui, |ui, _| show_preset_selector(state, ui));
        grid.cell(ui, |ui, _| show_engine_selector(state, ui));
        grid.finish(ui);
        ui.horizontal_wrapped(|ui| {
            crate::user_files_ui::save_aircraft_button(state, ui);
            show_save_engine_button(state, ui);
            crate::user_files_ui::library_button(state, ui);
            crate::user_files_ui::open_file_button(state, ui);
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
    let _ = crate::views::focus_card::card_for(
        ui,
        "TLAR / service requirements",
        &["requirements"],
        |ui| {
            // Why a run is blocked belongs on the card the value was typed into.
            crate::views::notices::show_group_issues(state, ui, "requirements");
            let fields = state
                .schema
                .field("requirements")
                .and_then(|f| match &f.entry {
                    alas_config::Entry::Node(n) => Some(n.fields.clone()),
                    _ => None,
                })
                .unwrap_or_default()
                .into_iter()
                .filter(|field| !field.advanced && field.name != "mtow_kg")
                .collect::<Vec<_>>();
            let mut fields = fields;
            super::inputs_baseline::use_loaded_cabin_preset(state, ui.ctx(), &mut fields);
            if state
                .typed_config_arc()
                .is_some_and(|config| custom_cabin_passenger_target_eligible(&config))
            {
                let at = fields
                    .iter()
                    .position(|field| field.name == "cabin_preset")
                    .map_or(fields.len(), |index| index + 1);
                fields.insert(at, passenger_count_field());
            }
            // The passenger mass stands beside the passenger count, so no row holds one field.
            if let Some(from) = fields
                .iter()
                .position(|field| field.name == "passenger_mass_kg")
            {
                let field = fields.remove(from);
                let at = fields
                    .iter()
                    .position(|field| field.name == "num_passengers")
                    .or_else(|| fields.iter().position(|field| field.name == "cabin_preset"))
                    .map_or(fields.len(), |index| index + 1);
                fields.insert(at, field);
            }
            // The fields the card adds by aircraft type and detail level fill
            // the same grid, never a second one under it.
            fields.extend(added_requirement_fields(state));
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
            if let Some(values) = state.group_mut("requirements") {
                let edits = crate::views::form::with_baseline_group("requirements", || {
                    dynamic_form(ui, &fields, values, &error_fields, lang, false)
                });
                if !edits.is_empty() {
                    state.on_config_modified();
                    for edit in edits {
                        state.note_parameter_modified(edit.label, edit.value);
                    }
                }
            }
        },
    );
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

/// The one passenger count that is a real user input, as a schema field so it
/// flows in the requirement grid like its neighbours (a lone cell on its own
/// row left the rest of the row empty). Exposing a named aircraft's or a
/// percentage-mix clean sheet's copied planning count would make a discrete
/// row shortfall look like a failed requirement, hence the Custom-cabin gate.
fn passenger_count_field() -> alas_config::Field {
    alas_config::Field {
        name: "num_passengers",
        label: "Passenger count",
        unit: "pax",
        help: "Starting passenger count for a hand-edited Custom cabin. Every other cabin preset resolves its own capacity from the class-mix percentages and the candidate's geometry.",
        long_help: "",
        advanced: false,
        visible_if: None,
        entry: alas_config::Entry::Leaf(alas_config::LeafField {
            kind: alas_config::Kind::Int,
            optional_value_kind: None,
            value: Value::from(350),
            min: Some(alas_config::Number::Integer(1)),
            max: Some(alas_config::Number::Integer(5_000)),
            decimals: None,
            columns: None,
            readonly_unless: None,
            options: None,
            zero_means: None,
        }),
    }
}

/// The requirement fields the card adds to its grid beyond the routine ones.
///
/// The cargo payload mass the user asks the design to match, for a freighter
/// only: the cargo analogue of the passenger requirement, meaningless on a
/// passenger aircraft, so it is added by type. It is advanced in the schema:
/// at the Advanced level it stands in the grid like any other requirement, at
/// Basic the card says more settings are in the Advanced view. What is entered
/// stays a target: it is scored by the objective
/// (`DesignRequirements::cargo_target_kg`) and never becomes the hold's
/// capacity or the payload a candidate carries. No validation rule rejects it.
///
/// The gravitational acceleration is a constant, not a design constraint: the
/// card shows it at the Advanced level only.
fn added_requirement_fields(state: &AppState) -> Vec<alas_config::Field> {
    let advanced = state.shell.detail == crate::detail::Detail::Advanced;
    let named = |name: &str| {
        state
            .schema
            .field("requirements")
            .and_then(|group| match &group.entry {
                alas_config::Entry::Node(node) => {
                    node.fields.iter().find(|field| field.name == name).cloned()
                }
                alas_config::Entry::Leaf(_) => None,
            })
    };
    let freighter = state
        .typed_config_arc()
        .is_some_and(|config| config.requirements.aircraft_type == "cargo");
    let mut added = Vec::new();
    if let Some(mut field) = named("cargo_objective_kg").filter(|_| freighter) {
        field.advanced = !advanced;
        added.push(field);
    }
    if let Some(mut field) = named("gravity_m_s2").filter(|_| advanced) {
        field.advanced = false;
        added.push(field);
    }
    added
}

fn show_route_card(state: &mut AppState, ui: &mut Ui) {
    let open_for = [
        "departure_airport",
        "arrival_airport",
        "toda_override_m",
        "lda_override_m",
    ];
    let _ = crate::views::focus_card::card_for(ui, "Route", &open_for, |ui| {
        // The airports and the values derived from them: one grid.
        let mut grid = crate::views::form::CellGrid::new(ui, "route_grid");
        for (label, key) in [
            ("Departure airport", "departure_airport"),
            ("Arrival airport", "arrival_airport"),
        ] {
            grid.cell(ui, |ui, _| show_route_field(state, ui, label, key));
        }
        crate::views::route_override::derived_route_cells(state, ui, &mut grid);
        grid.finish(ui);
    });
}

fn show_route_field(state: &mut AppState, ui: &mut Ui, label: &str, key: &str) {
    let airports = state.airport_names.clone();
    let localized_label = tr_label(label);
    let caption = crate::notation::styled(ui, crate::theme::field_label(&localized_label, true));
    let label_rect = crate::views::form::label_row(ui, caption, |_| {}).rect;
    let current = state
        .config_values
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let mut chosen = None;
    let mut open_custom_editor = false;
    let combo_response = ComboBox::from_id_salt(format!("inputs_{key}_combo"))
        .icon(crate::combo_icon::down_arrow)
        .height(crate::selector_search::FIXED_SEARCH_COMBO_HEIGHT)
        .width(ui.available_width())
        .truncate()
        .selected_text(&current)
        .show_ui(ui, |ui| {
            // A command rather than a route value: it opens the detached
            // custom-airport editor for this selector and is never stored.
            if ui
                .selectable_label(false, tr_label(airport_window::CUSTOM_AIRPORT_OPTION))
                .on_hover_text(tr_label(
                    "Enter, import, or reuse an airport that is not in the curated database.",
                ))
                .clicked()
            {
                open_custom_editor = true;
            }
            ui.separator();
            let salt = format!("inputs_{key}_combo");
            let query = crate::selector_search::search_field(ui, &salt);
            egui::ScrollArea::vertical()
                .max_height(320.0)
                .show(ui, |ui| {
                    for name in &airports {
                        let icao = alas_config::airports::get(name)
                            .map(|airport| airport.icao.clone())
                            .unwrap_or_default();
                        if !crate::selector_search::matches(&query, &[name, &icao]) {
                            continue;
                        }
                        if ui.selectable_label(*name == current, name).clicked() {
                            chosen = Some(name.clone());
                        }
                    }
                });
        });
    crate::selector_search::reset_when_closed(
        ui,
        &format!("inputs_{key}_combo"),
        &combo_response.inner,
    );
    crate::views::focus_card::anchor_span(ui, key, label_rect, combo_response.response.rect);
    combo_response
        .response
        .on_hover_text(airport_resolution_tooltip(&current));
    if open_custom_editor {
        crate::native_viewport::raise(ui.ctx(), "custom_airport");
        airport_window::open_for(state, key, label);
    }
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
            tr_label("No airport selected; route and field-performance data are unavailable.")
        }
        AirportResolution::Unknown => tr_label(
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
                tr_label("Resolved airport data"),
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
                RunwayDataKind::PhysicalRunwayLength => tr_label(
                    "Physical runway length only; declared take-off and landing distances are missing.",
                ),
                RunwayDataKind::Missing => tr_label(
                    "Runway distance data are missing; declared field-performance checks cannot use this airport.",
                ),
            });
            lines.join("\n")
        }
    };

    format!(
        "{details}\n\n{}",
        tr_label("Overridden by a configured SimBrief flight plan (Mission Analysis, Advanced) whenever its origin/destination match.")
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
    tr_label(match source {
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
            ("label", tr_label(label)),
            ("value", format_optional_number(value)),
            ("unit", unit.to_owned()),
        ],
    )
}

fn format_optional_number(value: Option<f64>) -> String {
    value
        .filter(|value| value.is_finite())
        .map(|value| format!("{value:.1}"))
        .unwrap_or_else(|| tr_label("missing"))
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
        .unwrap_or_else(|| tr_label("missing"))
}

pub(crate) fn show_run_options_card(state: &mut AppState, ui: &mut Ui) {
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
    // Without a search there is no second design to compare with the first.
    state.run_options.compare_baseline = optimize;
}

fn show_run_content_options(state: &mut AppState, ui: &mut Ui) {
    ui.label(RichText::new(tr_label("Run contents")).strong());
    ui.horizontal_wrapped(|ui| {
    let mut optimize = crate::views::inputs_mtow::optimize_active(state);
    if crate::checkbox::checkbox(ui, &mut optimize, tr_label("Optimize design space"))
        .on_hover_text(tr_label(
            "On: the optimizer searches the design space before analysis. Off: the current design is analysed as drawn.",
        ))
        .changed()
    {
        apply_optimize_choice(state, optimize);
    }
    ui.add_enabled_ui(optimize, |ui| {
        crate::checkbox::checkbox(
            ui,
            &mut state.run_options.compare_baseline,
            tr_label("Compare against baseline design"),
        );
    });
    crate::checkbox::checkbox(
        ui,
        &mut state.run_options.write_outputs,
        tr_label("Write CPACS aircraft and output files"),
    );
    });
    let mut output_dir = state
        .pipeline_options
        .output_dir
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "outputs".to_owned());
    ui.horizontal(|ui| {
        ui.label(tr_label("Output directory")).on_hover_text(tr_label(
            "Choose a separate writable folder when a previous CPACS export is open in another program.",
        ));
        let response = ui.add(
            egui::TextEdit::singleline(&mut output_dir)
                .desired_width((ui.available_width() - 130.0).clamp(60.0, 560.0))
                .min_size(egui::vec2(0.0, crate::theme::control_height(ui)))
                .vertical_align(egui::Align::Center),
        )
        .on_hover_text(output_dir.clone());
        if response.changed() && !output_dir.trim().is_empty() {
            state.pipeline_options.output_dir = Some(output_dir.trim().into());
        }
        let data = state.data_dir();
        let current = output_dir.clone();
        crate::file_dialog::browse_button(
            state,
            ui,
            crate::file_dialog::DialogTarget::OutputDirectory,
            |_| {
                crate::file_dialog::spec(
                    crate::file_dialog::DialogKind::PickFolder,
                    "Output directory",
                    &current,
                    &data,
                    None,
                    Vec::new(),
                )
            },
        );
    });
}

/// The solver backend selections, shown in the Advanced level.
pub(crate) fn show_run_evaluation_options(state: &mut AppState, ui: &mut Ui) {
    ui.label(RichText::new(tr_label("Aerodynamic solvers")).strong());
    ui.add_space(4.0);
    ui.label(
        RichText::new(tr_label("Aero evaluation backend"))
            .weak()
            .small(),
    );
    ComboBox::from_id_salt("alas_optimization_solver")
        .icon(crate::combo_icon::down_arrow)
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
                tr_label("ALAS VLM"),
            );
            ui.selectable_value(
                &mut state.pipeline_options.optimization_solver,
                OptimizationSolverMode::Avl,
                tr_label("Athena AVL"),
            );
            ui.selectable_value(
                &mut state.pipeline_options.optimization_solver,
                OptimizationSolverMode::Both,
                tr_label("VLM + AVL"),
            );
        });
    ui.add_space(4.0);
    ui.label(
        RichText::new(tr_label("Aerodynamic results"))
            .weak()
            .small(),
    );
    ComboBox::from_id_salt("alas_aerodynamic_solver")
        .icon(crate::combo_icon::down_arrow)
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
                tr_label("ALAS VLM"),
            );
            ui.selectable_value(
                &mut state.pipeline_options.aerodynamic_solver,
                AerodynamicSolverMode::Avl,
                tr_label("Athena AVL"),
            );
            ui.selectable_value(
                &mut state.pipeline_options.aerodynamic_solver,
                AerodynamicSolverMode::Both,
                tr_label("VLM + AVL comparison"),
            );
        });
    ui.add_space(4.0);
    crate::checkbox::checkbox(ui,
        &mut state.run_options.parallel,
        tr_label("Run independent VLM and AVL stages in parallel"),
    )
    .on_hover_text(tr_label(
        "Parallel mode isolates each solver's retained artifacts and keeps a usable VLM result when AVL is unavailable.",
    ));
}

#[cfg(test)]
mod tests {
    use super::{
        airport_resolution, airport_resolution_tooltip, custom_cabin_passenger_target_eligible,
        AirportResolution,
    };
    use crate::state::AppState;
    use alas_config::{DesignMode, FieldSource, RunwayDataKind};

    /// Paint the starting-design card on a `width`-point screen and return the
    /// rectangle of every text label egui actually laid out.
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
    fn a_loaded_aircraft_runs_as_drawn_until_the_search_is_asked_for() {
        let mut state = AppState::default();
        state.load_preset("A320-200");
        assert!(!crate::views::inputs_mtow::optimize_active(&state));
        assert!(!state.run_options.compare_baseline);
        super::apply_optimize_choice(&mut state, true);
        assert!(crate::views::inputs_mtow::optimize_active(&state));
        assert!(state.run_options.compare_baseline);
        assert_eq!(state.design_mode(), DesignMode::ReferenceAdaptation);
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
        // status. The AVE default seeds the Custom cabin of its declared
        // 777-9 arrangement, so the input stays available once the study
        // goes clean-sheet.
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
            "Custom"
        );
        assert!(custom_cabin_passenger_target_eligible(
            &state.typed_config().expect("default config")
        ));

        // A named airline profile hides the hand-edit input, and switching
        // the scheme back to Custom makes it available again.
        if let Some(values) = state.group_mut("requirements") {
            values["cabin_preset"] = serde_json::Value::String("Ryanair".to_owned());
        }
        assert!(!custom_cabin_passenger_target_eligible(
            &state.typed_config().expect("airline cabin config")
        ));
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
