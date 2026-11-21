// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Airfoil Screening (top bar > Analysis): the multi-stage sweep's options,
//! run/cancel controls, status, and ranked result table.

mod preview;
mod results;
#[cfg(test)]
mod tests;

use alas_screen::types::ScreeningObjective;
use egui::{vec2, DragValue, RichText, ScrollArea, Ui};
use std::path::{Path, PathBuf};

use crate::state::AppState;
use crate::views::{tr, tr_fields};
use preview::show_screening_preview;
use results::{show_result, translate_screening_status};

/// Render the airfoil screening workspace (the top bar's Analysis window and
/// the tour's page). The airfoil selector offers custom `.dat` import.
pub fn show_screening_view(state: &mut AppState, ui: &mut Ui) {
    // The run log is a resizable bottom panel. Keeping the complete screening
    // page in one scroll area makes the lower figures reachable at its maximum
    // height instead of letting the central panel clip them.
    ScrollArea::vertical()
        .id_salt("screening_page_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| show_screening_content(state, ui));
}

fn show_screening_content(state: &mut AppState, ui: &mut Ui) {
    ui.heading(tr("Airfoil Screening")).on_hover_text(tr(
        "Ranks every airfoil in the database against this design's cruise condition, in up \
         to three fidelity stages (2-D proxy -> real 3-D wing -> MSES). No full Run needed.",
    ));
    ui.add_space(6.0);

    let width = ui.available_width();
    if screening_page_column_count(width) == 2 {
        // Setup | results side by side; each column is clipped to its own width
        // so no card spills into its neighbour.
        let left = (width * 0.4).clamp(SETUP_COLUMN_MIN_WIDTH, 460.0);
        let right = (width - left - COLUMN_GAP - 2.0 * ui.spacing().item_spacing.x)
            .max(RESULTS_COLUMN_MIN_WIDTH);
        ui.horizontal_top(|ui| {
            for (index, column_width) in [left, right].into_iter().enumerate() {
                if index == 1 {
                    ui.add_space(COLUMN_GAP);
                }
                ui.allocate_ui_with_layout(
                    vec2(column_width, 10_000.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(column_width);
                        ui.set_max_width(column_width);
                        if index == 0 {
                            show_setup_column(state, ui);
                        } else {
                            show_results_column(state, ui);
                        }
                    },
                );
            }
        });
    } else {
        show_setup_column(state, ui);
        ui.add_space(12.0);
        show_results_column(state, ui);
    }
}

/// Narrowest width that still fits the setup cards without clipping.
const SETUP_COLUMN_MIN_WIDTH: f32 = 340.0;
/// Narrowest width that still fits one result figure card (280 px canvas plus
/// the card frame).
const RESULTS_COLUMN_MIN_WIDTH: f32 = 340.0;
/// Space between the two columns.
const COLUMN_GAP: f32 = 16.0;

/// Two columns (setup | results) when both fit side by side, else one that
/// wraps the results under the setup.
fn screening_page_column_count(available_width: f32) -> usize {
    if available_width >= SETUP_COLUMN_MIN_WIDTH + COLUMN_GAP + RESULTS_COLUMN_MIN_WIDTH + 140.0 {
        2
    } else {
        1
    }
}

/// The airfoil preview, the options and the run controls.
fn show_setup_column(state: &mut AppState, ui: &mut Ui) {
    show_screening_preview(&mut state.screening, ui);
    ui.add_space(12.0);
    show_options(state, ui);
    ui.add_space(8.0);
    show_screening_actions(state, ui);
}

/// The ranked table and the figures of the last result.
fn show_results_column(state: &mut AppState, ui: &mut Ui) {
    if let Some(result) = state.screening.result.take() {
        show_result(state, ui, &result);
        state.screening.result = Some(result);
    } else {
        ui.label(RichText::new(tr("Run the screening to see ranked candidates here.")).weak());
    }
}

fn resolved_mses_dir(state: &AppState) -> Option<PathBuf> {
    let path = state.config_values.pointer("/mses/mses_dir")?.as_str()?;
    state.tool_locator.resolve_mses_dir(Path::new(path))
}

fn show_mses_readiness(state: &mut AppState, ui: &mut Ui) {
    let configured = state
        .config_values
        .pointer("/mses/enabled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let path = state
        .config_values
        .pointer("/mses/mses_dir")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    state
        .screening
        .mses_readiness
        .refresh(&state.tool_locator, path, configured);
    let readiness = &state.screening.mses_readiness;
    let resolved = &readiness.path;
    if configured {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(if readiness.pending() {
                100
            } else {
                5000
            }));
    }
    let status = if !configured {
        tr("MSES was disabled or did not produce the requested export.")
    } else if let Some(path) = resolved {
        path.display().to_string()
    } else if readiness.pending() {
        tr("Checking...")
    } else {
        tr("MSES executables not configured (External Tools page)")
    };
    let color = if resolved.is_some() {
        crate::theme::success_color(ui.visuals())
    } else {
        ui.visuals().warn_fg_color
    };
    ui.colored_label(color, tr_fields("MSES: {status}", &[("status", status)]));
}

fn show_options(state: &mut AppState, ui: &mut Ui) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.label(RichText::new(tr("Options")).strong().size(16.0));
        ui.add_space(4.0);
        let o = &mut state.screening.options;
        if screening_option_column_count(ui.available_width()) == 1 {
            show_ranking_options(ui, o);
            ui.add_space(10.0);
            show_envelope_options(ui, o);
        } else {
            ui.columns(2, |columns| {
                show_ranking_options(&mut columns[0], o);
                show_envelope_options(&mut columns[1], o);
            });
        }
        ui.add_space(6.0);
        ui.separator();
        ui.add_space(4.0);
        ui.label(RichText::new(tr("Fidelity stages")).strong());
        if screening_option_column_count(ui.available_width()) == 1 {
            show_fidelity_options(ui, o, true);
            ui.add_space(6.0);
            show_fidelity_options(ui, o, false);
        } else {
            ui.columns(2, |columns| {
                show_fidelity_options(&mut columns[0], o, true);
                show_fidelity_options(&mut columns[1], o, false);
            });
        }
    });
}

