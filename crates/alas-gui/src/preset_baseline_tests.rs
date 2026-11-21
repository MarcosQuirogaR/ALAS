// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A freshly loaded preset must show no "Modified" marker anywhere: every
//! form page and the Aircraft cards are drawn for each registered preset with
//! all cards open, and no modified dot may be painted.

use crate::state::AppState;

/// The modified dots painted in `output`: filled circles of the marker's size.
fn painted_modified(output: &egui::FullOutput) -> usize {
    fn count(shape: &egui::Shape) -> usize {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().map(count).sum(),
            egui::Shape::Circle(circle) => usize::from(
                (circle.radius - crate::views::form::MODIFIED_DOT * 0.5).abs() < 0.01
                    && circle.fill != egui::Color32::TRANSPARENT,
            ),
            _ => 0,
        }
    }
    output
        .shapes
        .iter()
        .map(|clipped| count(&clipped.shape))
        .sum()
}

fn frame(
    ctx: &egui::Context,
    state: &mut AppState,
    draw: impl FnOnce(&mut AppState, &mut egui::Ui),
) -> egui::FullOutput {
    let mut draw = Some(draw);
    ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1400.0, 30000.0),
            )),
            ..Default::default()
        },
        |ctx| {
            state.publish_form_baseline();
            egui::CentralPanel::default().show(ctx, |ui| {
                if let Some(draw) = draw.take() {
                    draw(state, ui);
                }
            });
        },
    )
}

#[test]
fn a_freshly_loaded_preset_paints_no_modified_badge_on_any_page() {
    crate::perf_tour::set_force_open_for_test(true);
    let mut failures = Vec::new();
    for name in alas_config::presets::available() {
        let mut state = AppState::default();
        state.finish_walkthrough();
        state.load_preset(name);
        state.begin_frame();
        let ctx = egui::Context::default();
        crate::theme::apply_theme(crate::theme::AppTheme::Dark, &ctx);
        crate::detail::publish(&ctx, crate::detail::Detail::Advanced);
        for page in crate::nav::all_pages().filter(|page| page.group.is_some()) {
            let mut output = None;
            for _ in 0..2 {
                output = Some(frame(&ctx, &mut state, |state, ui| {
                    crate::views::form_page::show_form_page(state, ui, page);
                }));
            }
            let count = output.as_ref().map_or(0, painted_modified);
            if count > 0 {
                failures.push(format!("{name} / page {}: {count}", page.id));
            }
        }
        let mut output = None;
        for _ in 0..2 {
            output = Some(frame(&ctx, &mut state, |state, ui| {
                crate::views::inputs_view::show_aircraft_body(state, ui);
            }));
        }
        let count = output.as_ref().map_or(0, painted_modified);
        if count > 0 {
            failures.push(format!("{name} / Aircraft page: {count}"));
        }
    }
    crate::perf_tour::set_force_open_for_test(false);
    assert!(
        failures.is_empty(),
        "Modified right after load:\n{}",
        failures.join("\n")
    );
}

/// Control: the detector does see a badge when a value really is edited.
#[test]
fn an_edited_aircraft_field_is_painted_modified() {
    crate::perf_tour::set_force_open_for_test(true);
    let mut state = AppState::default();
    state.finish_walkthrough();
    state.load_preset("A320-200");
    state.begin_frame();
    state.config_values["requirements"]["cruise_mach"] = serde_json::json!(0.61);
    let ctx = egui::Context::default();
    let mut output = None;
    for _ in 0..2 {
        output = Some(frame(&ctx, &mut state, |state, ui| {
            crate::views::inputs_view::show_aircraft_body(state, ui);
        }));
    }
    crate::perf_tour::set_force_open_for_test(false);
    assert!(output.as_ref().map_or(0, painted_modified) > 0);
}
