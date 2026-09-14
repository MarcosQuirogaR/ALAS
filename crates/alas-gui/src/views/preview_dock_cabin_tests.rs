// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::{centered_row, controls_row_width, preview_overlay_rects, show_cabin_legend};
use crate::theme::{apply_theme, AppTheme};
use egui::{pos2, vec2, Align, Color32, Context, Frame, FullOutput, Rect};

fn raw_input(viewport: Rect, scale: f32) -> egui::RawInput {
    let mut raw_input = egui::RawInput {
        screen_rect: Some(viewport),
        ..Default::default()
    };
    raw_input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .expect("root viewport input")
        .native_pixels_per_point = Some(scale);
    raw_input
}

fn rendered_text_rect(output: &FullOutput, label: &str) -> Rect {
    output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.job.text == label => {
                Some(Rect::from_min_size(text.pos, text.galley.size()))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing rendered text: {label}"))
}

fn union_rects(rects: &[Rect]) -> Rect {
    rects.iter().copied().fold(Rect::NOTHING, |rect, next| {
        if rect == Rect::NOTHING {
            next
        } else {
            rect.union(next)
        }
    })
}

fn render_control_row(
    theme: AppTheme,
    viewport: Rect,
    scale: f32,
) -> (FullOutput, Rect, [Rect; 3], Align) {
    let ctx = Context::default();
    apply_theme(theme, &ctx);
    let mut captured = None;
    let output = ctx.run(raw_input(viewport, scale), |ctx| {
        egui::CentralPanel::default()
            .frame(Frame::default())
            .show(ctx, |ui| {
                let inner = ui.allocate_new_ui(egui::UiBuilder::new().max_rect(viewport), |ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    let row_width = controls_row_width(ui);
                    let row_height = ui.spacing().interact_size.y;
                    centered_row(ui, row_width, row_height, |ui| {
                        let exterior = ui.selectable_label(false, super::tr("Exterior"));
                        let interior = ui.selectable_label(false, super::tr("Interior"));
                        let reset = ui.add(egui::Button::new(super::tr("Reset")).small());
                        (
                            [exterior.rect, interior.rect, reset.rect],
                            ui.layout().horizontal_align(),
                        )
                    })
                });
                captured = Some((inner.inner.response.rect, inner.inner.inner));
            });
    });
    let (row_rect, (widget_rects, text_align)) = captured.expect("control row geometry");
    (output, row_rect, widget_rects, text_align)
}

#[test]
fn cabin_controls_and_labels_stay_centered_across_viewports_scales_languages_and_themes() {
    alas_i18n::es::install();
    for language in ["en", "es"] {
        alas_i18n::set_language(Some(language));
        for theme in [AppTheme::Light, AppTheme::Grey, AppTheme::Dark] {
            for (width, scale) in [(300.0, 1.0), (420.0, 1.5), (720.0, 2.0)] {
                let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(width, 80.0));
                let (output, row_rect, widget_rects, text_align) =
                    render_control_row(theme, viewport, scale);
                let bounds = union_rects(&widget_rects);
                assert!(
                    (bounds.center().x - viewport.center().x).abs() < 0.6,
                    "{language}/{theme:?} {width}px row is at x={} in viewport center {}",
                    bounds.center().x,
                    viewport.center().x
                );
                assert_eq!(text_align, Align::Center);
                assert!((row_rect.center().x - viewport.center().x).abs() < 0.6);
                assert_eq!(output.pixels_per_point, scale);

                for (label, widget_rect) in [
                    (super::tr("Exterior"), widget_rects[0]),
                    (super::tr("Interior"), widget_rects[1]),
                    (super::tr("Reset"), widget_rects[2]),
                ] {
                    let text_rect = rendered_text_rect(&output, label.as_ref());
                    assert!(
                        (text_rect.center().x - widget_rect.center().x).abs() < 0.6,
                        "{language}/{theme:?} {width}px label {label:?} is not centered"
                    );
                }
            }
        }
    }
    alas_i18n::set_language(Some("en"));
}

fn render_cabin_legend(theme: AppTheme, viewport: Rect, scale: f32) -> FullOutput {
    let ctx = Context::default();
    apply_theme(theme, &ctx);
    ctx.run(raw_input(viewport, scale), |ctx| {
        egui::CentralPanel::default()
            .frame(Frame::default())
            .show(ctx, |ui| {
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(viewport), |ui| {
                    show_cabin_legend(ui, viewport)
                });
            });
    })
}

#[test]
fn cabin_legend_rows_are_centered_and_contained_after_translation() {
    alas_i18n::es::install();
    alas_i18n::set_language(Some("es"));
    for theme in [AppTheme::Light, AppTheme::Grey, AppTheme::Dark] {
        for (width, scale) in [(300.0, 1.0), (420.0, 2.0)] {
            let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(width, 240.0));
            let output = render_cabin_legend(theme, viewport, scale);
            let (_, legend_rect) = preview_overlay_rects(viewport, crate::state::PreviewTab::Cabin);
            let colors = [
                Color32::from_rgb(142, 68, 173),
                Color32::from_rgb(41, 128, 185),
                Color32::from_rgb(39, 174, 96),
                Color32::from_rgb(230, 126, 34),
                Color32::from_rgb(93, 173, 226),
                Color32::from_rgb(231, 76, 60),
            ];
            let labels = [
                super::tr("First class"),
                super::tr("Business class"),
                super::tr("Economy class"),
                super::tr("Galley"),
                super::tr("Lavatory"),
                super::tr("Exit"),
            ];
            let item_rects: Vec<Rect> = colors
                .iter()
                .zip(labels.iter())
                .map(|(color, label)| {
                    let swatch = output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            egui::Shape::Rect(rect)
                                if rect.fill == *color && rect.rect.width() == 10.0 =>
                            {
                                Some(rect.rect)
                            }
                            _ => None,
                        })
                        .unwrap_or_else(|| panic!("missing legend swatch: {label}"));
                    swatch.union(rendered_text_rect(&output, label.as_ref()))
                })
                .collect();
            let first_row = union_rects(&item_rects[..3]);
            let second_row = union_rects(&item_rects[3..]);
            for row in [first_row, second_row] {
                assert!((row.center().x - viewport.center().x).abs() < 0.6);
                assert!(row.left() >= legend_rect.left() - 0.6);
                assert!(row.right() <= legend_rect.right() + 0.6);
            }
            assert!(second_row.top() > first_row.bottom());
            assert!(
                second_row.bottom() <= legend_rect.bottom() + 0.6,
                "{theme:?} width={width} scale={scale}: first={first_row:?}, second={second_row:?}, legend={legend_rect:?}"
            );
            assert_eq!(output.pixels_per_point, scale);
        }
    }
    alas_i18n::set_language(Some("en"));
}
