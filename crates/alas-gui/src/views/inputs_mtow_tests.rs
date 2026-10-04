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
fn payload_adjusted_displays_the_effective_range_without_explanatory_prose() {
    let mut state = optimizing_state();
    set_mode(&mut state, "payload_adjusted");
    let text = page_text(&mut state);
    assert!(has(&text, "Design range"));
    assert!(!has(&text, "fuel reserves of the fuel policy"));
}

#[test]
fn automatic_range_displays_the_model_resolution_without_changing_the_setting() {
    let mut state = optimizing_state();
    set_mode(&mut state, "payload_adjusted");
    set_objective(&mut state, "design_range_nmi", json!(0.0), "Design range");
    let expected = state
        .typed_config()
        .unwrap()
        .mtow_plan()
        .design_mission
        .unwrap()
        .range
        .declared_nmi()
        .unwrap();
    assert_eq!(effective_design_range_nmi(&state), Some(expected));
    let text = page_text(&mut state);
    assert!(has(&text, &format!("{expected:.0} nmi")), "{text:?}");
    assert_eq!(
        state
            .typed_config()
            .unwrap()
            .optimizer
            .objective
            .design_range_nmi,
        0.0
    );
    set_objective(
        &mut state,
        "design_range_nmi",
        json!(1234.0),
        "Design range",
    );
    assert_eq!(effective_design_range_nmi(&state), Some(1234.0));
}

#[test]
fn automatic_range_falls_back_to_the_still_air_route() {
    let mut state = optimizing_state();
    set_mode(&mut state, "payload_adjusted");
    state.config_values["preset"] = json!("");
    state.config_values["mass_model"]["flops_transport"]["design_range_nmi"] =
        serde_json::Value::Null;
    let destination = state.config_values["arrival_airport"].clone();
    state.config_values["departure_airport"] = destination;
    assert_eq!(effective_design_range_nmi(&state), Some(0.0));
}

#[test]
fn unresolved_automatic_range_keeps_a_numeric_editor() {
    let mut state = optimizing_state();
    set_mode(&mut state, "payload_adjusted");
    state.config_values["preset"] = json!("");
    state.config_values["mass_model"]["flops_transport"]["design_range_nmi"] =
        serde_json::Value::Null;
    state.config_values["departure_airport"] = json!("unresolved-airport");
    assert_eq!(effective_design_range_nmi(&state), None);
    assert!(has(&page_text(&mut state), "0 nmi"));
    set_objective(&mut state, "design_range_nmi", json!(500.0), "Design range");
    assert_eq!(effective_design_range_nmi(&state), Some(500.0));
}

#[test]
fn payload_adjusted_auto_range_uses_the_route_instead_of_the_flops_mass_input() {
    let mut state = optimizing_state();
    set_mode(&mut state, "payload_adjusted");
    state.config_values["preset"] = json!("");
    state.config_values["mass_model"]["flops_transport"]["design_range_nmi"] = json!(7600.0);
    state.config_values["departure_airport"] = json!("LEMD");
    state.config_values["arrival_airport"] = json!("LEPA");
    let config = state.typed_config().unwrap();
    assert!(config.mtow_plan().design_mission.is_none());
    let origin = alas_config::airport_dataset::resolve("LEMD").unwrap();
    let destination = alas_config::airport_dataset::resolve("LEPA").unwrap();
    let route = alas_route::route::haversine_m(
        origin.latitude_deg.value.unwrap(),
        origin.longitude_deg.value.unwrap(),
        destination.latitude_deg.value.unwrap(),
        destination.longitude_deg.value.unwrap(),
    ) / 1852.0;
    assert_eq!(effective_design_range_nmi(&state), Some(route));
    set_objective(
        &mut state,
        "design_range_nmi",
        json!(1500.0),
        "Design range",
    );
    assert_eq!(effective_design_range_nmi(&state), Some(1500.0));
}

#[test]
fn airport_edits_invalidate_a_cached_band_without_a_new_analysis() {
    use alas_pipeline::quick_analysis::band::{BandStatus, DesignMissionBand};
    let mut state = optimizing_state();
    set_mode(&mut state, "mtow_band");
    state.config_values["preset"] = json!("");
    state.config_values["mass_model"]["flops_transport"]["design_range_nmi"] =
        serde_json::Value::Null;
    state.config_values["departure_airport"] = json!("LEMD");
    state.config_values["arrival_airport"] = json!("LEPA");
    let key = band_cache_key(&state);
    let range = effective_design_range_nmi(&state);
    let ctx = Context::default();
    let id = egui::Id::new("inputs_mtow_band_check");
    ctx.data_mut(|data| {
        data.insert_temp(
            id,
            (
                key.clone(),
                Some(DesignMissionBand {
                    range_at_lo_m: 1.0,
                    range_at_hi_m: 2.0,
                    status: BandStatus::Inside,
                    note: String::new(),
                }),
            ),
        )
    });
    state.config_values["arrival_airport"] = json!("LEMD");
    assert_ne!(effective_design_range_nmi(&state), range);
    assert_ne!(band_cache_key(&state), key);
    let _ = ctx.run(RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| show_mtow_controls(&mut state, ui));
    });
    let cached = ctx
        .data(|data| data.get_temp::<(String, Option<DesignMissionBand>)>(id))
        .unwrap();
    assert_eq!(cached.0, band_cache_key(&state));
    assert!(cached.1.is_none());
    assert!(state.pipeline_result.is_none());
}

