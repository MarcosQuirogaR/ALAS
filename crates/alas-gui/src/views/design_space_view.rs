// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Design Space page: the optimizer's search variables, each with
//! an editable initial value and lower/upper bound.

use alas_config::{DesignMode, VariableEnvelope, DESIGN_VARIABLE_SPECS};
use egui::{DragValue, RichText, ScrollArea, Ui};

use crate::sandbox::StartingDesign;
use crate::state::AppState;
use crate::views::form::dynamic_form;

#[path = "design_space_fixed.rs"]
mod design_space_fixed;
use crate::views::{tr, tr_label};
use design_space_fixed::show_fixed_variable_picker;

/// A search variable needs room for its three numeric values. Two columns are
/// useful on a desktop, but a third makes the labels and bounds too narrow to
/// scan as one unit.
const MIN_DESIGN_COLUMN_WIDTH: f32 = 480.0;

fn display_name(spec: &alas_config::DesignVariableSpec) -> String {
    let name = if spec.unit == "m" {
        spec.name.strip_suffix("_m").unwrap_or(spec.name)
    } else if spec.unit == "deg" {
        spec.name.strip_suffix("_deg").unwrap_or(spec.name)
    } else {
        spec.name
    };
    // Sentence case: only the first word is capitalised ("Wing longitudinal shift").
    let words = name
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let words = words.replace(" x shift", " longitudinal shift");
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

/// The range the GUI proposes around a reference aircraft's value: its value
/// plus and minus this fraction. A proposal only; the solver's own defaults
/// (and `0 = disabled` for command-line configurations) are unchanged.
const AIRCRAFT_RANGE_FRACTION: f64 = 0.2;

/// Restore the design space to the active aircraft: initial values from the
/// reference design, each mutable variable's range at plus and minus
/// [`AIRCRAFT_RANGE_FRACTION`] of it (within the mode's envelope), and the
/// minimum passenger capacity at the reference seat count. A clean sheet has
/// no reference to return to, so it re-derives the box from its brief.
pub(crate) fn reset_design_space_to_aircraft(state: &mut AppState) {
    if state.starting_design() != StartingDesign::PresetAircraft {
        state.reset_design_space_bounds_to_mode();
        return;
    }
    let reference = state.starting_design_defaults();
    for (spec, value) in DESIGN_VARIABLE_SPECS
        .iter()
        .zip(reference.configured_nominal_design().to_array())
    {
        state.design_values.insert(spec.name.to_owned(), value);
    }
    // The mode's envelope first, so the proposal below stays inside it.
    state.reset_design_space_bounds_to_mode();
    if let Some(config) = state.typed_config_arc() {
        let nominal = state.current_design().unwrap_or_default();
        for envelope in config.design_envelope(&nominal) {
            if envelope.fixed {
                continue;
            }
            let value = state
                .design_values
                .get(envelope.name)
                .copied()
                .unwrap_or(envelope.nominal);
            let a = value * (1.0 - AIRCRAFT_RANGE_FRACTION);
            let b = value * (1.0 + AIRCRAFT_RANGE_FRACTION);
            let lower = a.min(b).max(envelope.lower);
            let upper = a.max(b).min(envelope.upper);
            let (lower, upper) = if lower <= upper {
                (lower, upper)
            } else {
                (envelope.lower, envelope.upper)
            };
            state
                .bounds
                .insert(envelope.name.to_owned(), (lower, upper));
        }
    }
    let seats = reference.requirements.num_passengers;
    if reference.requirements.aircraft_type != "cargo" && seats > 0 {
        if let Some(requirements) = state
            .group_mut("requirements")
            .and_then(serde_json::Value::as_object_mut)
        {
            requirements.insert(
                "min_passenger_capacity".to_owned(),
                serde_json::json!(seats),
            );
        }
    }
    state.on_config_modified();
}

/// Propose the aircraft-derived defaults once per reference aircraft, the
/// first time this page is shown for it. A saved aircraft keeps the design
/// space it was saved with, and a minimum the user already set is kept.
fn propose_aircraft_defaults_once(state: &mut AppState, ui: &Ui) {
    let id = egui::Id::new("design_space_proposed_for");
    let key = state.active_preset.clone();
    let seen = ui.data(|data| data.get_temp::<String>(id));
    if seen.as_deref() == Some(key.as_str()) {
        return;
    }
    ui.data_mut(|data| data.insert_temp(id, key));
    let untouched = state
        .typed_config_arc()
        .is_some_and(|config| config.requirements.min_passenger_capacity == 0);
    if state.starting_design() == StartingDesign::PresetAircraft
        && state.files.active_aircraft.is_none()
        && untouched
    {
        reset_design_space_to_aircraft(state);
    }
}

/// Render the Design Space page.
pub fn show_design_space_view(state: &mut AppState, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.heading(tr("Design Space")).on_hover_text(tr(
            "The starting design and the optimization choice on Aircraft set the study. Each row shows the starting design and the limits handed to the optimizer.",
        ));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button(tr_label("Reset to aircraft"))
                .on_hover_text(tr(
                    "Restore the initial values to the aircraft, set each range to the aircraft's value plus and minus 20 %, and the minimum passenger capacity to its seat count.",
                ))
                .clicked()
            {
                reset_design_space_to_aircraft(state);
            }
        });
    });
    ui.add_space(6.0);
    // What the page fills in or normalises when it opens is not an edit of
    // the case: it must not mark it unsaved.
    state.without_marking_edit(|state| propose_aircraft_defaults_once(state, ui));
    let mode = state.design_mode();

    // Fixed rows are enforced at the view boundary as well as immediately
    // before a run. This keeps the editor honest when a user returns from an
    // advanced page after changing a requirement or loading a file.
    state.without_marking_edit(|state| state.enforce_design_space_fixed_variables());

    // The mode card scrolls with the table: it is taller than a short
    // window, and above the scroll area it would push the whole table out
    // of reach.
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // The numeric boxes keep their field fill under the pointer: the
            // accent tint of a hovered widget read as a selected field.
            let field_fill = ui.visuals().widgets.inactive.weak_bg_fill;
            ui.style_mut().visuals.widgets.hovered.weak_bg_fill = field_fill;
            crate::theme::card_frame(ui).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                show_design_mode_settings(state, ui, mode);
            });
            ui.add_space(8.0);
            // Read after the mode card, whose edits change the envelope.
            let config = state.typed_config_arc().unwrap_or_default();
            let nominal = state.current_design().unwrap_or_default();
            let envelopes = config.design_envelope(&nominal);
            show_design_constraints(state, ui);
            ui.add_space(8.0);
            let columns = design_space_column_count(ui.available_width());
            for row in DESIGN_VARIABLE_SPECS.chunks(columns) {
                ui.columns(columns, |column_uis| {
                    for (index, spec) in row.iter().enumerate() {
                        if let Some(envelope) = envelopes.iter().find(|e| e.name == spec.name) {
                            show_variable_editor(state, &mut column_uis[index], spec, envelope);
                        }
                    }
                });
                ui.add_space(8.0);
            }
        });
}