fn show_ranking_options(ui: &mut Ui, o: &mut alas_screen::types::AirfoilScreeningOptions) {
    ui.label(RichText::new(tr("Ranking and target")).strong());
    screening_field_label(ui, "Ranking objective");
    o.objective = ScreeningObjective::Balanced;
    ui.label(tr(o.objective.as_str()));
    let mut use_custom_cl = o.target_cl.is_some();
    let target_label = screening_field_label(ui, "Target CL");
    if !use_custom_cl {
        target_label.on_hover_text(tr("Use level-flight CL"));
    }
    if crate::checkbox::checkbox(ui, &mut use_custom_cl, tr("Set explicitly")).changed() {
        o.target_cl = use_custom_cl.then_some(o.target_cl.unwrap_or(0.5));
    }
    if let Some(target_cl) = &mut o.target_cl {
        ui.add_sized(
            [ui.available_width(), ui.spacing().interact_size.y],
            DragValue::new(target_cl).speed(0.01).range(0.01..=3.0),
        );
    }
    screening_field_label(ui, "L/D weight");
    ui.add_sized(
        [ui.available_width(), ui.spacing().interact_size.y],
        DragValue::new(&mut o.ld_weight)
            .speed(0.01)
            .range(0.0..=1.0),
    );
    screening_field_label(ui, "Fuel-volume weight");
    ui.add_sized(
        [ui.available_width(), ui.spacing().interact_size.y],
        DragValue::new(&mut o.fuel_weight)
            .speed(0.01)
            .range(0.0..=1.0),
    );
    screening_field_label(ui, "Robustness weight");
    ui.add_sized(
        [ui.available_width(), ui.spacing().interact_size.y],
        DragValue::new(&mut o.robustness_weight)
            .speed(0.01)
            .range(0.0..=1.0),
    );
    screening_field_label(ui, "Top N");
    ui.add_sized(
        [ui.available_width(), ui.spacing().interact_size.y],
        DragValue::new(&mut o.top_n).range(1..=500),
    );
}

