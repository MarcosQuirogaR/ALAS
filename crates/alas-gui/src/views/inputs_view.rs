// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Setup > Inputs page: preset + engine selectors, the mission-requirements
//! form, the route airports, and the per-run toggles.
//!
//! A port of the reference desktop app's `InputsScreen`.

use alas_config::airport_dataset::{self, FieldSource, RunwayDataKind};
use alas_geom::builder::AircraftBuilder;
use alas_payload::{apply_cabin_preset, build_payload_layout, layout::LayoutSummary};
use alas_pipeline::{AerodynamicSolverMode, OptimizationSolverMode};
use egui::{CollapsingHeader, ComboBox, DragValue, RichText, ScrollArea, Ui};
use serde_json::Value;

use crate::state::AppState;
use crate::views::form::dynamic_form;
use crate::views::tour_data::TourTarget;
use crate::views::{tr, tr_fields};

/// Render the Inputs page.
pub fn show_inputs_view(state: &mut AppState, ui: &mut Ui) {
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
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
        if let Some(estimate) = live_payload_estimate(state) {
            ui.add_space(6.0);
            ui.label(RichText::new(tr("Live cabin estimate")).strong());
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(estimate.primary).strong());
                for metric in estimate.metrics {
                    ui.separator();
                    ui.label(metric);
                }
            });
            if state.help_verbose {
                ui.label(
                    RichText::new(tr(
                        "Derived from the selected preset's fuselage and cabin settings; it updates before a run.",
                    ))
                    .weak()
                    .small(),
                );
            }
        } else {
            ui.weak(tr(
                "Cabin estimate is unavailable while the edited geometry is incomplete.",
            ));
        }
    });
    if state.walkthrough_targets(TourTarget::AircraftConfig) {
        response.scroll_to_me(Some(egui::Align::Center));
    }
    if ui.clip_rect().intersects(response.rect) {
        state.record_walkthrough_target(TourTarget::AircraftConfig, response.rect);
    }
}

/// Display-ready layout metrics drawn from the same live cabin build as the
/// preview. Keeping the individual measures separate avoids a dense sentence
/// with semicolons and avoids presenting a model-derived AVE capacity as an
/// unknown real-world certification number.
struct LivePayloadEstimate {
    primary: String,
    metrics: Vec<String>,
}

/// Build the same detailed layout used by the report. Named cabin presets are
/// applied to a temporary configuration first, so switching Ryanair, Iberia,
/// or Emirates immediately changes the estimate before a run writes anything
/// back to the edit buffer.
fn live_payload_estimate(state: &AppState) -> Option<LivePayloadEstimate> {
    let mut config = state.typed_config()?;
    let design = state.current_design();
    if config.requirements.cabin_preset != "Custom" {
        apply_cabin_preset(&mut config, design.as_ref()).ok()?;
    }
    let airplane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(design.as_ref(), true)
        .ok()?;
    let layout = build_payload_layout(&airplane, &config, 0.0, 0.0).ok()?;
    match &layout.summary {
        LayoutSummary::Passenger(summary) => Some(LivePayloadEstimate {
            primary: tr_fields(
                "{seated} passengers seated",
                &[("seated", summary.seated_pax.to_string())],
            ),
            metrics: vec![
                tr_fields(
                    "Payload: {payload} t",
                    &[("payload", format!("{:.1}", summary.payload_t))],
                ),
                tr_fields(
                    "Hold: {used} of {capacity} t",
                    &[
                        ("used", format!("{:.1}", summary.hold_used_t)),
                        ("capacity", format!("{:.1}", summary.hold_capacity_t)),
                    ],
                ),
            ],
        }),
        LayoutSummary::Cargo(summary) => Some(LivePayloadEstimate {
            primary: tr_fields(
                "Cargo payload: {payload} t",
                &[("payload", format!("{:.1}", summary.payload_t))],
            ),
            metrics: vec![
                tr_fields(
                    "Hold capacity: {capacity} t",
                    &[("capacity", format!("{:.1}", summary.capacity_t))],
                ),
                tr_fields(
                    "{slots} loading positions",
                    &[("slots", summary.n_slots.to_string())],
                ),
            ],
        }),
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
        ui.label(
            RichText::new(tr(
                "Set the mission target, aircraft type, payload layout and design mass here. Detailed limits and stability targets stay with the selected design mode on the Design Space page.",
            ))
            .weak()
            .small(),
        );
        if let Some(values) = state.group_mut("requirements") {
            let edits = dynamic_form(ui, &fields, values, &error_fields, lang, show_help);
            if !edits.is_empty() {
                state.on_config_modified();
                for edit in edits {
                    state.note_parameter_modified(edit.label, edit.value);
                }
            }
        }
        show_clean_sheet_passenger_target(state, ui);
    });
}

