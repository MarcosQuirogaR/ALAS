// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Headless layout tests for the Study and Advanced tabs: a preview strip
//! sized to the section's own aspect ratio, single full-width cards, and
//! control groups that share a row only when each keeps a readable width.

use super::advanced::show_advanced_tab;
use super::layout::{group_column_count, preview_height, GROUP_MIN_WIDTH};
use super::study::show_study_tab;
use super::widgets::{coefficient_value, count_value, pair_columns, physical_value};
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

/// The widest panel painted with the extreme background and the 4 pt painter
/// radius: the outline strip (cards share the fill in Light, but use a 12 pt
/// radius).
fn preview_rect(output: &FullOutput, fill: egui::Color32) -> Rect {
    output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect)
                if rect.fill == fill
                    && rect.rounding == egui::Rounding::same(4.0)
                    && rect.rect.height() >= 60.0 =>
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
}

/// The preview box follows the section's thickness-to-chord extent between a
/// legible floor and a bounded ceiling, so a thin section gets a short strip
/// instead of a tall mostly empty rectangle.
#[test]
fn preview_box_tracks_the_section_aspect_ratio_within_bounds() {
    let width = 800.0_f32;
    let aspect = 0.14_f32;
    let height = preview_height(width, aspect);
    let drawn = (width - 20.0) * aspect + 20.0;
    assert!((height - drawn).abs() < 0.001, "{height} vs {drawn}");
    assert!(height < 0.24 * width, "thin sections stay short: {height}");
    assert_eq!(preview_height(200.0, 0.02), 84.0);
    assert_eq!(preview_height(4000.0, 0.5), 200.0);
}

/// Counted quantities are integers and small physical quantities keep
/// scientific notation, so a nonzero cell volume never reads as an exact zero.
#[test]
fn numeric_formatting_keeps_counts_integral_and_small_values_visible() {
    assert_eq!(count_value(182_588), "182\u{202f}588");
    assert_eq!(count_value(0), "0");
    assert_eq!(physical_value(3.1e-12), "3.1000e-12");
    assert_eq!(physical_value(0.0), "0");
    assert_eq!(coefficient_value(0.000119), "1.190e-4");
    assert_eq!(coefficient_value(0.016014), "0.01601");
    assert!(pair_columns(300.0) == 1 && pair_columns(1900.0) == 3);
}

#[test]
fn study_tab_puts_the_preview_on_top_of_one_single_column_card() {
    for language in ["en", "es"] {
        use_language(language);
        for theme in THEMES {
            for width in [NARROW, WIDE, 1400.0] {
                let (output, viewport, fill) = render(theme, width, show_study_tab);
                let database = text_rect(&output, &tr("Database airfoil"));
                let strip = preview_rect(&output, fill);
                let routine = text_rect(&output, &tr("Routine study controls"));
                let sweep = text_rect(&output, &tr("Sequential AoA / Reynolds sweep"));
                assert!(
                    database.bottom() <= strip.top(),
                    "{language} {theme:?} {width} {database:?} {strip:?}"
                );
                assert!(
                    strip.width() >= 0.8 * viewport.width(),
                    "{language} {theme:?} {width}"
                );
                assert!(
                    strip.bottom() <= routine.top(),
                    "{language} {theme:?} {width}"
                );
                assert!(
                    routine.bottom() <= sweep.top(),
                    "{language} {theme:?} {width}"
                );
                for rect in [database, routine, sweep] {
                    assert!(
                        rect.right() <= viewport.right(),
                        "{language} {theme:?} {width}"
                    );
                }
            }
        }
    }
    use_language("en");
}

#[test]
fn routine_groups_share_a_row_only_when_the_card_is_wide() {
    for language in ["en", "es"] {
        use_language(language);
        let (wide, _, _) = render(AppTheme::Dark, WIDE, show_study_tab);
        let operating = text_rect(&wide, &tr("Operating point"));
        let boundary = text_rect(&wide, &tr("Boundary conditions"));
        let effective = text_rect(&wide, &tr("Effective condition"));
        assert!(same_row(operating, boundary), "{language} wide");
        assert!(
            same_row(boundary, effective) || stacked(operating, effective),
            "{language} wide"
        );
        let (narrow, _, _) = render(AppTheme::Light, NARROW, show_study_tab);
        let operating = text_rect(&narrow, &tr("Operating point"));
        let boundary = text_rect(&narrow, &tr("Boundary conditions"));
        assert!(stacked(operating, boundary), "{language} narrow");
    }
    use_language("en");
}

