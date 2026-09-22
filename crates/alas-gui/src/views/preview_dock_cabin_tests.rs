// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::{
    centered_row, controls_row_width, preview_overlay_rects, remembered_row_width,
    show_cabin_legend,
};
use crate::theme::{apply_theme, AppTheme};
use egui::{pos2, vec2, Align, Color32, Context, Frame, FullOutput, Id, Rect};

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
                    let row_id = Id::new("test_controls_row");
                    centered_row(ui, row_id, row_width, row_height, |ui| {
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

fn render_cabin_legend(theme: AppTheme, viewport: Rect, scale: f32) -> (FullOutput, Color32) {
    let ctx = Context::default();
    apply_theme(theme, &ctx);
    let panel_fill = ctx.style().visuals.panel_fill;
    let output = ctx.run(raw_input(viewport, scale), |ctx| {
        egui::CentralPanel::default()
            .frame(Frame::default())
            .show(ctx, |ui| {
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(viewport), |ui| {
                    show_cabin_legend(ui, viewport)
                });
            });
    });
    (output, panel_fill)
}

#[test]
fn cabin_legend_rows_are_centered_and_contained_after_translation() {
    alas_i18n::es::install();
    alas_i18n::set_language(Some("es"));
    for theme in [AppTheme::Light, AppTheme::Grey, AppTheme::Dark] {
        for (width, scale) in [(300.0, 1.0), (360.0, 1.5), (420.0, 2.0)] {
            let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(width, 240.0));
            let (output, panel_fill) = render_cabin_legend(theme, viewport, scale);
            let (controls_rect, legend_rect) =
                preview_overlay_rects(viewport, crate::state::PreviewTab::Cabin);
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

            // The card hugs the bottom edge of the viewport, clear of the
            // controls, with the title and both rows inside its margin.
            let card = painted_card(&output, panel_fill, first_row.union(second_row));
            let context = format!("{theme:?} width={width} scale={scale}: card={card:?}");
            assert!(
                (viewport.bottom() - card.bottom() - 8.0).abs() < 0.6,
                "{context} does not sit on the bottom margin of {viewport:?}"
            );
            assert!(
                (card.center().x - viewport.center().x).abs() < 0.6,
                "{context}"
            );
            assert!(
                card.top() >= controls_rect.bottom() + 5.0 - 0.6,
                "{context}"
            );
            assert!(card.top() >= legend_rect.top() - 0.6, "{context}");
            let title = rendered_text_rect(&output, super::tr("Cabin legend").as_ref());
            assert!(
                title.top() >= card.top() + 7.0 - 0.6,
                "{context} title={title:?}"
            );
            assert!(
                first_row.top() >= title.bottom() - 0.6,
                "{context} title={title:?}"
            );
            assert!(
                second_row.bottom() <= card.bottom() - 7.0 + 0.6,
                "{context}"
            );
        }
    }
    alas_i18n::set_language(Some("en"));
}

/// The group frame painted behind an overlay card: the widest panel-filled
/// rectangle that contains `anchor`.
fn painted_card(output: &FullOutput, panel_fill: Color32, anchor: Rect) -> Rect {
    output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect)
                if rect.fill == panel_fill && rect.rect.contains_rect(anchor) =>
            {
                Some(rect.rect)
            }
            _ => None,
        })
        .max_by(|a, b| a.width().total_cmp(&b.width()))
        .expect("missing painted card frame")
}

fn render_controls_card(
    theme: AppTheme,
    tab: crate::state::PreviewTab,
    viewport: Rect,
    scale: f32,
) -> (FullOutput, Color32) {
    let ctx = Context::default();
    apply_theme(theme, &ctx);
    let panel_fill = ctx.style().visuals.panel_fill;
    let mut state = crate::state::AppState {
        preview_tab: tab,
        ..Default::default()
    };
    let output = ctx.run(raw_input(viewport, scale), |ctx| {
        egui::CentralPanel::default()
            .frame(Frame::default())
            .show(ctx, |ui| {
                super::show_aircraft_viewer_controls(&mut state, ui, viewport, "cam", "view");
            });
    });
    (output, panel_fill)
}

