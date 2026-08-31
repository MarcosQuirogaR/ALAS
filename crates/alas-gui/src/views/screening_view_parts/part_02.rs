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

