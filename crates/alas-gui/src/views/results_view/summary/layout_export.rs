// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Headless bilingual results-panel evidence from a seeded optimized run.

mod fixture;
mod shell;
mod svg;

use crate::state::{AppState, Language};
use alas_pipeline::optimizer_summary::objective::{objective_help, objective_label, RANKING_HELP};
use egui::{Event, Pos2};
use shell::{render_frame, CASES};

#[test]
#[ignore = "exports bilingual results widgets after one seeded product search"]
fn export_summary_layout_without_horizontal_overflow() {
    let result = fixture::optimized_fixture();
    assert!(result.optimized_report.is_some());
    assert!(result
        .optimization_result
        .as_ref()
        .is_some_and(|run| run.history.n_evaluations() > 0));
    let output_dir = std::env::temp_dir().join("alas-shots");
    std::fs::create_dir_all(&output_dir).expect("evidence directory");
    alas_i18n::es::install();
    let mut matrix = Vec::new();
    for language in [Language::En, Language::Es] {
        alas_i18n::set_language(Some(language.code()));
        let label = crate::views::tr(objective_label(result.config.optimizer.objective.kind));
        let formula = crate::views::tr(objective_help(result.config.optimizer.objective.kind));
        let ranking = crate::views::tr(RANKING_HELP);
        for case in CASES {
            let mut state = AppState {
                pipeline_result: Some(result.clone()),
                pipeline_result_complete: true,
                config_values: serde_json::to_value(&result.config).expect("result configuration"),
                active_preset: result.config.preset.clone(),
                results_tab: "summary".to_owned(),
                active_page: "results".to_owned(),
                language,
                run_log_open: false,
                ..Default::default()
            };
            let context = egui::Context::default();
            crate::theme::apply_theme(crate::theme::AppTheme::Dark, &context);
            let _ = render_frame(&context, &mut state, case, 0.0, Vec::new());
            let top = render_frame(&context, &mut state, case, 0.1, Vec::new());
            let objective_rect = svg::text_rect(&top.output.shapes, &label)
                .expect("localized objective label in actual Summary widget");
            assert!(top.content.contains_rect(objective_rect));
            assert!(
                !svg::all_text(&top.output.shapes).contains(&formula),
                "formula is hover-only"
            );
            let stem = format!("summary_optimized_{}_{}", language.code(), case.name);
            matrix.push(serde_json::json!({
                "case": case.name, "language": language.code(),
                "viewport_width": top.viewport.width(), "viewport_height": top.viewport.height(),
                "content_left": top.content.left(), "content_width": top.content.width(),
                "top": format!("{stem}.svg"), "hover": format!("{stem}_tooltip.svg"),
                "bottom": format!("{stem}_bottom.svg")
            }));
            svg::write(&output_dir.join(format!("{stem}.svg")), &top, &label);

            let pointer = objective_rect.center();
            let _ = render_frame(
                &context,
                &mut state,
                case,
                0.2,
                vec![Event::PointerMoved(pointer)],
            );
            let _ = render_frame(&context, &mut state, case, 1.5, Vec::new());
            let hover = render_frame(&context, &mut state, case, 2.5, Vec::new());
            let text = svg::all_text(&hover.output.shapes);
            assert!(
                text.contains(&formula),
                "missing localized objective formula: {stem}"
            );
            assert!(
                text.contains(&ranking),
                "missing localized ranking formula: {stem}"
            );
            let tooltip_rect = svg::text_rect(&hover.output.shapes, &formula)
                .expect("objective tooltip text bounds");
            assert!(
                hover.viewport.contains_rect(tooltip_rect),
                "objective tooltip is clipped: {stem} {tooltip_rect:?}"
            );
            svg::write(
                &output_dir.join(format!("{stem}_tooltip.svg")),
                &hover,
                &formula,
            );

            // Scroll the actual Results ScrollArea to cover text below the
            // first viewport, rather than widening or stretching the panel.
            let mut seen_preset = svg::all_text(&top.output.shapes).contains(&result.config.preset);
            let mut seen_propulsion = false;
            for step in 0..40 {
                let frame = render_frame(
                    &context,
                    &mut state,
                    case,
                    4.0 + f64::from(step),
                    vec![
                        Event::PointerMoved(Pos2::new(
                            top.content.center().x,
                            top.content.bottom() - 30.0,
                        )),
                        Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: egui::vec2(0.0, -220.0),
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
                svg::assert_bounds(&frame.output.shapes);
                seen_preset |= svg::all_text(&frame.output.shapes).contains(&result.config.preset);
                seen_propulsion |= svg::all_text(&frame.output.shapes)
                    .contains(&crate::views::tr("Propulsion cycle details"));
                if step == 39 {
                    svg::write(&output_dir.join(format!("{stem}_bottom.svg")), &frame, "");
                }
            }
            assert!(
                seen_preset,
                "the real scrolled panel must include the result preset"
            );
            assert!(
                seen_propulsion,
                "scroll coverage reaches the final summary section: {stem}"
            );
        }
    }
    alas_i18n::set_language(Some("en"));
    std::fs::write(output_dir.join("summary_matrix.json"), serde_json::to_string_pretty(&serde_json::json!({
        "preset": result.config.preset, "optimization_enabled": true, "seed": 42,
        "history_evaluations": result.optimization_result.as_ref().map(|run| run.history.n_evaluations()),
        "cases": matrix,
    })).expect("matrix JSON")).expect("matrix evidence");
    for obsolete in [
        "summary_layout_760.svg",
        "summary_layout_1280.svg",
        "summary_layout_760.png",
        "summary_layout_1280.png",
    ] {
        let path = output_dir.join(obsolete);
        if path.exists() {
            std::fs::remove_file(path).expect("remove superseded summary evidence");
        }
    }
}