/// Render the one passenger count that is a real user input. A clean-sheet
/// study has no registered cabin capacity to preserve, so its count sizes the
/// fuselage. A named aircraft uses its class shares and the usable floor
/// instead; exposing its copied planning count would make a discrete row
/// shortfall look like a failed requirement.
fn show_clean_sheet_passenger_target(state: &mut AppState, ui: &mut Ui) {
    let eligible = state
        .typed_config()
        .is_some_and(|config| config.uses_fixed_passenger_target());
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
    ui.label(RichText::new(tr("Clean-sheet passenger target")).strong());
    ui.label(
        RichText::new(tr(
            "Choose the passenger load for a clean-sheet study. Named aircraft presets derive capacity from class shares and usable cabin floor.",
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
        if state.help_verbose {
            ui.add(
                egui::Label::new(
                    RichText::new(
                        tr("Overridden by a configured SimBrief flight plan (Mission Advanced Settings) whenever its origin/destination match."),
                    )
                    .weak()
                    .small(),
                )
                .wrap(),
            );
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
    ComboBox::from_id_salt(format!("inputs_{key}_combo"))
        .width(ui.available_width())
        .selected_text(&current)
        .show_ui(ui, |ui| {
            for name in &airports {
                if ui.selectable_label(*name == current, name).clicked() {
                    chosen = Some(name.clone());
                }
            }
        });
    show_airport_resolution(ui, &current);
    if let Some(name) = chosen {
        let feedback_value = name.clone();
        if let Some(obj) = state.config_values.as_object_mut() {
            obj.insert(key.to_owned(), Value::String(name));
        }
        state.on_config_modified();
        state.note_parameter_modified(localized_label, feedback_value);
    }
}

/// Show the resolved route data next to the simple airport selector. The
/// resolver owns the distinction between curated declared distances and the
/// small OurAirports physical-runway snapshot; the GUI only reports that
/// distinction and never promotes one into the other.
fn show_airport_resolution(ui: &mut Ui, name: &str) {
    let status = airport_resolution(name);
    match status {
        AirportResolution::Missing => {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tr("No airport selected; route and field-performance data are unavailable."),
            );
        }
        AirportResolution::Unknown => {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tr("No matching airport record; routing and declared field-performance data remain unresolved."),
            );
        }
        AirportResolution::Resolved {
            source,
            runway_kind,
            elevation_m,
            latitude_deg,
            longitude_deg,
            toda_m,
            lda_m,
        } => {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(tr("Resolved airport data")).strong());
                ui.label(tr_fields(
                    "Source: {source}",
                    &[("source", source_label(source))],
                ));
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(format_optional_metric("Elevation", elevation_m, "m"));
                ui.separator();
                ui.label(format_coordinates(latitude_deg, longitude_deg));
            });
            match runway_kind {
                RunwayDataKind::DeclaredOperationalDistance => {
                    ui.label(tr_fields(
                        "Declared take-off / landing distances: {toda} / {lda} m",
                        &[
                            ("toda", format_optional_number(toda_m)),
                            ("lda", format_optional_number(lda_m)),
                        ],
                    ));
                }
                RunwayDataKind::PhysicalRunwayLength => {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        tr("Physical runway length only; declared take-off and landing distances are missing."),
                    );
                }
                RunwayDataKind::Missing => {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        tr("Runway distance data are missing; declared field-performance checks cannot use this airport."),
                    );
                }
            }
        }
    }
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
        if run_options_column_count(ui.available_width()) == 1 {
            show_run_content_options(state, ui);
            ui.add_space(10.0);
            show_run_evaluation_options(state, ui);
        } else {
            ui.columns(2, |columns| {
                show_run_content_options(state, &mut columns[0]);
                show_run_evaluation_options(state, &mut columns[1]);
            });
        }
    });
}

