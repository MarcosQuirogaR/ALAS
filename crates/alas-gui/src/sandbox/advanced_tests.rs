// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Bilingual settings geometry checks and optional paint-output evidence.

use crate::state::{AppState, Language};
use alas_config::ConfigNode;
use egui::{Context, Event, Pos2, RawInput, Rect};

use crate::test_svg as svg;

fn frame(ctx: &Context, state: &mut AppState, size: [f32; 2], events: Vec<Event>) -> svg::Capture {
    let viewport = Rect::from_min_size(Pos2::ZERO, size.into());
    let mut content = Rect::NOTHING;
    let mut used = Rect::NOTHING;
    let output = ctx.run(
        RawInput {
            screen_rect: Some(viewport),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                content = ui.max_rect();
                super::show_advanced_settings_contents(state, ui);
                used = ui.min_rect();
            });
        },
    );
    let mut editable = Vec::new();
    string_values(&state.config_values, &mut editable);
    svg::assert_bounds(&output.shapes, &editable);
    assert!(
        content.contains_rect(used),
        "settings overflow: {} {} {used:?} in {content:?}",
        state.sandbox.advanced_tab,
        state.language.code()
    );
    svg::Capture {
        output,
        content,
        viewport,
        editable,
    }
}

/// Every string in the configuration: what a text box may hold.
fn string_values(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) => out.push(text.clone()),
        serde_json::Value::Array(items) => items.iter().for_each(|item| string_values(item, out)),
        serde_json::Value::Object(map) => map.values().for_each(|item| string_values(item, out)),
        _ => {}
    }
}

/// The A320-200 preset in reference adaptation with optimization on and the
/// objective with the longest label, showing `tab`.
fn preset_state(language: Language, tab: &str) -> AppState {
    let mut state = AppState {
        language,
        ..Default::default()
    };
    let config = alas_config::AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"}))
        .expect("registered preset");
    state.config_values = serde_json::to_value(config).expect("preset configuration");
    state.set_design_mode(alas_config::DesignMode::ReferenceAdaptation);
    state.run_options.optimize = true;
    state.config_values["optimizer"]["objective"]["kind"] =
        serde_json::json!("fuel_per_seat_kilometre");
    state.sandbox.advanced_tab = tab.to_owned();
    state
}

fn context() -> Context {
    let ctx = Context::default();
    crate::theme::apply_theme(crate::theme::AppTheme::Dark, &ctx);
    ctx.style_mut(|style| style.animation_time = 0.0);
    ctx
}

fn evidence_directory(export: bool) -> std::path::PathBuf {
    let directory = std::env::temp_dir().join("alas-shots");
    if export {
        std::fs::create_dir_all(&directory).expect("settings evidence directory");
    }
    directory
}

/// Every Advanced Settings form tab, top to bottom, in both languages and
/// both window sizes: the label-row and editor changes apply to all of them.
fn exercise_all_form_tabs(export: bool) {
    alas_i18n::es::install();
    let previous = alas_i18n::get_language();
    let directory = evidence_directory(export);
    for language in [Language::En, Language::Es] {
        alas_i18n::set_language(Some(language.code()));
        for page in crate::nav::ADVANCED_SETTINGS_PAGES
            .iter()
            .filter(|page| page.group.is_some())
        {
            for (case, size) in [
                ("narrow_short", [560.0, 380.0]),
                ("default", [760.0, 560.0]),
            ] {
                let mut state = preset_state(language, page.id);
                let ctx = context();
                frame(&ctx, &mut state, size, Vec::new());
                let top = frame(&ctx, &mut state, size, Vec::new());
                let title = crate::views::tr(page.title);
                assert!(
                    svg::text_rect(&top.output.shapes, &title).is_some(),
                    "{} title",
                    page.id
                );
                if export {
                    svg::write(
                        &directory.join(format!(
                            "settings_all_{}_{}_{case}.svg",
                            page.id,
                            language.code()
                        )),
                        &top,
                        &title,
                    );
                }
                for _ in 0..30 {
                    frame(
                        &ctx,
                        &mut state,
                        size,
                        vec![
                            Event::PointerMoved(Pos2::new(size[0] * 0.5, size[1] * 0.75)),
                            Event::MouseWheel {
                                unit: egui::MouseWheelUnit::Point,
                                delta: egui::vec2(0.0, -300.0),
                                modifiers: egui::Modifiers::NONE,
                            },
                        ],
                    );
                }
            }
        }
    }
    alas_i18n::set_language(Some(&previous));
}

