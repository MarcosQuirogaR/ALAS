// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

fn show_screening_preview(screening: &mut crate::screening::ScreeningState, ui: &mut Ui) {
    screening
        .preview
        .update_filter(&screening.options.name_filter);
    if screening.preview.selected().is_none() {
        if let Some(name) = screening.preview.filtered_names().first().cloned() {
            screening.preview.select(&name);
        }
    }
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.label(RichText::new(tr("Airfoil outline")).strong());
        ui.label(RichText::new(tr("Inspect a library section without changing the aircraft. Coordinates are x/c and y/c at equal scale.")).weak().small());
        egui::ComboBox::from_id_salt("screening_preview_airfoil")
            .width(ui.available_width().min(320.0))
            .selected_text(screening.preview.selected().unwrap_or("-"))
            .show_ui(ui, |ui| {
                let row_height = ui.text_style_height(&egui::TextStyle::Button)
                    .max(ui.spacing().interact_size.y);
                let mut selected = None;
                ScrollArea::vertical().max_height(240.0).show_rows(
                    ui, row_height, screening.preview.filtered_names().len(), |ui, range| {
                    for index in range {
                        let name = &screening.preview.filtered_names()[index];
                        if ui.selectable_label(screening.preview.selected() == Some(name.as_str()), name).clicked() {
                            selected = Some(name.clone());
                        }
                    }
                });
                if let Some(name) = selected {
                    screening.preview.select(&name);
                }
            });
        if screening.preview.filtered_names().is_empty() {
            ui.label(tr("No library sections match the name filter."));
        }
        let Some(points) = screening.preview.coordinates() else {
            ui.colored_label(ui.visuals().error_fg_color, tr("Airfoil coordinates unavailable."));
            return;
        };
        let width = ui.available_width().max(1.0);
        let (rect, _) = ui.allocate_exact_size(vec2(width, (width * 0.28).clamp(100.0, 220.0)), egui::Sense::hover());
        let points = screening_outline_points(points, rect.shrink(12.0));
        ui.painter().add(egui::Shape::line(points, egui::Stroke::new(2.0_f32, ui.visuals().text_color())));
    });
}

/// Preserve the supplied physical aspect ratio; +y/c is up on screen.
fn screening_outline_points(points: &[(f64, f64)], rect: egui::Rect) -> Vec<egui::Pos2> {
    let (mut xmin, mut xmax, mut ymin, mut ymax) = (
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    );
    for &(x, y) in points {
        xmin = xmin.min(x);
        xmax = xmax.max(x);
        ymin = ymin.min(y);
        ymax = ymax.max(y);
    }
    let scale = (rect.width().max(0.0) as f64 / (xmax - xmin).max(1e-9))
        .min(rect.height().max(0.0) as f64 / (ymax - ymin).max(1e-9));
    points
        .iter()
        .map(|&(x, y)| {
            egui::pos2(
                rect.center().x + ((x - (xmin + xmax) * 0.5) * scale) as f32,
                rect.center().y - ((y - (ymin + ymax) * 0.5) * scale) as f32,
            )
        })
        .collect()
}

#[cfg(test)]
mod preview_tests {
    use super::*;

    #[test]
    fn screening_preview_filter_cache_invalidates_without_changing_selection() {
        let mut preview = crate::screening::ScreeningPreview::default();
        preview.update_filter("");
        let all_count = preview.filtered_names().len();
        assert!(all_count > 1000);
        let cached = preview.filtered_names().as_ptr();
        preview.update_filter("");
        assert_eq!(cached, preview.filtered_names().as_ptr());
        preview.select("rae2822");
        let geometry = preview.coordinates().unwrap().to_vec();
        for query in ["naca", "SC2*", "rae2822, sc20412", "no-such-section", ""] {
            preview.update_filter(query);
            assert_eq!(
                preview.filtered_names(),
                alas_screen::runner::filter_names(
                    &alas_geom::airfoil_library::AirfoilLibrary::get_available_airfoils(),
                    query
                )
            );
            assert_eq!(preview.selected(), Some("rae2822"));
            assert_eq!(preview.coordinates().unwrap(), geometry);
        }
        assert_eq!(preview.filtered_names().len(), all_count);
    }