/// The names shown in the product-level mode selector. The serialized values
/// remain the `DesignMode` snake-case tokens owned by `alas-config`.
pub(crate) fn design_mode_display_name(mode: DesignMode) -> String {
    tr(match mode {
        DesignMode::CleanSheet => "New aircraft",
        DesignMode::ReferenceAdaptation => "Adapt reference",
        DesignMode::BaselineSandbox => "Analyze reference",
    })
}

fn show_design_mode_settings(state: &mut AppState, ui: &mut Ui, mode: DesignMode) {
    let fields = state
        .schema
        .field("optimizer")
        .and_then(|field| match &field.entry {
            alas_config::Entry::Node(node) => node
                .fields
                .iter()
                .find(|field| field.name == "design_space"),
            alas_config::Entry::Leaf(_) => None,
        })
        .and_then(|field| match &field.entry {
            alas_config::Entry::Node(node) => Some(
                node.fields
                    .iter()
                    .filter(|field| match mode {
                        DesignMode::CleanSheet => matches!(
                            field.name,
                            "fuselage_sized_by_cabin"
                                | "engine_sized_by_requirements"
                                | "engine_rating_window"
                        ),
                        DesignMode::ReferenceAdaptation => matches!(
                            field.name,
                            "reference_fraction_half_width"
                                | "reference_angle_half_width_deg"
                                | "reference_shift_half_width_m"
                                | "reference_bump_half_width"
                        ),
                        DesignMode::BaselineSandbox => false,
                    })
                    .cloned()
                    .map(|mut field| {
                        // The mode card already provides the hierarchy; these
                        // controls should not acquire another Advanced fold.
                        field.advanced = false;
                        field
                    })
                    .collect::<Vec<_>>(),
            ),
            alas_config::Entry::Leaf(_) => None,
        })
        .unwrap_or_default();

    let title = match mode {
        DesignMode::CleanSheet => "Clean-sheet options",
        DesignMode::ReferenceAdaptation => "Reference envelope controls",
        DesignMode::BaselineSandbox => "Baseline options",
    };
    // The same disclosure header as the "Design constraints" card below; a
    // mode with nothing to edit has nothing to fold, so it keeps a plain label.
    let heading_text = crate::theme::card_title(ui, tr_label(title));
    if fields.is_empty() {
        let heading = ui.label(heading_text);
        if mode == DesignMode::BaselineSandbox {
            heading.on_hover_text(tr(
                "All design variables are fixed at the selected reference values for this run.",
            ));
        }
        return;
    }
    let header = egui::CollapsingHeader::new(heading_text)
        .id_salt(("design_space::mode_controls", title))
        .default_open(true)
        .open(crate::perf_tour::force_open());
    crate::theme::show_header(ui, header, |ui| {
        show_design_mode_controls(state, ui, mode, &fields);
    });
}