#[test]
fn advanced_tab_groups_mesh_and_solver_controls_by_purpose() {
    let first_row = ["Mesh resolution", "Domain extents [c]", "Prism layers"];
    let second_row = [
        "Solver safeguards",
        "Operational Limits",
        "Final convection scheme",
    ];
    for language in ["en", "es"] {
        use_language(language);
        for theme in THEMES {
            let (wide, viewport, _) = render(theme, WIDE, show_advanced_tab);
            for headings in [first_row, second_row] {
                let rects = headings.map(|heading| text_rect(&wide, &tr(heading)));
                assert!(same_row(rects[0], rects[1]), "{language} {theme:?} wide");
                assert!(same_row(rects[1], rects[2]), "{language} {theme:?} wide");
                assert!(
                    rects[2].right() <= viewport.right(),
                    "{language} {theme:?} wide"
                );
            }
            let setup = text_rect(&wide, &tr("Mesh and solver settings"));
            let effective = text_rect(&wide, &tr("Effective configuration"));
            assert!(setup.bottom() <= effective.top());
            let (narrow, _, _) = render(theme, NARROW, show_advanced_tab);
            for headings in [first_row, second_row] {
                let rects = headings.map(|heading| text_rect(&narrow, &tr(heading)));
                assert!(stacked(rects[0], rects[1]), "{language} {theme:?} narrow");
                assert!(stacked(rects[1], rects[2]), "{language} {theme:?} narrow");
            }
        }
    }
    use_language("en");
}

/// The effective-configuration table must use the card width rather than
/// hugging the left edge: on a wide window it packs several label/value pairs
/// into one row.
#[test]
fn effective_configuration_table_fills_the_card_width() {
    use_language("en");
    let (wide, viewport, _) = render(AppTheme::Dark, 1400.0, show_advanced_tab);
    let airfoil = text_rect(&wide, &tr("Airfoil"));
    let template = text_rect(&wide, &tr("Template"));
    assert!(
        same_row(airfoil, template),
        "{airfoil:?} {template:?} should pack into one row"
    );
    assert!(template.right() <= viewport.right());
}

/// Activate a language for a layout assertion.  Registering the Spanish
/// catalog is what makes the `es` dimension of these tests real: without it
/// `tr` returns the English key and the comparison is vacuous.
fn use_language(language: &str) {
    if language == "es" {
        alas_i18n::es::install();
    }
    alas_i18n::set_language(Some(language));
}

/// The pressure-relative-tolerance contract this tab presents as "requested
/// versus effective".
///
/// The policy itself belongs to `alas-cfd`; what is pinned here is the
/// consumer's three assumptions, because the Advanced tab shows a different
/// badge for each: automatic follows the mesh preset, an explicit override does
/// not, and a study archived before the field became optional still loads as
/// explicit rather than silently becoming automatic.
#[test]
fn pressure_relative_tolerance_distinguishes_automatic_from_explicit() {
    use alas_cfd::{MeshPreset, SolverSettings};
    let automatic = SolverSettings::default();
    assert_eq!(automatic.pressure_relative_tolerance, None);
    assert_eq!(
        automatic.effective_pressure_relative_tolerance(MeshPreset::Coarse),
        0.05
    );
    assert_eq!(
        automatic.effective_pressure_relative_tolerance(MeshPreset::Medium),
        0.05
    );
    assert_eq!(
        automatic.effective_pressure_relative_tolerance(MeshPreset::Fine),
        0.01
    );

    let explicit = SolverSettings {
        pressure_relative_tolerance: Some(0.02),
        ..SolverSettings::default()
    };
    for preset in [MeshPreset::Coarse, MeshPreset::Medium, MeshPreset::Fine] {
        assert_eq!(
            explicit.effective_pressure_relative_tolerance(preset),
            0.02,
            "an override must not follow the preset"
        );
    }

    // A study written before the field became optional carries a literal
    // number; one written without it at all is automatic.
    let older: SolverSettings =
        serde_json::from_str(r#"{"pressure_relative_tolerance": 0.05}"#).expect("older study");
    assert_eq!(
        older.pressure_relative_tolerance,
        Some(0.05),
        "an imported explicit value must stay explicit"
    );
    let unset: SolverSettings = serde_json::from_str("{}").expect("study without the field");
    assert_eq!(unset.pressure_relative_tolerance, None);
}
