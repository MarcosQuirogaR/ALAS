// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

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
                    // There are ten cells in every row, including the
                    // reference marker. Keeping the count honest is what
                    // prevents the MSES state from drifting under another
                    // heading on a narrow display.
                    .num_columns(10)
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
                    "CFD",
                ] {
                    ui.label(RichText::new(tr(h)).strong());
                }
                ui.end_row();

                        for c in &result.candidates {
                            ui.label(if c.is_reference { "*" } else { "" });
                            if ui
                                .selectable_label(
                                    state.screening.preview.selected() == Some(c.name.as_str()),
                                    &c.name,
                                )
                                .clicked()
                            {
                                state.screening.preview.select(&c.name);
                                ui.ctx().request_repaint();
                            }
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
                            if ui
                                .small_button(tr("Open CFD"))
                                .on_hover_text(tr("Open an independent Airfoil CFD study for this database section."))
                                .clicked()
                            {
                                state.open_cfd_for_airfoil(&c.name);
                            }
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
    let theme = state.theme.figure_theme_name().to_owned();
    let language = alas_i18n::get_language();
    let (columns, card_width) = responsive_card_layout(ui.available_width());
    for row in alas_report::SCREENING_FIGURES.chunks(columns) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = CARD_GAP;
            for descriptor in row {
                let scene = state.screening.figure_cache.get(
                    result,
                    state.screening.result_revision,
                    &theme,
                    &language,
                    descriptor.id,
                );
                let scene_revision = state.screening.figure_cache.revision();
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
                            let size =
                                vec2(canvas_width, screening_figure_height(canvas_width, &scene));
                            if !ui.is_rect_visible(egui::Rect::from_min_size(
                                ui.next_widget_position(),
                                size,
                            )) {
                                // Preserve scroll layout without rasterizing cards below
                                // the viewport on first entry to the screening page.
                                ui.allocate_space(size);
                                return;
                            }
                            let view_key = format!("screening:{}", descriptor.id);
                            let view = SceneView::new(&scene, state.view_state_mut(view_key))
                                .cache_key(("screening", descriptor.id))
                                .cache_revision(scene_revision)
                                .static_view()
                                .show_toolbar(false)
                                // Screening cards share the page ScrollArea;
                                // zoom is available in the fullscreen/result
                                // viewer instead of stealing page scrolling.
                                .wheel_zoom(false)
                                .desired_size(size);
                            ui.add(view);
                        }
                        None => {
                            ui.add_sized(
                                [canvas_width, 120.0],
                                egui::Label::new(
                                    RichText::new(screening_unavailable_reason(
                                        state,
                                        Some(result),
                                        descriptor.id,
                                    ))
                                    .weak(),
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

fn screening_unavailable_reason(
    state: &crate::state::AppState,
    completed_result: Option<&alas_screen::types::AirfoilScreeningResult>,
    figure_id: &str,
) -> String {
    if let Some(error) = state.screening.error.as_deref() {
        return tr_fields(
            "Not available: screening failed: {error}",
            &[("error", error.to_owned())],
        );
    }
    if state.screening.running {
        return tr("Not available: the screening sweep is still running.");
    }

    let Some(result) = completed_result else {
        return tr("Not available: run the airfoil screening sweep to produce this figure.");
    };

    match figure_id {
        "rerank_2d_3d" => screening_stage_2_unavailable_reason(result),
        "mses_verification" => screening_mses_unavailable_reason(result),
        "trade_map" | "ranking_bars" | "section_shapes" if !has_screening_stage_1_data(result) => {
            tr("Not available: screening completed, but no candidate produced usable 2-D data.")
        }
        _ => {
            tr("Not available: the completed screening result has no usable data for this figure.")
        }
    }
}

fn has_screening_stage_1_data(result: &alas_screen::types::AirfoilScreeningResult) -> bool {
    result
        .candidates
        .iter()
        .any(|candidate| candidate.status == "ok")
}

fn has_screening_stage_2_data(result: &alas_screen::types::AirfoilScreeningResult) -> bool {
    result
        .candidates
        .iter()
        .any(|candidate| candidate.status == "ok" && candidate.refined)
}

fn screening_stage_2_attempted(result: &alas_screen::types::AirfoilScreeningResult) -> bool {
    result.n_refined > 0
        || result
            .candidates
            .iter()
            .any(|candidate| candidate.refine_error.is_some())
}

fn screening_stage_2_unavailable_reason(
    result: &alas_screen::types::AirfoilScreeningResult,
) -> String {
    if !has_screening_stage_1_data(result) {
        return tr(
            "Not available: screening completed, but no candidate produced usable 2-D data.",
        );
    }
    if result.cancelled && !has_screening_stage_2_data(result) {
        return tr(
            "Not available: screening was cancelled before Stage 2 (3-D wing) produced usable data.",
        );
    }
    if screening_stage_2_attempted(result) {
        return tr(
            "Not available: Stage 2 (3-D wing) completed, but no candidate produced usable data.",
        );
    }
    tr("Not available: Stage 2 (3-D wing) was not run, so this figure has no data.")
}

fn screening_mses_unavailable_reason(
    result: &alas_screen::types::AirfoilScreeningResult,
) -> String {
    if !has_screening_stage_1_data(result) {
        return tr(
            "Not available: screening completed, but no candidate produced usable 2-D data.",
        );
    }
    if !has_screening_stage_2_data(result) {
        return if result.cancelled {
            tr("Not available: screening was cancelled before any candidate was MSES-verified.")
        } else {
            tr("Not available: no candidate reached Stage 2 (3-D wing), so MSES verification has no input.")
        };
    }

    let refined = result
        .candidates
        .iter()
        .filter(|candidate| candidate.status == "ok" && candidate.refined);
    let statuses = refined
        .filter_map(|candidate| candidate.mses_status.as_deref())
        .collect::<Vec<_>>();

    if result.cancelled && result.n_mses_verified == 0 {
        return tr(
            "Not available: screening was cancelled before any candidate was MSES-verified.",
        );
    }
    if !statuses.is_empty() && statuses.iter().all(|status| *status == "not_configured") {
        return tr(
            "Not available: MSES verification was selected, but no MSES installation was resolved (Setup > External Tools).",
        );
    }
    if !statuses.is_empty() && statuses.iter().all(|status| *status == "disabled") {
        return tr(
            "Not available: MSES verification was disabled, so this figure has no Stage 3 data.",
        );
    }
    if statuses.is_empty() {
        return tr(
            "Not available: MSES verification was not run, so this figure has no Stage 3 data.",
        );
    }
    tr("Not available: MSES verification completed, but no candidate produced usable Stage 3 data.")
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
    use alas_screen::types::{AirfoilCandidateResult, AirfoilScreeningResult};

    fn result_with_candidate(candidate: AirfoilCandidateResult) -> AirfoilScreeningResult {
        AirfoilScreeningResult {
            n_total: 1,
            n_ok: usize::from(candidate.status == "ok"),
            n_error: usize::from(candidate.status != "ok"),
            n_refined: usize::from(candidate.refined),
            n_mses_verified: usize::from(candidate.mses_verified),
            candidates: vec![candidate],
            ..Default::default()
        }
    }

    #[test]
    fn screening_without_a_completed_sweep_explains_how_to_produce_figures() {
        assert_eq!(
            screening_unavailable_reason(&AppState::default(), None, "trade_map"),
            "Not available: run the airfoil screening sweep to produce this figure."
        );
    }

    #[test]
    fn completed_result_without_stage_1_data_does_not_ask_for_another_sweep() {
        let result = AirfoilScreeningResult {
            n_total: 1,
            n_error: 1,
            ..Default::default()
        };
        let message =
            screening_unavailable_reason(&AppState::default(), Some(&result), "trade_map");
        assert_eq!(
            message,
            "Not available: screening completed, but no candidate produced usable 2-D data."
        );
    }

    #[test]
    fn completed_stage_2_failure_names_the_missing_stage_data() {
        let result = result_with_candidate(AirfoilCandidateResult {
            name: "failed-refinement".to_owned(),
            status: "ok".to_owned(),
            refine_error: Some("trim failed".to_owned()),
            ..Default::default()
        });
        assert_eq!(
            screening_unavailable_reason(&AppState::default(), Some(&result), "rerank_2d_3d"),
            "Not available: Stage 2 (3-D wing) completed, but no candidate produced usable data."
        );
    }

    #[test]
    fn completed_mses_failure_names_the_missing_stage_3_data() {
        let result = result_with_candidate(AirfoilCandidateResult {
            name: "failed-mses".to_owned(),
            status: "ok".to_owned(),
            refined: true,
            mses_status: Some("error".to_owned()),
            mses_error: Some("no convergence".to_owned()),
            ..Default::default()
        });
        assert_eq!(
            screening_unavailable_reason(&AppState::default(), Some(&result), "mses_verification"),
            "Not available: MSES verification completed, but no candidate produced usable Stage 3 data."
        );
    }

    #[test]
    fn failed_and_running_states_keep_priority_over_completed_context() {
        let result = AirfoilScreeningResult::default();
        let mut state = AppState::default();
        state.screening.error = Some("worker failed".to_owned());
        assert_eq!(
            screening_unavailable_reason(&state, Some(&result), "trade_map"),
            "Not available: screening failed: worker failed"
        );

        state.screening.error = None;
        state.screening.running = true;
        assert_eq!(
            screening_unavailable_reason(&state, Some(&result), "trade_map"),
            "Not available: the screening sweep is still running."
        );
    }
}