/// Top edge of each Objective-group label on the Optimizer tab, keyed by
/// field name, in one language.
fn objective_label_rows(language: Language) -> Vec<(&'static str, f32)> {
    alas_i18n::set_language(Some(language.code()));
    let mut state = preset_state(language, "optimizer");
    let ctx = context();
    let size = [760.0, 1400.0];
    frame(&ctx, &mut state, size, Vec::new());
    let top = frame(&ctx, &mut state, size, Vec::new());
    // Fields the Inputs card owns are not on this page and are not found.
    let schema = alas_config::ObjectiveConfig::default().schema();
    schema
        .fields
        .iter()
        .filter(|field| matches!(field.entry, alas_config::Entry::Leaf(_)))
        .filter_map(|field| {
            let label = alas_i18n::t(Some(field.label), Some(language.code())).into_owned();
            // The last match: the "Objective" group header paints the same
            // word before the field label does.
            let rect = top.output.shapes.iter().rev().find_map(|shape| {
                let egui::Shape::Text(text) = &shape.shape else {
                    return None;
                };
                (text.galley.text() == label)
                    .then(|| text.galley.rect.translate(text.pos.to_vec2()))
            })?;
            Some((field.name, rect.top()))
        })
        .collect()
}

#[test]
fn optimizer_rows_line_up_identically_in_both_languages() {
    alas_i18n::es::install();
    let previous = alas_i18n::get_language();
    let english = objective_label_rows(Language::En);
    let spanish = objective_label_rows(Language::Es);
    alas_i18n::set_language(Some(&previous));
    assert!(english.len() >= 4, "objective labels found: {english:?}");
    assert_eq!(
        english.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        spanish.iter().map(|(name, _)| *name).collect::<Vec<_>>()
    );
    for ((name, y_en), (_, y_es)) in english.iter().zip(&spanish) {
        assert!(
            (y_en - y_es).abs() < 0.5,
            "{name} sits at y = {y_en} in English and {y_es} in Spanish"
        );
    }
    // Labels of one row share a top edge; a taller cell would offset the
    // fields placed after it.
    let mut rows: Vec<f32> = english.iter().map(|(_, y)| *y).collect();
    rows.sort_by(f32::total_cmp);
    rows.dedup_by(|a, b| (*a - *b).abs() < 0.5);
    assert!(
        rows.len() < english.len(),
        "two columns share rows: {english:?}"
    );
}

#[test]
fn the_tab_strip_wraps_into_rows_and_pages_whole_tabs_around_the_selection() {
    use super::{paged_window, wrapped_rows};
    let widths = [100.0, 120.0, 80.0, 140.0, 90.0];
    assert_eq!(wrapped_rows(&widths, 10.0, 1000.0), 1);
    assert_eq!(wrapped_rows(&widths, 10.0, 240.0), 3);
    for selected in 0..widths.len() {
        for first in 0..widths.len() {
            let (start, end) = paged_window(&widths, 10.0, 250.0, first, selected);
            assert!(
                start <= selected && selected < end,
                "{selected} in {start}..{end}"
            );
            let span: f32 =
                widths[start..end].iter().sum::<f32>() + 10.0 * (end - start - 1) as f32;
            assert!(
                span <= 250.0 || end - start == 1,
                "{start}..{end} spans {span}"
            );
            // No further whole tab fits on either side.
            if end < widths.len() {
                assert!(span + 10.0 + widths[end] > 250.0);
            }
            if start > 0 {
                assert!(span + 10.0 + widths[start - 1] > 250.0);
            }
        }
    }
}