#[test]
fn a_registered_preset_without_a_mode_matches_the_loaded_hard_default() {
    let mut state = optimizing_state();
    state.config_values["preset"] = json!("A320-200");
    state.config_values["optimizer"]["objective"]
        .as_object_mut()
        .unwrap()
        .remove("mtow_sizing");
    assert_eq!(stored_mode(&state), MtowSizing::FixedRequirement);
    assert_eq!(
        stored_mode(&state),
        state
            .typed_config()
            .unwrap()
            .optimizer
            .objective
            .mtow_sizing
    );
}

#[test]
fn mtow_controls_fit_narrow_and_typical_cards() {
    for width in [280.0, 600.0] {
        for mode in ["fixed_requirement", "mtow_band", "payload_adjusted"] {
            let mut state = optimizing_state();
            set_mode(&mut state, mode);
            let ctx = Context::default();
            for _ in 0..2 {
                let input = RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(width, 1200.0))),
                    ..RawInput::default()
                };
                let _ = ctx.run(input, |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let right = ui.max_rect().right();
                        show_mtow_card(&mut state, ui);
                        assert!(ui.min_rect().right() <= right, "{mode} at {width} points");
                    });
                });
            }
        }
    }
}

#[test]
fn translated_mtow_cards_and_mass_dialog_fit_short_viewports() {
    alas_i18n::es::install();
    let previous_language = alas_i18n::get_language();
    for language in [crate::state::Language::En, crate::state::Language::Es] {
        alas_i18n::set_language(Some(language.code()));
        for (dialog, width, height) in [
            (false, 280.0, 220.0),
            (false, 600.0, 380.0),
            (true, 560.0, 380.0),
        ] {
            for mode in ["fixed_requirement", "mtow_band", "payload_adjusted"] {
                let mut state = optimizing_state();
                state.language = language;
                state.sandbox.advanced_tab = "mass_advanced".to_owned();
                set_mode(&mut state, mode);
                let ctx = Context::default();
                for _ in 0..2 {
                    let input = RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            egui::vec2(width, height),
                        )),
                        ..RawInput::default()
                    };
                    let output = ctx.run(input, |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let bounds = ui.max_rect();
                            if dialog {
                                crate::sandbox::advanced::show_advanced_settings_contents(
                                    &mut state, ui,
                                );
                            } else {
                                egui::ScrollArea::vertical()
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| show_mtow_card(&mut state, ui));
                            }
                            assert!(
                                ui.min_rect().right() <= bounds.right(),
                                "{mode}, {}: horizontal overflow",
                                language.code()
                            );
                            assert!(
                                ui.min_rect().bottom() <= bounds.bottom(),
                                "{mode}, {}: vertical overflow",
                                language.code()
                            );
                        });
                    });
                    let mut visible_text = 0;
                    for shape in &output.shapes {
                        if let egui::Shape::Text(text) = &shape.shape {
                            let rect = Rect::from_min_size(text.pos, text.galley.size());
                            if shape.clip_rect.intersects(rect) {
                                visible_text += 1;
                                assert!(
                                    rect.left() >= shape.clip_rect.left()
                                        && rect.right() <= shape.clip_rect.right(),
                                    "clipped text {:?}, mode {mode}, {}: {rect:?} in {:?}",
                                    text.galley.text(),
                                    language.code(),
                                    shape.clip_rect
                                );
                            }
                        }
                    }
                    assert!(visible_text > 0);
                }
            }
        }
    }
    alas_i18n::set_language(Some(&previous_language));
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
    state.config_values["preset"] = json!("");
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
    assert!(!has(&text, "came from a loaded configuration"));
}

#[test]
fn a_stored_sized_by_mission_key_reads_as_loaded() {
    let mut state = optimizing_state();
    set_mode(&mut state, "sized_by_mission");
    let text = page_text(&mut state);
    assert!(has(&text, "Sized by mission (legacy)"));
}