/// The controls under a mode's disclosure header: for the reference mode its
/// fixed-variable picker, then the mode's fields.
fn show_design_mode_controls(
    state: &mut AppState,
    ui: &mut Ui,
    mode: DesignMode,
    fields: &[alas_config::Field],
) {
    let lang = Some(state.language.code());
    let error_fields: std::collections::HashSet<String> = state
        .validation_findings
        .iter()
        .filter(|finding| finding.field_path.starts_with("optimizer.design_space"))
        .filter_map(|finding| finding.field_path.rsplit('.').next())
        .map(str::to_owned)
        .collect();
    let Some(optimizer) = state.group_mut("optimizer") else {
        return;
    };
    let Some(object) = optimizer.as_object_mut() else {
        return;
    };
    let values = object
        .entry("design_space".to_owned())
        .or_insert_with(|| serde_json::json!({}));
    let mut fixed_edit = None;
    if mode == DesignMode::ReferenceAdaptation {
        let list = values
            .get("reference_fixed_variables")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if let Some(updated) = show_fixed_variable_picker(ui, &list) {
            values["reference_fixed_variables"] = serde_json::json!(updated);
            fixed_edit = Some(updated);
        }
        ui.add_space(8.0);
    }
    let edits = crate::views::form::with_baseline_group("design_space", || {
        dynamic_form(ui, fields, values, &error_fields, lang, false)
    });
    if let Some(updated) = fixed_edit {
        state.reset_design_space_bounds_to_mode();
        state.on_config_modified();
        state.note_parameter_modified(tr_label("Fixed variables (reference mode)"), updated);
    }
    if !edits.is_empty() {
        // Changing a window or the clean-sheet cabin-sizing policy changes the
        // declared envelope. Rebuild run bounds from that same source.
        state.reset_design_space_bounds_to_mode();
        state.on_config_modified();
        for edit in edits {
            state.note_parameter_modified(edit.label, edit.value);
        }
    }
}

