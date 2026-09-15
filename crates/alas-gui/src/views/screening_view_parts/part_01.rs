// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use alas_screen::types::ScreeningObjective;
use alas_viz::SceneView;
use egui::{vec2, DragValue, Grid, RichText, ScrollArea, Ui};
use std::path::{Path, PathBuf};

use crate::state::AppState;
use crate::views::results_view::{responsive_card_layout, CARD_GAP};
use crate::views::{tr, tr_fields};

/// Render the airfoil screening page.
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
    ui.heading(tr("Airfoil Screening"));
    if state.help_verbose {
        ui.label(
            RichText::new(tr(
                "Ranks every airfoil in the database against this design's cruise condition, in up \
                 to three fidelity stages (2-D proxy -> real 3-D wing -> MSES). No full Run needed.",
            ))
            .weak(),
        );
    }
    ui.add_space(6.0);

    show_screening_preview(&mut state.screening, ui);
    ui.add_space(12.0);
    show_options(state, ui);
    ui.add_space(8.0);
    show_screening_actions(state, ui);
    ui.add_space(12.0);
    if let Some(result) = state.screening.result.take() {
        show_result(state, ui, &result);
        state.screening.result = Some(result);
    } else {
        ui.label(RichText::new(tr("Run the sweep to see ranked candidates.")).weak());
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
        tr("MSES executables not configured (Advanced Settings > External Tools)")
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
    screening_field_label(ui, "Target CL");
    let mut use_custom_cl = o.target_cl.is_some();
    if ui
        .checkbox(&mut use_custom_cl, tr("Set explicitly"))
        .changed()
    {
        o.target_cl = use_custom_cl.then_some(o.target_cl.unwrap_or(0.5));
    }
    if let Some(target_cl) = &mut o.target_cl {
        ui.add_sized(
            [ui.available_width(), ui.spacing().interact_size.y],
            DragValue::new(target_cl).speed(0.01).range(0.01..=3.0),
        );
    } else {
        ui.label(RichText::new(tr("Use level-flight CL")).weak());
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
    screening_field_label(ui, "Flow regime");
    ui.label(RichText::new(tr("Derived from cruise Mach and sweep")).weak());
    screening_field_label(ui, "Model size");
    egui::ComboBox::from_id_salt("screening_model_size")
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
        });
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
        ui.checkbox(
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
        ui.checkbox(
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

fn screening_field_label(ui: &mut Ui, label: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(tr(label)).weak().small());
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