    #[test]
    fn screening_preview_virtual_selector_paints_visible_rows_and_selects_geometry() {
        let ctx = egui::Context::default();
        let mut state = crate::screening::ScreeningState::default();
        let mut frame = |events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        vec2(760.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .show(ctx, |ui| show_screening_preview(&mut state, ui));
                },
            )
        };
        let locate = |output: &egui::FullOutput, name: &str| {
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.job.text == name => {
                        Some(text.pos + vec2(5.0, 5.0))
                    }
                    _ => None,
                })
                .unwrap()
        };
        let click = |pos, pressed| {
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::default(),
                },
            ]
        };
        let output = frame(vec![]);
        let pos = locate(&output, "2032c");
        frame(click(pos, true));
        frame(click(pos, false));
        let output = frame(vec![]);
        let names = alas_geom::airfoil_library::AirfoilLibrary::get_available_airfoils();
        let rows = output
            .shapes
            .iter()
            .filter(|shape| match &shape.shape {
                egui::Shape::Text(text) => names.contains(&text.galley.job.text.as_str()),
                _ => false,
            })
            .count();
        assert!(
            (2..40).contains(&rows),
            "only viewport rows painted, got {rows}"
        );
        let target = names[1];
        let pos = locate(&output, target);
        frame(click(pos, true));
        frame(click(pos, false));
        assert_eq!(state.preview.selected(), Some(target));
        assert_eq!(
            state.preview.coordinates().unwrap(),
            alas_geom::airfoil_library::AirfoilLibrary::get(target)
                .unwrap()
                .coordinates
        );
    }

    #[test]
    #[ignore = "manual local timing evidence, not a performance threshold"]
    fn screening_preview_frame_timings() {
        use std::time::Instant;
        let ctx = egui::Context::default();
        let mut state = crate::screening::ScreeningState::default();
        let mut frame = |events: Vec<egui::Event>, filter: &str| {
            state.options.name_filter = filter.to_owned();
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        vec2(760.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .show(ctx, |ui| show_screening_preview(&mut state, ui));
                },
            )
        };
        let cold = Instant::now();
        let output = frame(vec![], "");
        eprintln!("cold frame {:?}", cold.elapsed());
        let selector = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == "2032c" => {
                    Some(text.pos + vec2(5.0, 5.0))
                }
                _ => None,
            })
            .expect("first library name selector");
        for (label, open) in [("closed", false), ("open", true)] {
            if open {
                let _ = frame(
                    vec![
                        egui::Event::PointerMoved(selector),
                        egui::Event::PointerButton {
                            pos: selector,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: egui::Modifiers::default(),
                        },
                    ],
                    "",
                );
                let _ = frame(
                    vec![egui::Event::PointerButton {
                        pos: selector,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::default(),
                    }],
                    "",
                );
            }
            let start = Instant::now();
            for _ in 0..300 {
                std::hint::black_box(frame(vec![], ""));
            }
            eprintln!("{label} average {:?}", start.elapsed() / 300);
        }
        let start = Instant::now();
        for index in 0..300 {
            std::hint::black_box(frame(vec![], if index % 2 == 0 { "naca" } else { "sc2" }));
        }
        eprintln!("edited filter average {:?}", start.elapsed() / 300);
        let names = alas_geom::airfoil_library::AirfoilLibrary::get_available_airfoils();
        eprintln!("library names {}", names.len());
        for query in ["", "naca"] {
            let start = Instant::now();
            for _ in 0..1000 {
                std::hint::black_box(alas_screen::runner::filter_names(&names, query));
            }
            eprintln!("filter {query:?} average {:?}", start.elapsed() / 1000);
        }
        let start = Instant::now();
        for _ in 0..1000 {
            std::hint::black_box(alas_geom::airfoil_library::AirfoilLibrary::get("rae2822"));
        }
        eprintln!("resolve rae2822 average {:?}", start.elapsed() / 1000);
    }

    #[test]
    fn screening_preview_renders_before_results_in_all_themes_and_widths() {
        use crate::theme::{apply_theme, AppTheme};
        for theme in [AppTheme::Light, AppTheme::Grey, AppTheme::Dark] {
            for width in [320.0, 760.0, 1400.0] {
                let ctx = egui::Context::default();
                apply_theme(theme, &ctx);
                let mut screening = crate::screening::ScreeningState::default();
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            vec2(width, 900.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default()
                            .show(ctx, |ui| show_screening_preview(&mut screening, ui));
                    },
                );
                assert!(screening.preview.coordinates().is_some());
                let outline = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Path(path) if path.points.len() > 20 => Some(path),
                        _ => None,
                    })
                    .expect("resolved outline is painted before any sweep");
                assert!(outline.points.iter().all(|point| point.x >= 0.0
                    && point.x <= width
                    && point.y >= 0.0
                    && point.y <= 900.0));
            }
        }
    }

    #[test]
    fn screening_preview_uses_library_geometry_before_and_during_run() {
        let mut state = crate::screening::ScreeningState::default();
        state.preview.select("rae2822");
        let expected = alas_geom::airfoil_library::AirfoilLibrary::get("rae2822").unwrap();
        assert_eq!(
            state.preview.coordinates().unwrap(),
            expected.coordinates.as_slice()
        );
        assert!(state.result.is_none());
        state.running = true;
        state.preview.select("sc20412");
        assert_eq!(state.preview.selected(), Some("sc20412"));
        state.preview.select("missing-section-for-preview-test");
        assert!(state.preview.coordinates().is_none());
    }

    #[test]
    fn screening_preview_selection_does_not_mutate_configuration() {
        let mut state = AppState::default();
        let before = serde_json::to_value(state.typed_config().unwrap()).unwrap();
        let design = state.design_values.clone();
        state.screening.preview.select("rae2822");
        state.screening.preview.select("sc20412");
        assert_eq!(
            before,
            serde_json::to_value(state.typed_config().unwrap()).unwrap()
        );
        assert_eq!(design, state.design_values);
    }

    #[test]
    fn screening_preview_projection_has_equal_axes_and_positive_y_up() {
        let points = screening_outline_points(
            &[(0.0, 0.0), (1.0, 0.0), (0.0, 0.2)],
            egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(300.0, 100.0)),
        );
        assert!((points[1].x - points[0].x - 300.0).abs() < 1e-4);
        assert!((points[0].y - points[2].y - 60.0).abs() < 1e-4);
    }

    #[test]
    fn screening_preview_remains_selected_when_results_reorder_or_clear() {
        use alas_screen::types::{AirfoilCandidateResult, AirfoilScreeningResult};
        let mut state = crate::screening::ScreeningState::default();
        state.preview.select("rae2822");
        state.result = Some(AirfoilScreeningResult {
            candidates: vec![
                AirfoilCandidateResult {
                    name: "rae2822".into(),
                    ..Default::default()
                },
                AirfoilCandidateResult {
                    name: "sc20412".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        });
        state.result.as_mut().unwrap().candidates.reverse();
        assert_eq!(state.preview.selected(), Some("rae2822"));
        state.result = None;
        assert_eq!(state.preview.selected(), Some("rae2822"));
    }
}