/// Render the optimizer-facing limits next to the variables they constrain.
///
/// These values remain in the requirements configuration group so the solver
/// receives exactly the same numbers. Only their presentation moves here:
/// keeping one editor for each field avoids two pages disagreeing about a
/// limit while making the relationship to the design vector explicit.
fn show_design_constraints(state: &mut AppState, ui: &mut Ui) {
    let Some(field) = state.schema.field("requirements") else {
        return;
    };
    let alas_config::Entry::Node(node) = &field.entry else {
        return;
    };
    let fields: Vec<alas_config::Field> = node
        .fields
        .iter()
        .filter(|field| field.advanced && field.name != "gravity_m_s2")
        .cloned()
        .map(|mut field| {
            // This card is already the advanced design-constraint section; a
            // second nested disclosure would put the moved fields out of view.
            field.advanced = false;
            field
        })
        .collect();
    if fields.is_empty() {
        return;
    }

    let error_fields: std::collections::HashSet<String> = state
        .validation_findings
        .iter()
        .filter(|finding| finding.field_path.starts_with("requirements"))
        .filter_map(|finding| finding.field_path.rsplit('.').next())
        .map(str::to_owned)
        .collect();
    let lang = Some(state.language.code());
    // Limits left empty fall back to the registered preset's declared value;
    // show that value in the empty box instead of "Not set".
    let declared = state.typed_config().map(|config| {
        (
            alas_config::presets::get(&config.preset)
                .ok()
                .and_then(|preset| preset.reference.mlw_kg)
                .filter(|_| config.requirements.mlw_kg.is_none()),
            alas_config::presets::published_mzfw_kg(&config)
                .filter(|_| config.requirements.mzfw_kg.is_none()),
        )
    });
    for (name, value) in [
        ("mlw_kg", declared.and_then(|d| d.0)),
        ("mzfw_kg", declared.and_then(|d| d.1)),
    ] {
        ui.ctx().data_mut(|data| match value {
            Some(v) => data.insert_temp(egui::Id::new(("optional_preset_value", name)), v),
            None => data.remove::<f64>(egui::Id::new(("optional_preset_value", name))),
        });
    }
    let mut edits = Vec::new();
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        let header =
            egui::CollapsingHeader::new(crate::theme::card_title(ui, tr_label("Design constraints")))
                .id_salt("design_space::constraints")
                .default_open(true)
                .open(crate::perf_tour::force_open());
        crate::theme::show_header(ui, header, |ui| {
            if let Some(values) = state.group_mut("requirements") {
                edits.extend(crate::views::form::with_baseline_group("requirements", || {
                    dynamic_form(ui, &fields, values, &error_fields, lang, false)
                }));
            }
        })
        .header_response
            .on_hover_text(tr(
                "Limits and stability targets used to score candidates. Their values are kept in the Input configuration and are not optimizer variables.",
            ));
    });
    if !edits.is_empty() {
        state.on_config_modified();
        for edit in edits {
            state.note_parameter_modified(edit.label, edit.value);
        }
    }
}

fn show_variable_editor(
    state: &mut AppState,
    ui: &mut Ui,
    spec: &alas_config::DesignVariableSpec,
    envelope: &VariableEnvelope,
) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr(&display_name(spec))).strong())
                .on_hover_text(tr(spec.description));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(tr(if envelope.fixed { "Fixed" } else { "Mutable" }))
                        .weak()
                        .small(),
                );
                let unit = crate::views::form::display_unit(spec.unit);
                if !unit.is_empty() {
                    ui.label(RichText::new(unit).weak().small());
                }
            });
        });
        ui.add_space(2.0);

        let decimals = spec.decimals.max(0) as usize;
        let requested_bounds = state
            .bounds
            .get(spec.name)
            .copied()
            .unwrap_or((envelope.lower, envelope.upper));
        let lower = if envelope.fixed {
            envelope.lower
        } else {
            requested_bounds.0.max(envelope.lower).min(envelope.upper)
        };
        let upper = if envelope.fixed {
            envelope.upper
        } else {
            requested_bounds.1.min(envelope.upper).max(lower)
        };
        let mut values = [
            state
                .design_values
                .get(spec.name)
                .copied()
                .unwrap_or(envelope.nominal)
                .clamp(lower, upper),
            lower,
            upper,
        ];
        let mut changed = false;
        let labels = ["Initial value", "Lower bound", "Upper bound"];
        ui.columns(3, |columns| {
            for (index, label) in labels.into_iter().enumerate() {
                let column = &mut columns[index];
                column.label(RichText::new(tr_label(label)).weak().small());
                let width = column.available_width().max(72.0);
                let height = column.spacing().interact_size.y;
                let response = column.add_enabled_ui(!envelope.fixed, |ui| {
                    ui.add_sized(
                        [width, height],
                        DragValue::new(&mut values[index])
                            .speed(0.01)
                            .max_decimals(decimals),
                    )
                });
                changed |= response.inner.changed();
            }
        });
        if changed && !envelope.fixed {
            if values[1] > values[2] {
                values.swap(1, 2);
            }
            values[0] = values[0].clamp(values[1], values[2]);
            state.design_values.insert(spec.name.to_owned(), values[0]);
            state
                .bounds
                .insert(spec.name.to_owned(), (values[1], values[2]));
            state.on_config_modified();
            state.note_parameter_modified(tr(&display_name(spec)), tr("design-space bounds"));
        }
    });
}

