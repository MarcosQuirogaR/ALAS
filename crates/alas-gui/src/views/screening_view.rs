// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Advanced Settings > Airfoil Screening: the multi-stage sweep's options,
//! run/cancel controls, status, and ranked result table.
//!
//! A port of the reference desktop app's `AirfoilSweepScreen`.

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

    show_options(state, ui);
    ui.add_space(8.0);
    show_screening_actions(state, ui);

    ui.add_space(12.0);
    if state.screening.result.is_some() {
        let result = state.screening.result.clone();
        if let Some(result) = result.as_ref() {
            show_result(state, ui, result);
        }
    } else {
        ui.label(RichText::new(tr("Run the sweep to see ranked candidates.")).weak());
    }
}

fn resolved_mses_dir(state: &AppState) -> Option<PathBuf> {
    let config = state.typed_config()?;
    state
        .tool_locator
        .resolve_environment(
            Path::new(&config.mses.mses_dir),
            Path::new(&config.structures.nastran_exe_path),
            Path::new(&config.structures.patran_exe_path),
            Path::new(state.tool_preferences.openvsp_dir.as_deref().unwrap_or("")),
            Path::new(state.tool_preferences.avl_exe.as_deref().unwrap_or("")),
        )
        .mses_dir
}

fn show_mses_readiness(state: &AppState, ui: &mut Ui) {
    let configured = state
        .typed_config()
        .is_some_and(|config| config.mses.enabled);
    let resolved = configured.then(|| resolved_mses_dir(state)).flatten();
    let status = if !configured {
        tr("MSES was disabled or did not produce the requested export.")
    } else if let Some(path) = &resolved {
        path.display().to_string()
    } else {
        tr("MSES executables not configured (Setup > External Tools)")
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
    egui::ComboBox::from_id_salt("screening_objective")
        .width(ui.available_width())
        .selected_text(tr(o.objective.as_str()))
        .show_ui(ui, |ui| {
            for objective in [
                ScreeningObjective::Balanced,
                ScreeningObjective::Efficiency,
                ScreeningObjective::FuelCapacity,
                ScreeningObjective::Robustness,
            ] {
                ui.selectable_value(&mut o.objective, objective, tr(objective.as_str()));
            }
        });
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

fn show_result(
    state: &mut AppState,
    ui: &mut Ui,
    result: &alas_screen::types::AirfoilScreeningResult,
) {
    let summary = tr_fields(
        "{ok} of {total} candidates evaluated ({errors} errors); {refined} refined in 3-D, {verified} verified with MSES.",
        &[
            ("ok", result.n_ok.to_string()),
            ("total", result.n_total.to_string()),
            ("errors", result.n_error.to_string()),
            ("refined", result.n_refined.to_string()),
            ("verified", result.n_mses_verified.to_string()),
        ],
    );
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.label(RichText::new(tr("Screening results")).strong().size(16.0));
        ui.label(if result.cancelled {
            format!("{} {}", summary, tr("Cancelled early."))
        } else {
            summary
        });
        let target_origin = if result.uses_explicit_target_cl {
            tr("explicit target")
        } else {
            tr("level-flight target")
        };
        ui.label(
            RichText::new(tr_fields(
                "Analysis condition: target CL {cl}; section Mach {mach}; Reynolds {re}; flow regime {regime} ({origin}).",
                &[
                    ("cl", format!("{:.3}", result.cl_target)),
                    ("mach", format!("{:.3}", result.section_mach)),
                    ("re", format!("{:.3e}", result.cruise_reynolds)),
                    ("regime", tr(result.flow_regime.as_str())),
                    ("origin", target_origin),
                ],
            ))
            .weak(),
        );
        if result.transonic_caveat {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tr("Section flow is transonic: 2-D and 3-D fidelity stop capturing wave drag. Weight complete MSES-verified rows."),
            );
        }
        ui.add_space(6.0);

        ScrollArea::both()
            .id_salt("screening_result_scroll")
            .max_height(360.0)
            .show(ui, |ui| {
                Grid::new("screening_result_grid")
                    // There are nine cells in every row, including the
                    // reference marker. Keeping the count honest is what
                    // prevents the MSES state from drifting under another
                    // heading on a narrow display.
                    .num_columns(9)
                    .striped(true)
                    .spacing([10.0, 3.0])
                    .show(ui, |ui| {
                for h in [
                    "",
                    "Airfoil",
                    "L/D (2D)",
                    "CL",
                    "CD",
                    "Score",
                    "L/D (3D)",
                    "L/D (MSES)",
                    "MSES",
                ] {
                    ui.label(RichText::new(tr(h)).strong());
                }
                ui.end_row();

                        for c in &result.candidates {
                            ui.label(if c.is_reference { "*" } else { "" });
                            ui.label(&c.name);
                            ui.label(fmt_opt(c.l_over_d));
                            ui.label(fmt_opt(c.cl));
                            ui.label(fmt_opt(c.cd));
                            ui.label(fmt_opt(c.score));
                            ui.label(if c.refined {
                                fmt_opt(c.l_over_d_3d)
                            } else {
                                "-".to_owned()
                            });
                            ui.label(if c.mses_verified {
                                fmt_opt(c.l_over_d_mses)
                            } else {
                                "-".to_owned()
                            });
                            ui.label(candidate_mses_status(c));
                            ui.end_row();
                        }
                    });
            });
    });

    for candidate in result
        .candidates
        .iter()
        .filter(|candidate| !candidate.mses_verified)
        .filter_map(|candidate| {
            candidate
                .mses_error
                .as_ref()
                .map(|error| (candidate.name.as_str(), error.as_str()))
        })
        .take(3)
    {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            format!("{}: {}", candidate.0, candidate.1),
        );
    }

    ui.add_space(12.0);
    ui.label(RichText::new(tr("Screening figures")).strong());
    let config = state.typed_config().unwrap_or_default();
    let theme = state.theme.figure_theme_name().to_owned();
    let (columns, card_width) = responsive_card_layout(ui.available_width());
    for row in alas_report::SCREENING_FIGURES.chunks(columns) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = CARD_GAP;
            for descriptor in row {
                let scene = crate::scene::build_screening_figure(state, descriptor.id, &theme);
                crate::theme::card_frame(ui).show(ui, |ui| {
                    let content_width = crate::theme::card_content_width(card_width);
                    ui.set_min_width(content_width);
                    ui.set_max_width(content_width);
                    ui.label(RichText::new(tr(descriptor.title)).strong())
                        .on_hover_text(tr(descriptor.description));
                    if state.help_verbose {
                        ui.label(RichText::new(tr(descriptor.description)).weak().small());
                    }
                    let canvas_width = ui.available_width().max(280.0);
                    match scene {
                        Some(scene) => {
                            let view_key = crate::scene::figure_cache_key(
                                state.run_identity,
                                &config,
                                &theme,
                                descriptor.id,
                            );
                            let view = SceneView::new(&scene, state.view_state_mut(view_key))
                                .static_view()
                                .show_toolbar(false)
                                .wheel_zoom(true)
                                .desired_size(vec2(
                                    canvas_width,
                                    screening_figure_height(canvas_width, &scene),
                                ));
                            ui.add(view);
                        }
                        None => {
                            ui.add_sized(
                                [canvas_width, 120.0],
                                egui::Label::new(
                                    RichText::new(screening_unavailable_reason(state)).weak(),
                                ),
                            );
                        }
                    }
                });
            }
        });
        ui.add_space(16.0);
    }
}