#[test]
fn controls_card_is_centered_on_the_viewport_and_its_buttons_inside_the_card() {
    alas_i18n::es::install();
    for language in ["en", "es"] {
        alas_i18n::set_language(Some(language));
        for theme in [AppTheme::Light, AppTheme::Grey, AppTheme::Dark] {
            for tab in [
                crate::state::PreviewTab::Exterior,
                crate::state::PreviewTab::Cabin,
            ] {
                for (width, scale) in [(300.0, 1.0), (420.0, 1.5), (720.0, 2.0)] {
                    let viewport = Rect::from_min_size(pos2(30.0, 10.0), vec2(width, 300.0));
                    let (output, panel_fill) = render_controls_card(theme, tab, viewport, scale);
                    let labels = union_rects(&[
                        rendered_text_rect(&output, super::tr("Exterior").as_ref()),
                        rendered_text_rect(&output, super::tr("Interior").as_ref()),
                        rendered_text_rect(&output, super::tr("Reset").as_ref()),
                    ]);
                    let card = painted_card(&output, panel_fill, labels);
                    let (controls, _) = preview_overlay_rects(viewport, tab);
                    let context = format!("{language}/{theme:?}/{tab:?} {width}px x{scale}");
                    assert!(
                        (card.center().x - viewport.center().x).abs() < 0.6,
                        "{context}: card {card:?} is not centered on viewport {viewport:?}"
                    );
                    assert!(
                        (labels.center().x - card.center().x).abs() < 0.6,
                        "{context}: labels {labels:?} are not centered in card {card:?}"
                    );
                    assert!(
                        (card.top() - controls.top()).abs() < 0.6,
                        "{context}: card {card:?} does not hang from the controls region {controls:?}"
                    );
                    assert!(card.left() >= viewport.left() && card.right() <= viewport.right());
                    assert!(labels.left() > card.left() && labels.right() < card.right());
                }
            }
        }
    }
    alas_i18n::set_language(Some("en"));
}

#[test]
fn centered_row_recentres_itself_after_a_wrong_width_estimate() {
    alas_i18n::set_language(Some("en"));
    let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(360.0, 80.0));
    let ctx = Context::default();
    apply_theme(AppTheme::Light, &ctx);
    let mut offsets = Vec::new();
    for _ in 0..2 {
        let mut bounds = None;
        let _ = ctx.run(raw_input(viewport, 1.0), |ctx| {
            egui::CentralPanel::default()
                .frame(Frame::default())
                .show(ctx, |ui| {
                    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(viewport), |ui| {
                        ui.spacing_mut().item_spacing.x = 5.0;
                        let row_height = ui.spacing().interact_size.y;
                        // Deliberately underestimate the row so the first frame
                        // starts the widgets at the centre instead of around it.
                        let row_id = Id::new("test_recentre_row");
                        let row_width = remembered_row_width(ui, row_id, 1.0);
                        let inner = centered_row(ui, row_id, row_width, row_height, |ui| {
                            let exterior = ui.selectable_label(false, super::tr("Exterior"));
                            let interior = ui.selectable_label(false, super::tr("Interior"));
                            let reset = ui.add(egui::Button::new(super::tr("Reset")).small());
                            union_rects(&[exterior.rect, interior.rect, reset.rect])
                        });
                        bounds = Some(inner.inner);
                    });
                });
        });
        offsets.push(bounds.expect("row bounds").center().x - viewport.center().x);
    }
    assert!(
        offsets[0] > 20.0,
        "first frame should be visibly off-centre, offset {}",
        offsets[0]
    );
    assert!(
        offsets[1].abs() < 0.6,
        "second frame should be recentred, offset {}",
        offsets[1]
    );
}
