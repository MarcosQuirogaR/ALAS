// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

#[cfg(test)]
mod performance_tests {
    use super::*;

    #[test]
    #[ignore = "local frame-timing probe; no solver execution"]
    fn screening_page_timing_probe() {
        let mut state = AppState::default();
        state.config_values["mses"]["enabled"] = serde_json::json!(true);
        let ctx = egui::Context::default();
        crate::theme::apply_theme(state.theme, &ctx);
        for frame in 0..8 {
            let start = std::time::Instant::now();
            let _output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        vec2(1200.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .show(ctx, |ui| show_screening_view(&mut state, ui));
                },
            );
            eprintln!(
                "screening frame {frame}: {:.3} ms",
                start.elapsed().as_secs_f64() * 1000.0
            );
        }
    }

    #[test]
    #[ignore = "local completed-result frame timing; synthetic display fixture, no solver"]
    fn screening_completed_page_timing_probe() {
        let mut state = AppState::default();
        state.config_values["mses"]["enabled"] = serde_json::json!(true);
        state.screening.result = Some(alas_screen::types::AirfoilScreeningResult {
            n_total: 50,
            n_ok: 50,
            candidates: (0..50)
                .map(|index| alas_screen::types::AirfoilCandidateResult {
                    name: "naca0012".into(),
                    status: "ok".into(),
                    cl: Some(0.5),
                    cd: Some(0.01 + index as f64 * 0.0001),
                    l_over_d: Some(50.0 - index as f64 * 0.1),
                    score: Some(1.0 - index as f64 * 0.01),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        });
        let ctx = egui::Context::default();
        crate::theme::apply_theme(state.theme, &ctx);
        for frame in 0..8 {
            let start = std::time::Instant::now();
            let _output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        vec2(1200.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .show(ctx, |ui| show_screening_view(&mut state, ui));
                },
            );
            assert_eq!(
                state.screening.result.as_ref().unwrap().candidates.len(),
                50
            );
            eprintln!(
                "completed screening frame {frame}: {:.3} ms",
                start.elapsed().as_secs_f64() * 1000.0
            );
        }
    }
    #[test]
    fn offscreen_result_cards_keep_layout_and_render_when_scrolled_into_view() {
        let mut state = AppState::default();
        let result = alas_screen::types::AirfoilScreeningResult {
            n_total: 1,
            n_ok: 1,
            candidates: vec![alas_screen::types::AirfoilCandidateResult {
                name: "naca0012".into(),
                status: "ok".into(),
                cl: Some(0.5),
                cd: Some(0.01),
                l_over_d: Some(50.0),
                score: Some(1.0),
                ..Default::default()
            }],
            ..Default::default()
        };
        let ctx = egui::Context::default();
        crate::theme::apply_theme(state.theme, &ctx);
        let mut frame = |offset: f32| {
            let mut content_height = 0.0;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        vec2(900.0, 500.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let scroll = ScrollArea::vertical()
                            .id_salt("result_visibility_regression")
                            .auto_shrink([false, false])
                            .vertical_scroll_offset(offset)
                            .show(ui, |ui| {
                                ui.add_space(1000.0);
                                show_result(&mut state, ui, &result);
                            });
                        content_height = scroll.content_size.y;
                    });
                },
            );
            // Texture zero is egui's font atlas; show_result has no other
            // image producer besides the figure SceneViews.
            let uploaded_figures = output
                .textures_delta
                .set
                .iter()
                .filter(|(id, _)| *id != egui::TextureId::default())
                .count();
            let visible_figures = ctx
                .tessellate(output.shapes, output.pixels_per_point)
                .iter()
                .filter(|primitive| match &primitive.primitive {
                    egui::epaint::Primitive::Mesh(mesh) => {
                        mesh.texture_id != egui::TextureId::default() && !mesh.indices.is_empty()
                    }
                    _ => false,
                })
                .count();
            (content_height, uploaded_figures, visible_figures)
        };
        let hidden = frame(0.0);
        assert!(
            hidden.0 > 1600.0,
            "offscreen figures must retain scroll height"
        );
        assert_eq!(hidden.1, 0, "offscreen figures must not rasterize");
        assert_eq!(hidden.2, 0);
        // Settle scrollbar width before comparing the reserved and drawn layouts.
        let hidden = frame(0.0);
        let shown = frame(1200.0);
        assert!(shown.1 > 0, "scrolling must upload newly visible figures");
        assert!(shown.2 > 0, "newly visible figures must paint image meshes");
        assert!(
            (shown.0 - hidden.0).abs() < 1.0,
            "drawing must preserve reserved height"
        );
        let warm = frame(1200.0);
        assert!(warm.2 > 0, "cached figures must remain visible on repaint");
        let hidden_again = frame(0.0);
        assert_eq!(hidden_again.2, 0);
        let shown_again = frame(1200.0);
        assert!(
            shown_again.2 > 0,
            "scrolling away and back must not leave blank cards"
        );
        assert!((shown_again.0 - hidden.0).abs() < 1.0);
    }
}