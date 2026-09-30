// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::inputs_mtow::*;
use crate::state::AppState;
use crate::views::show_inputs_view;
use alas_config::{DesignMode, MtowSizing};
use egui::{Context, Pos2, RawInput, Rect};
use serde_json::json;

/// Every text run the Inputs page paints, after two frames so measured sizes
/// settle.
fn page_text(state: &mut AppState) -> Vec<String> {
    let ctx = Context::default();
    let mut text = Vec::new();
    for _ in 0..2 {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 4000.0))),
            ..RawInput::default()
        };
        let output = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| show_inputs_view(state, ui));
        });
        text = output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Text(shape) => Some(shape.galley.text().to_owned()),
                _ => None,
            })
            .collect();
    }
    text
}

fn has(text: &[String], needle: &str) -> bool {
    text.iter().any(|line| line.contains(needle))
}

fn optimizing_state() -> AppState {
    let mut state = AppState::default();
    state.set_design_mode(DesignMode::ReferenceAdaptation);
    state.run_options.optimize = true;
    state
}

fn set_mode(state: &mut AppState, wire: &str) {
    set_objective(state, "mtow_sizing", json!(wire), "Takeoff mass sizing");
}

#[test]
fn the_card_follows_the_requirements_card() {
    let mut state = optimizing_state();
    assert!(optimize_active(&state));
    let text = page_text(&mut state);
    let requirements = text
        .iter()
        .position(|line| line.contains("TLAR / service requirements"))
        .expect("requirements card");
    let card = text
        .iter()
        .position(|line| line.contains("Maximum take-off mass"))
        .expect("MTOW card");
    assert!(card > requirements);
    assert!(has(&text, "MTOW mode"));
}

#[test]
fn the_declared_mtow_is_rendered_once_with_the_optimizer_on_or_off() {
    for optimize in [true, false] {
        let mut state = optimizing_state();
        if !optimize {
            state.set_design_mode(DesignMode::BaselineSandbox);
        }
        assert_eq!(optimize_active(&state), optimize);
        let text = page_text(&mut state);
        let count = text
            .iter()
            .filter(|line| line.contains("Max take-off weight (MTOW)"))
            .count();
        assert_eq!(count, 1, "optimize={optimize}");
    }
}

#[test]
fn with_the_optimizer_off_the_mode_is_hidden_and_read_as_the_hard_limit() {
    let mut state = optimizing_state();
    set_mode(&mut state, "mtow_band");
    state.set_design_mode(DesignMode::BaselineSandbox);
    assert!(!optimize_active(&state));
    let text = page_text(&mut state);
    assert!(has(&text, "Available when Optimize design space is on."));
    assert!(!has(&text, "MTOW mode"));
    assert!(!has(&text, "Allowed variation"));
    assert_eq!(stored_mode(&state), MtowSizing::MtowBand);
}

#[test]
fn each_choice_writes_its_wire_string_and_the_config_still_parses() {
    let mut state = optimizing_state();
    for (mode, wire) in [
        (MtowSizing::FixedRequirement, "fixed_requirement"),
        (MtowSizing::MtowBand, "mtow_band"),
        (MtowSizing::PayloadAdjusted, "payload_adjusted"),
    ] {
        set_mode(&mut state, mode.as_str());
        assert_eq!(
            state.config_values["optimizer"]["objective"]["mtow_sizing"],
            json!(wire)
        );
        assert_eq!(stored_mode(&state), mode);
        let config = state.typed_config().expect("edited config parses");
        assert_eq!(config.optimizer.objective.mtow_sizing, mode);
    }
}

#[test]
fn band_controls_write_target_fraction_and_range() {
    let mut state = optimizing_state();
    set_mode(&mut state, "mtow_band");
    set_objective(&mut state, "mtow_target_kg", json!(70_000.0), "MTOW target");
    set_objective(
        &mut state,
        "mtow_band_fraction",
        json!(0.08),
        "MTOW band fraction",
    );
    set_objective(
        &mut state,
        "design_range_nmi",
        json!(2_500.0),
        "Design range",
    );
    let config = state.typed_config().expect("edited config parses");
    assert_eq!(config.optimizer.objective.mtow_target_kg, 70_000.0);
    assert_eq!(config.optimizer.objective.mtow_band_fraction, 0.08);
    assert_eq!(config.optimizer.objective.design_range_nmi, 2_500.0);
    let text = page_text(&mut state);
    assert!(has(&text, "Allowed variation"));
    assert!(has(&text, "Run a baseline analysis to check the band."));
}

#[test]
fn payload_adjusted_states_the_closure_and_that_the_mtow_is_only_a_seed() {
    let mut state = optimizing_state();
    set_mode(&mut state, "payload_adjusted");
    let text = page_text(&mut state);
    assert!(has(&text, "fuel reserves of the fuel policy"));
    assert!(has(&text, "not a limit in this mode"));
}

#[test]
fn legacy_modes_display_read_only_labels() {
    for (wire, label) in [
        ("sized_by_mission", "Sized by mission (legacy)"),
        ("unconstrained", "Unconstrained (calibration)"),
    ] {
        let mut state = optimizing_state();
        set_mode(&mut state, wire);
        let text = page_text(&mut state);
        assert!(has(&text, label), "{label}");
    }
}

#[test]
fn a_config_without_the_new_keys_reads_as_the_legacy_default_and_parses() {
    let mut state = optimizing_state();
    set_mode(&mut state, "mtow_band");
    if let Some(objective) = state
        .config_values
        .pointer_mut("/optimizer/objective")
        .and_then(serde_json::Value::as_object_mut)
    {
        for key in [
            "mtow_sizing",
            "mtow_target_kg",
            "mtow_band_fraction",
            "design_range_nmi",
        ] {
            objective.remove(key);
        }
    }
    assert_eq!(stored_mode(&state), MtowSizing::SizedByMission);
    let config = state.typed_config().expect("an older document still loads");
    assert_eq!(
        config.optimizer.objective.mtow_sizing,
        MtowSizing::SizedByMission
    );
    let text = page_text(&mut state);
    assert!(has(&text, "Sized by mission (default)"));
    assert!(has(
        &text,
        "Default mode: take-off mass closed by the mission"
    ));
    assert!(!has(&text, "came from a loaded configuration"));
}

#[test]
fn a_stored_sized_by_mission_key_reads_as_loaded() {
    let mut state = optimizing_state();
    set_mode(&mut state, "sized_by_mission");
    let text = page_text(&mut state);
    assert!(has(&text, "came from a loaded configuration"));
}