fn design_space_column_count(available_width: f32) -> usize {
    if available_width >= MIN_DESIGN_COLUMN_WIDTH * 2.0 {
        2
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::{
        design_mode_display_name, design_space_column_count, display_name, MIN_DESIGN_COLUMN_WIDTH,
    };
    use crate::state::AppState;
    use alas_config::{DesignMode, DESIGN_VARIABLE_SPECS};

    #[test]
    fn design_space_labels_use_human_names_and_keep_units_separate() {
        let sweep = DESIGN_VARIABLE_SPECS
            .iter()
            .find(|spec| spec.name == "sweep_deg")
            .expect("sweep variable");
        let tail = DESIGN_VARIABLE_SPECS
            .iter()
            .find(|spec| spec.name == "tail_scale")
            .expect("tail scale variable");

        assert_eq!(display_name(sweep), "Sweep");
        assert_eq!(sweep.unit, "deg");
        assert_eq!(display_name(tail), "Tail scale");
        assert_eq!(tail.unit, "-");
    }

    #[test]
    fn design_space_uses_two_columns_only_when_each_editor_remains_readable() {
        assert_eq!(
            design_space_column_count(MIN_DESIGN_COLUMN_WIDTH * 2.0 - 1.0),
            1
        );
        assert_eq!(design_space_column_count(MIN_DESIGN_COLUMN_WIDTH * 2.0), 2);
    }

    #[test]
    fn product_mode_labels_are_stable_and_baseline_is_distinct() {
        assert_eq!(
            design_mode_display_name(DesignMode::CleanSheet),
            "New aircraft"
        );
        assert_eq!(
            design_mode_display_name(DesignMode::ReferenceAdaptation),
            "Adapt reference"
        );
        assert_eq!(
            design_mode_display_name(DesignMode::BaselineSandbox),
            "Analyze reference"
        );
    }

    /// Top edge of the last painted text equal to `label`, if any is visible.
    fn visible_label_top(output: &egui::FullOutput, label: &str) -> Option<f32> {
        output
            .shapes
            .iter()
            .rev()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    let rect = text.galley.rect.translate(text.pos.to_vec2());
                    shape.clip_rect.contains_rect(rect).then_some(rect.top())
                }
                _ => None,
            })
    }

    #[test]
    fn every_design_variable_is_reachable_by_scrolling_a_short_window() {
        let mut state = AppState::default();
        state.load_preset("A220-300");
        state.set_design_mode(DesignMode::ReferenceAdaptation);
        let ctx = egui::Context::default();
        crate::theme::apply_theme(crate::theme::AppTheme::Light, &ctx);
        ctx.style_mut(|style| style.animation_time = 0.0);
        let size = egui::vec2(600.0, 420.0);
        let last = DESIGN_VARIABLE_SPECS.last().expect("design variables");
        let label = crate::views::tr(&display_name(last));
        let mut found = false;
        for frame in 0..80 {
            let events = if frame < 2 {
                Vec::new()
            } else {
                vec![
                    egui::Event::PointerMoved(egui::pos2(300.0, 300.0)),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, -200.0),
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            };
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        super::show_design_space_view(&mut state, ui);
                    });
                },
            );
            if visible_label_top(&output, &label).is_some_and(|top| top < size.y) {
                found = true;
                break;
            }
        }
        assert!(found, "{label} never scrolled into view");
    }

    #[test]
    fn selecting_reference_mode_writes_the_typed_config_and_fixes_reference_rows() {
        let mut state = AppState::default();
        state.set_design_mode(DesignMode::ReferenceAdaptation);

        assert_eq!(state.design_mode(), DesignMode::ReferenceAdaptation);
        assert_eq!(
            state
                .config_values
                .pointer("/optimizer/design_space/mode")
                .and_then(serde_json::Value::as_str),
            Some("reference_adaptation")
        );
        let config = state.typed_config().expect("typed config");
        let fixed = config.optimizer.design_space.fixed_variable_names();
        for name in fixed {
            let (lower, upper) = state.bounds[name];
            assert_eq!(lower, upper, "{name} must stay fixed in reference mode");
        }
    }
}