/// The tab strip takes one row of a short window and every tab label in it
/// is whole; the default window wraps it into at most two rows.
#[test]
fn the_tab_strip_takes_one_row_of_a_narrow_window_in_both_languages() {
    alas_i18n::es::install();
    let previous = alas_i18n::get_language();
    for language in [Language::En, Language::Es] {
        alas_i18n::set_language(Some(language.code()));
        for (size, max_rows) in [
            ([560.0, 380.0], 1),
            ([576.0, 384.0], 1),
            ([760.0, 560.0], 2),
        ] {
            let mut state = preset_state(language, "control_surfaces");
            let ctx = context();
            frame(&ctx, &mut state, size, Vec::new());
            let capture = frame(&ctx, &mut state, size, Vec::new());
            let pages = super::pages();
            let titles: Vec<String> = pages
                .iter()
                .map(|page| crate::views::tr(page.title))
                .chain(std::iter::once(crate::views::tr("Run options")))
                .collect();
            let mut rows: Vec<f32> = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for shape in &capture.output.shapes {
                let egui::Shape::Text(text) = &shape.shape else {
                    continue;
                };
                // The strip paints before the page, so a title's first
                // occurrence is its tab; the active page repeats its title as
                // the heading.
                if !titles.iter().any(|title| title == text.galley.text())
                    || !seen.insert(text.galley.text().to_owned())
                {
                    continue;
                }
                let bounds = text.galley.rect.translate(text.pos.to_vec2());
                assert!(
                    shape.clip_rect.contains_rect(bounds),
                    "cut tab label {}",
                    text.galley.text()
                );
                if !rows.iter().any(|row| (row - bounds.top()).abs() < 1.0) {
                    rows.push(bounds.top());
                }
            }
            assert!(
                !rows.is_empty() && rows.len() <= max_rows,
                "{} {size:?}: tab rows at {rows:?}",
                language.code()
            );
        }
    }
    alas_i18n::set_language(Some(&previous));
}

#[test]
fn every_form_tab_fits_narrow_short_and_default_viewports_in_both_languages() {
    exercise_all_form_tabs(false);
}

#[test]
#[ignore = "exports bilingual settings paint output for every form tab"]
fn export_every_form_tab_layout() {
    exercise_all_form_tabs(true);
}