fn show_run_content_options(state: &mut AppState, ui: &mut Ui) {
    ui.label(RichText::new(tr("Run contents")).strong());
    let baseline = state.design_mode() == alas_config::DesignMode::BaselineSandbox;
    if baseline {
        state.run_options.optimize = false;
    }
    ui.add_enabled_ui(!baseline, |ui| {
        ui.checkbox(&mut state.run_options.optimize, tr("Optimize design space"));
    });
    if baseline {
        ui.label(
            RichText::new(tr(
                "Baseline sandbox is fixed-aircraft analysis; select New aircraft or Adapt reference to enable MADS.",
            ))
            .weak()
            .small(),
        );
    }
    if baseline {
        state.run_options.compare_baseline = false;
    }
    ui.add_enabled_ui(!baseline, |ui| {
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

fn show_run_evaluation_options(state: &mut AppState, ui: &mut Ui) {
    ui.label(RichText::new(tr("MADS optimizer")).strong());
    ui.label(
        RichText::new(tr(
            "MADS is the single product optimizer: a mesh search with a progressive barrier that ranks hard-feasible candidates before mission cost.",
        ))
        .weak()
        .small(),
    );
    ui.label(
        RichText::new(tr(
            "Termination reports the evaluation budget, mesh limit, iteration limit or fixed bounds; it is not a claim of global optimality.",
        ))
        .weak()
        .small(),
    );
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

fn run_options_column_count(available_width: f32) -> usize {
    if available_width >= 760.0 {
        2
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::{
        airport_resolution, live_payload_estimate, route_column_count, run_options_column_count,
        AirportResolution,
    };
    use crate::state::AppState;
    use alas_config::{DesignMode, FieldSource, RunwayDataKind};

    #[test]
    fn preset_changes_rebuild_the_live_cabin_and_payload_estimate() {
        let mut state = AppState::default();
        let narrowbody = live_payload_estimate(&state).expect("default preset layout");

        state.load_preset("A380-800");
        let widebody = live_payload_estimate(&state).expect("widebody preset layout");

        assert!(
            narrowbody.primary != widebody.primary || narrowbody.metrics != widebody.metrics,
            "the live estimate should reflect the selected preset's cabin geometry"
        );
        assert!(widebody.primary.contains("passengers seated"));
    }

    #[test]
    fn named_cabin_presets_change_the_live_layout_before_a_run() {
        let mut state = AppState::default();
        let ryanair = live_payload_estimate(&state).expect("Ryanair layout");
        state.config_values["requirements"]["cabin_preset"] =
            serde_json::Value::String("Iberia".to_owned());
        let iberia = live_payload_estimate(&state).expect("Iberia layout");

        assert_ne!(ryanair.primary, iberia.primary);
        assert!(ryanair
            .metrics
            .iter()
            .all(|line| !line.contains("certified")));
    }

    #[test]
    fn input_cards_add_columns_only_when_the_window_has_room() {
        assert_eq!(route_column_count(679.0), 1);
        assert_eq!(route_column_count(680.0), 2);
        assert_eq!(run_options_column_count(759.0), 1);
        assert_eq!(run_options_column_count(760.0), 2);
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
    fn passenger_count_input_is_only_available_for_a_clean_sheet() {
        let mut state = AppState::default();
        assert_eq!(state.active_preset, "AVE");
        state.set_design_mode(DesignMode::CleanSheet);
        assert!(state
            .typed_config()
            .expect("default config")
            .uses_fixed_passenger_target());
        assert!(state.active_preset.is_empty());

        state.load_preset("A380-800");
        assert!(!state
            .typed_config()
            .expect("preset config")
            .uses_fixed_passenger_target());
    }
}