#[cfg(test)]
mod header_and_row_tests {
    use crate::state::AppState;
    use alas_config::DesignMode;

    /// Every painted string with its top-left corner and text height.
    fn painted(output: &egui::FullOutput) -> Vec<(String, egui::Pos2, f32)> {
        output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) => Some((
                    text.galley.text().trim().to_owned(),
                    text.pos + text.galley.rect.min.to_vec2(),
                    text.galley.rect.height(),
                )),
                _ => None,
            })
            .collect()
    }

    fn render(width: f32) -> egui::FullOutput {
        let mut state = AppState::default();
        state.load_preset("A220-300");
        state.set_design_mode(DesignMode::ReferenceAdaptation);
        let ctx = egui::Context::default();
        crate::theme::apply_theme(crate::theme::AppTheme::Dark, &ctx);
        ctx.style_mut(|style| style.animation_time = 0.0);
        let mut output = None;
        for _ in 0..3 {
            output = Some(ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 5000.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        super::show_design_space_view(&mut state, ui);
                    });
                },
            ));
        }
        output.expect("rendered frames")
    }

    /// "Reference envelope controls" is a disclosure header like "Design
    /// constraints": the same text height, and both start after a triangle,
    /// so their text sits right of the card's inner left edge by the same
    /// indent.
    #[test]
    fn the_mode_controls_and_the_constraints_share_one_header_style() {
        let output = render(1600.0);
        let texts = painted(&output);
        let find = |label: &str| {
            texts
                .iter()
                .find(|(text, _, _)| *text == crate::views::tr_label(label))
                .unwrap_or_else(|| panic!("{label} is not painted"))
        };
        let envelope = find("Reference envelope controls");
        let constraints = find("Design constraints");
        assert!(
            (envelope.2 - constraints.2).abs() < 0.5,
            "header text heights differ: {} vs {}",
            envelope.2,
            constraints.2
        );
        assert!(
            (envelope.1.x - constraints.1.x).abs() < 0.5,
            "header text starts differ: {} vs {}",
            envelope.1.x,
            constraints.1.x
        );
    }

    /// The constraint grid's rows are level: every editor box of the right
    /// column has its top at the top of one in the left column.
    #[test]
    fn the_constraint_grid_rows_are_level_across_columns() {
        let output = render(2000.0);
        let header_top = painted(&output)
            .iter()
            .find(|(text, _, _)| *text == crate::views::tr_label("Design constraints"))
            .expect("the constraints header")
            .1
            .y;
        let mut boxes: Vec<egui::Rect> = Vec::new();
        fn walk(shape: &egui::Shape, boxes: &mut Vec<egui::Rect>) {
            match shape {
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, boxes)),
                egui::Shape::Rect(rect)
                    if rect.fill != egui::Color32::TRANSPARENT
                        && (28.0..40.0).contains(&rect.rect.height())
                        && (150.0..450.0).contains(&rect.rect.width()) =>
                {
                    boxes.push(rect.rect);
                }
                _ => {}
            }
        }
        for clipped in &output.shapes {
            walk(&clipped.shape, &mut boxes);
        }
        boxes.retain(|rect| rect.top() > header_top);
        boxes.sort_by(|a, b| a.left().total_cmp(&b.left()));
        let first = boxes.first().expect("constraint editors").left();
        let (left, right): (Vec<&egui::Rect>, Vec<&egui::Rect>) =
            boxes.iter().partition(|rect| rect.left() < first + 20.0);
        assert!(
            left.len() >= 2 && right.len() >= 2,
            "{} / {}",
            left.len(),
            right.len()
        );
        for rect in &right {
            assert!(
                left.iter()
                    .any(|other| (other.top() - rect.top()).abs() < 0.5),
                "a right-column editor at y = {} has no left-column editor at its y: {:?}",
                rect.top(),
                left.iter().map(|rect| rect.top()).collect::<Vec<_>>()
            );
        }
    }
}
