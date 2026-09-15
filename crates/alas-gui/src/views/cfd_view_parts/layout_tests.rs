// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Headless layout tests for the Study and Advanced tabs: preview strip on
//! top, one single-column card, and control groups that share a row only
//! when each keeps a readable width.

use super::advanced::show_advanced_tab;
use super::layout::{group_column_count, preview_height, GROUP_MIN_WIDTH};
use super::study::show_study_tab;
use crate::state::AppState;
use crate::theme::{apply_theme, AppTheme};
use crate::views::tr;
use egui::{pos2, vec2, Context, Frame, FullOutput, Rect, Ui};

const THEMES: [AppTheme; 3] = [AppTheme::Dark, AppTheme::Light, AppTheme::Grey];
const NARROW: f32 = 620.0;
const WIDE: f32 = 1040.0;

fn render(
    theme: AppTheme,
    width: f32,
    tab: fn(&mut AppState, &mut Ui),
) -> (FullOutput, Rect, egui::Color32) {
    let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(width, 1400.0));
    let mut state = AppState::default();
    let ctx = Context::default();
    apply_theme(theme, &ctx);
    let mut output = None;
    for _ in 0..2 {
        let raw = egui::RawInput {
            screen_rect: Some(viewport),
            ..Default::default()
        };
        output = Some(ctx.run(raw, |ctx| {
            egui::CentralPanel::default()
                .frame(Frame::default())
                .show(ctx, |ui| tab(&mut state, ui));
        }));
    }
    let fill = ctx.style().visuals.extreme_bg_color;
    (output.expect("two frames"), viewport, fill)
}

fn text_rect(output: &FullOutput, label: &str) -> Rect {
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

/// The widest tall panel painted with the extreme background and the 4 pt
/// painter radius: the outline strip (cards share the fill in Light, but
/// use a 12 pt radius).
fn preview_rect(output: &FullOutput, fill: egui::Color32) -> Rect {
    output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect)
                if rect.fill == fill
                    && rect.rounding == egui::Rounding::same(4.0)
                    && rect.rect.height() >= 90.0 =>
            {
                Some(rect.rect)
            }
            _ => None,
        })
        .max_by(|a, b| a.width().total_cmp(&b.width()))
        .expect("painted preview strip")
}

fn same_row(a: Rect, b: Rect) -> bool {
    (a.top() - b.top()).abs() < 0.5 && a.right() <= b.left()
}

fn stacked(a: Rect, b: Rect) -> bool {
    (a.left() - b.left()).abs() < 0.5 && a.bottom() <= b.top()
}

#[test]
fn group_columns_only_appear_when_each_group_keeps_a_readable_width() {
    assert_eq!(group_column_count(GROUP_MIN_WIDTH - 1.0, 3), 1);
    assert_eq!(group_column_count(2.0 * GROUP_MIN_WIDTH + 12.0, 3), 2);
    assert_eq!(group_column_count(3.0 * GROUP_MIN_WIDTH + 24.0, 3), 3);
    assert_eq!(group_column_count(2000.0, 2), 2);
    assert_eq!(group_column_count(2000.0, 0), 1);
    assert_eq!(preview_height(400.0), 120.0);
    assert_eq!(preview_height(1000.0), 220.0);
}

#[test]
fn study_tab_puts_the_preview_on_top_of_one_single_column_card() {
    for language in ["en", "es"] {
        alas_i18n::set_language(Some(language));
        for theme in THEMES {
            for width in [NARROW, WIDE, 1400.0] {
                let (output, viewport, fill) = render(theme, width, show_study_tab);
                let outline = text_rect(&output, &tr("Resolved outline"));
                let strip = preview_rect(&output, fill);
                let database = text_rect(&output, &tr("Database airfoil"));
                let routine = text_rect(&output, &tr("Routine study controls"));
                let sweep = text_rect(&output, &tr("Sequential AoA / Reynolds sweep"));
                assert!(
                    outline.bottom() <= strip.top(),
                    "{language} {theme:?} {width} {outline:?} {strip:?}"
                );
                assert!(
                    strip.width() >= 0.8 * viewport.width(),
                    "{language} {theme:?} {width}"
                );
                assert!(
                    strip.bottom() <= database.top(),
                    "{language} {theme:?} {width}"
                );
                assert!(stacked(database, routine), "{language} {theme:?} {width}");
                assert!(
                    routine.bottom() <= sweep.top(),
                    "{language} {theme:?} {width}"
                );
                for rect in [outline, database, routine, sweep] {
                    assert!(
                        rect.right() <= viewport.right(),
                        "{language} {theme:?} {width}"
                    );
                }
            }
        }
    }
    alas_i18n::set_language(Some("en"));
}

#[test]
fn routine_groups_share_a_row_only_when_the_card_is_wide() {
    for language in ["en", "es"] {
        alas_i18n::set_language(Some(language));
        let (wide, _, _) = render(AppTheme::Dark, WIDE, show_study_tab);
        let operating = text_rect(&wide, &tr("Operating point"));
        let boundary = text_rect(&wide, &tr("Boundary conditions"));
        assert!(same_row(operating, boundary), "{language} wide");
        let (narrow, _, _) = render(AppTheme::Light, NARROW, show_study_tab);
        let operating = text_rect(&narrow, &tr("Operating point"));
        let boundary = text_rect(&narrow, &tr("Boundary conditions"));
        assert!(stacked(operating, boundary), "{language} narrow");
    }
    alas_i18n::set_language(Some("en"));
}

#[test]
fn advanced_tab_groups_mesh_and_solver_controls_by_purpose() {
    let mesh = ["Mesh resolution", "Domain extents [c]", "Prism layers"];
    let solver = [
        "Solver safeguards",
        "Operational Limits",
        "Final convection scheme",
    ];
    for language in ["en", "es"] {
        alas_i18n::set_language(Some(language));
        for theme in THEMES {
            let (wide, viewport, _) = render(theme, WIDE, show_advanced_tab);
            for headings in [mesh, solver] {
                let rects = headings.map(|heading| text_rect(&wide, &tr(heading)));
                assert!(same_row(rects[0], rects[1]), "{language} {theme:?} wide");
                assert!(same_row(rects[1], rects[2]), "{language} {theme:?} wide");
                assert!(
                    rects[2].right() <= viewport.right(),
                    "{language} {theme:?} wide"
                );
            }
            let mesh_title = text_rect(&wide, &tr("Mesh settings"));
            let solver_title = text_rect(&wide, &tr("Solver and resource settings"));
            let effective = text_rect(&wide, &tr("Effective configuration"));
            assert!(mesh_title.bottom() <= solver_title.top());
            assert!(solver_title.bottom() <= effective.top());
            let (narrow, _, _) = render(theme, NARROW, show_advanced_tab);
            for headings in [mesh, solver] {
                let rects = headings.map(|heading| text_rect(&narrow, &tr(heading)));
                assert!(stacked(rects[0], rects[1]), "{language} {theme:?} narrow");
                assert!(stacked(rects[1], rects[2]), "{language} {theme:?} narrow");
            }
        }
    }
    alas_i18n::set_language(Some("en"));
}