fn screening_figure_height(width: f32, scene: &alas_report::scene::Scene) -> f32 {
    let aspect = (scene.height / scene.width.max(1.0)) as f32;
    (width * aspect).clamp(220.0, 440.0)
}

fn candidate_mses_status(candidate: &alas_screen::types::AirfoilCandidateResult) -> String {
    if candidate.mses_verified {
        return tr("MSES-verified");
    }
    match candidate.mses_status.as_deref() {
        Some("not_configured") => tr("MSES executables not configured (Setup > External Tools)"),
        Some(status) => status.to_owned(),
        None => "-".to_owned(),
    }
}

fn screening_unavailable_reason(state: &crate::state::AppState) -> String {
    if let Some(error) = state.screening.error.as_deref() {
        return tr_fields(
            "Not available: screening failed: {error}",
            &[("error", error.to_owned())],
        );
    }
    if state.screening.running {
        return tr("Not available: the screening sweep is still running.");
    }
    tr("Not available: run the airfoil screening sweep to produce this figure.")
}

fn translate_screening_status(status: &str) -> String {
    let numbers = status
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if status.starts_with("Stage 1 (2-D):") && numbers.len() == 6 {
        return tr_fields(
            "Stage 1 (2-D): {done}/{total} evaluated -- {ok} ok, {errors} errors",
            &[
                ("done", numbers[2].to_owned()),
                ("total", numbers[3].to_owned()),
                ("ok", numbers[4].to_owned()),
                ("errors", numbers[5].to_owned()),
            ],
        );
    }
    if status.starts_with("Stage 2 (3-D wing):") && numbers.len() == 5 {
        return tr_fields(
            "Stage 2 (3-D wing): {done}/{total} re-simulated -- {ok} ok",
            &[
                ("done", numbers[2].to_owned()),
                ("total", numbers[3].to_owned()),
                ("ok", numbers[4].to_owned()),
            ],
        );
    }
    if status.starts_with("Stage 3 (MSES):") && numbers.len() == 4 {
        return tr_fields(
            "Stage 3 (MSES): {done}/{total} verified -- {ok} ok",
            &[
                ("done", numbers[1].to_owned()),
                ("total", numbers[2].to_owned()),
                ("ok", numbers[3].to_owned()),
            ],
        );
    }
    if status.starts_with("Done:") && numbers.len() == 2 {
        return tr_fields(
            "Done: {ok} of {total} candidates evaluated.",
            &[
                ("ok", numbers[0].to_owned()),
                ("total", numbers[1].to_owned()),
            ],
        );
    }
    tr(status)
}

fn fmt_opt(v: Option<f64>) -> String {
    v.map(|x| format!("{x:.3}"))
        .unwrap_or_else(|| "-".to_owned())
}

#[cfg(test)]
mod tests {
    use super::screening_unavailable_reason;
    use crate::state::AppState;

    #[test]
    fn screening_without_a_completed_sweep_explains_how_to_produce_figures() {
        assert_eq!(
            screening_unavailable_reason(&AppState::default()),
            "Not available: run the airfoil screening sweep to produce this figure."
        );
    }
}