fn exercise(export: bool) {
    alas_i18n::es::install();
    let previous = alas_i18n::get_language();
    let directory = std::env::temp_dir().join("alas-shots");
    if export {
        std::fs::create_dir_all(&directory).expect("settings evidence directory");
    }
    for language in [Language::En, Language::Es] {
        alas_i18n::set_language(Some(language.code()));
        for tab in ["optimizer", "mass_advanced"] {
            for (case, size) in [
                ("narrow_short", [560.0, 380.0]),
                ("default", [760.0, 560.0]),
            ] {
                let mut state = preset_state(language, tab);
                let ctx = context();
                frame(&ctx, &mut state, size, Vec::new());
                let top = frame(&ctx, &mut state, size, Vec::new());
                let title = crate::views::tr(if tab == "optimizer" {
                    "Optimizer"
                } else {
                    "Mass"
                });
                assert!(svg::text_rect(&top.output.shapes, &title).is_some());
                let text = svg::all_text(&top.output.shapes);
                for removed in ["Constraint policy", "Soft penalty weight"] {
                    assert!(!text.contains(&crate::views::tr(removed)));
                }
                if tab == "mass_advanced" {
                    assert!(
                        text.contains(&crate::views::tr("Hard MTOW constraint (preset default)"))
                    );
                }
                if export {
                    svg::write(
                        &directory.join(format!("settings_{tab}_{}_{case}.svg", language.code())),
                        &top,
                        &title,
                    );
                }
                let mut opened = std::collections::HashSet::new();
                let mut seen = svg::all_text(&top.output.shapes);
                let mut last = top;
                for step in 0..40 {
                    let header = last.output.shapes.iter().find_map(|shape| {
                        let egui::Shape::Text(text) = &shape.shape else {
                            return None;
                        };
                        let label = text.galley.text();
                        let bounds = text.galley.rect.translate(text.pos.to_vec2());
                        let group = ["Screening stage", "Refinement stage"]
                            .iter()
                            .any(|name| label == crate::views::tr(name));
                        let advanced = label.starts_with(
                            crate::views::tr("Advanced ({count})")
                                .split('{')
                                .next()
                                .unwrap_or_default(),
                        );
                        ((group || advanced)
                            && !opened.contains(label)
                            && shape.clip_rect.contains_rect(bounds))
                        .then(|| (label.to_owned(), bounds.center()))
                    });
                    if tab == "optimizer" {
                        if let Some((label, position)) = header {
                            opened.insert(label);
                            for pressed in [true, false] {
                                last = frame(
                                    &ctx,
                                    &mut state,
                                    size,
                                    vec![
                                        Event::PointerMoved(position),
                                        Event::PointerButton {
                                            pos: position,
                                            button: egui::PointerButton::Primary,
                                            pressed,
                                            modifiers: egui::Modifiers::NONE,
                                        },
                                    ],
                                );
                            }
                            if export {
                                svg::write(
                                    &directory.join(format!(
                                        "settings_{tab}_{}_{case}_expanded_{step}.svg",
                                        language.code()
                                    )),
                                    &last,
                                    &title,
                                );
                            }
                            seen.push_str(&svg::all_text(&last.output.shapes));
                            continue;
                        }
                    }
                    let bottom = frame(
                        &ctx,
                        &mut state,
                        size,
                        vec![
                            Event::PointerMoved(Pos2::new(size[0] * 0.5, size[1] * 0.75)),
                            Event::MouseWheel {
                                unit: egui::MouseWheelUnit::Point,
                                delta: egui::vec2(0.0, -80.0),
                                modifiers: egui::Modifiers::NONE,
                            },
                        ],
                    );
                    if export && step == 39 {
                        svg::write(
                            &directory.join(format!(
                                "settings_{tab}_{}_{case}_bottom.svg",
                                language.code()
                            )),
                            &bottom,
                            &title,
                        );
                    }
                    last = bottom;
                    seen.push_str(&svg::all_text(&last.output.shapes));
                    let workers = crate::views::tr("Native compute worker threads");
                    if export
                        && svg::text_rect(&last.output.shapes, &workers)
                            .is_some_and(|rect| last.content.contains_rect(rect))
                    {
                        svg::write(
                            &directory.join(format!(
                                "settings_{tab}_{}_{case}_workers.svg",
                                language.code()
                            )),
                            &last,
                            &title,
                        );
                    }
                }
                if tab == "optimizer" {
                    let automatic = crate::views::tr_fields(
                        "Automatic (all {count} threads)",
                        &[(
                            "count",
                            alas_config::SolverSettings::default()
                                .resolved_workers()
                                .to_string(),
                        )],
                    );
                    assert!(
                        seen.contains(&automatic),
                        "resolved worker value is visible: {automatic}"
                    );
                    let objective = crate::views::tr(
                        alas_pipeline::optimizer_summary::objective::objective_label(
                            alas_config::ObjectiveKind::FuelPerSeatKilometre,
                        ),
                    );
                    assert!(
                        seen.contains(&objective),
                        "precise objective is visible: {objective}"
                    );
                    for group in ["Screening stage", "Refinement stage"] {
                        assert!(
                            opened.contains(&crate::views::tr(group)),
                            "{group} must be expanded: {opened:?}"
                        );
                    }
                    assert!(
                        seen.contains(&crate::views::tr("Native compute worker threads")),
                        "worker control is exercised"
                    );
                    for label in ["Evaluation ceiling", "Time limit [s]"] {
                        assert!(
                            seen.contains(&crate::views::tr(label)),
                            "{label} is exercised"
                        );
                    }
                }
            }
        }
    }
    alas_i18n::set_language(Some(&previous));
}

#[test]
fn bilingual_settings_fit_narrow_short_and_default_viewports() {
    exercise(false);
}

#[test]
#[ignore = "exports bilingual settings paint output"]
fn export_bilingual_settings_layout() {
    exercise(true);
}