fn show_envelope_options(ui: &mut Ui, o: &mut alas_screen::types::AirfoilScreeningOptions) {
    ui.label(RichText::new(tr("Search envelope")).strong());
    screening_field_label(ui, "Off-design CL band");
    ui.add_sized(
        [ui.available_width(), ui.spacing().interact_size.y],
        DragValue::new(&mut o.cl_band).speed(0.005).range(0.0..=0.3),
    );
    screening_field_label(ui, "Flow regime")
        .on_hover_text(tr("Derived from cruise Mach and sweep"));
    ui.label(RichText::new(tr("Automatic")).weak())
        .on_hover_text(tr("Derived from cruise Mach and sweep"));
    screening_field_label(ui, "Accuracy (NeuralFoil model)").on_hover_text(tr(MODEL_SIZE_HINT));
    egui::ComboBox::from_id_salt("screening_model_size")
        .icon(crate::combo_icon::down_arrow)
        .width(ui.available_width())
        .selected_text(tr(&o.model_size))
        .show_ui(ui, |ui| {
            for size in ["small", "medium", "large", "xlarge"] {
                if ui
                    .selectable_label(o.model_size == size, tr(size))
                    .clicked()
                {
                    o.model_size = size.to_owned();
                }
            }
        })
        .response
        .on_hover_text(tr(MODEL_SIZE_HINT));
    screening_field_label(ui, "Alpha min [deg]");
    ui.add_sized(
        [ui.available_width(), ui.spacing().interact_size.y],
        DragValue::new(&mut o.alpha_min_deg).speed(0.5),
    );
    screening_field_label(ui, "Alpha max [deg]");
    ui.add_sized(
        [ui.available_width(), ui.spacing().interact_size.y],
        DragValue::new(&mut o.alpha_max_deg).speed(0.5),
    );
    screening_field_label(ui, "Alpha step [deg]");
    ui.add_sized(
        [ui.available_width(), ui.spacing().interact_size.y],
        DragValue::new(&mut o.alpha_step_deg)
            .speed(0.1)
            .range(0.1..=5.0),
    );
    screening_field_label(ui, "Name filter");
    ui.add_sized(
        [ui.available_width(), ui.spacing().interact_size.y],
        egui::TextEdit::singleline(&mut o.name_filter),
    );
    screening_field_label(ui, "Thickness-to-chord range");
    ui.horizontal(|ui| {
        ui.add(DragValue::new(&mut o.min_tc).speed(0.005).range(0.0..=0.5));
        ui.label(RichText::new(tr("to")).weak());
        ui.add(DragValue::new(&mut o.max_tc).speed(0.005).range(0.0..=0.5));
    });
}

fn show_fidelity_options(
    ui: &mut Ui,
    o: &mut alas_screen::types::AirfoilScreeningOptions,
    show_three_dimensional: bool,
) {
    if show_three_dimensional {
        crate::checkbox::checkbox(
            ui,
            &mut o.refine_3d,
            tr("Refine top candidates in 3-D (Stage 2)"),
        );
        if o.refine_3d {
            screening_field_label(ui, "Refine top N");
            ui.add_sized(
                [ui.available_width(), ui.spacing().interact_size.y],
                DragValue::new(&mut o.refine_top_n).range(1..=100),
            );
        }
    } else {
        crate::checkbox::checkbox(
            ui,
            &mut o.verify_mses,
            tr("Verify survivors with MSES (Stage 3)"),
        );
        if o.verify_mses {
            screening_field_label(ui, "MSES top N");
            ui.add_sized(
                [ui.available_width(), ui.spacing().interact_size.y],
                DragValue::new(&mut o.mses_top_n).range(1..=50),
            );
        }
    }
}

/// What the NeuralFoil model-size selector does.
const MODEL_SIZE_HINT: &str =
    "Size of the NeuralFoil surrogate network: larger is more accurate but slower.";

fn screening_field_label(ui: &mut Ui, label: &str) -> egui::Response {
    ui.add_space(4.0);
    ui.label(crate::notation::styled(
        ui,
        RichText::new(tr(label)).weak().small(),
    ))
}

fn screening_option_column_count(available_width: f32) -> usize {
    if available_width >= 700.0 {
        2
    } else {
        1
    }
}

fn show_screening_actions(state: &mut AppState, ui: &mut Ui) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tr("Run screening")).strong());
            let running = state.screening.running;
            if ui
                .add_enabled(!running, egui::Button::new(tr("Run")))
                .on_hover_text(tr("Run the sweep to see ranked candidates."))
                .clicked()
            {
                if let Some(config) = state.typed_config() {
                    let design = serde_json::to_value(&state.design_values)
                        .ok()
                        .and_then(|value| serde_json::from_value(value).ok())
                        .unwrap_or_default();
                    let mses_dir = resolved_mses_dir(state);
                    state.screening.start(config, design, mses_dir);
                }
            }
            if ui
                .add_enabled(running, egui::Button::new(tr("Cancel")))
                .clicked()
            {
                state.screening.cancel();
            }
            if running {
                ui.spinner();
            }
            ui.label(RichText::new(translate_screening_status(&state.screening.status)).weak());
        });
        ui.add_space(4.0);
        show_mses_readiness(state, ui);
        if let Some(error) = &state.screening.error {
            ui.colored_label(ui.visuals().error_fg_color, tr(error));
        }
    });
}
