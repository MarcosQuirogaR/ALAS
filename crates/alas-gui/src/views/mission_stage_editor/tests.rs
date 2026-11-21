// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;
use alas_config::{PlanError, StageKind};
use egui::{pos2, Rect};

fn state_with(template: Template) -> AppState {
    let mut state = AppState::default();
    let plan = template_plan(&state, template);
    model::write_plan(&mut state, &plan);
    state
}

fn render(state: &mut AppState, frames: usize) -> Vec<String> {
    fn collect(shape: &egui::Shape, out: &mut Vec<String>) {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, out)),
            egui::Shape::Text(text) => out.push(text.galley.text().to_owned()),
            _ => {}
        }
    }
    let ctx = egui::Context::default();
    let mut texts = Vec::new();
    for _ in 0..frames {
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1_200.0, 1_600.0))),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    show(state, ui);
                });
            },
        );
        texts.clear();
        for clipped in &output.shapes {
            collect(&clipped.shape, &mut texts);
        }
    }
    texts
}

#[test]
fn the_plan_round_trips_through_the_configuration_and_empty_removes_it() {
    let mut state = state_with(Template::Standard);
    let plan = model::read_plan(&state);
    assert!(plan.has_stages());
    assert!(state
        .config_values
        .pointer("/mission/plan/stages")
        .is_some());
    assert!(
        !model::write_plan(&mut state, &plan),
        "unchanged is no edit"
    );
    assert!(model::write_plan(&mut state, &MissionPlan::default()));
    assert!(state.config_values.pointer("/mission/plan").is_none());
    assert!(!model::read_plan(&state).has_stages());
}

#[test]
fn every_template_is_a_valid_plan_for_the_default_case() {
    let state = AppState::default();
    let templates = std::iter::once(Template::Standard)
        .chain(RaymerProfile::ALL.into_iter().map(Template::Raymer));
    for template in templates {
        let plan = template_plan(&state, template);
        assert!(plan.has_stages(), "{}", template.title());
        plan.validate()
            .unwrap_or_else(|error| panic!("{}: {error}", template.title()));
    }
}

#[test]
fn a_template_edit_writes_a_typed_config_the_pipeline_accepts() {
    let state = state_with(Template::Raymer(RaymerProfile::CommercialTransport));
    let config = state.typed_config().expect("typed config");
    assert!(config.mission.plan.has_stages());
    assert!(
        alas_config::validate(&config)
            .iter()
            .all(|issue| issue.field_path != "mission.plan"),
        "{:?}",
        alas_config::validate(&config)
    );
}

#[test]
fn a_refused_order_reports_the_rule_and_leaves_the_plan_alone() {
    let mut plan = template_plan(&AppState::default(), Template::Standard);
    let before = plan.clone();
    let result = plan.move_stage(0, 2);
    assert!(matches!(result, Err(PlanError::TakeoffMustComeFirst(_))));
    assert_eq!(plan, before);
    let last = plan.stages.len() - 1;
    let error = plan.move_stage(last, 0).unwrap_err();
    assert!(error.to_string().starts_with("Takeoff Must Come First"));
}

#[test]
fn a_drop_below_the_pointer_lands_in_the_slot_under_it() {
    let rows: Vec<Rect> = (0..5)
        .map(|i| Rect::from_min_size(pos2(0.0, i as f32 * 20.0), vec2(100.0, 18.0)))
        .collect();
    // Released over the fourth row (centre 69): stage 1 becomes index 3.
    assert_eq!(drop_target(&rows, 1, 70.0), 3);
    // Released above everything: index 0.
    assert_eq!(drop_target(&rows, 3, -5.0), 0);
    // Released below everything: last.
    assert_eq!(drop_target(&rows, 0, 500.0), 4);
    // Released in place: unchanged.
    assert_eq!(drop_target(&rows, 2, 50.0), 2);
}

#[test]
fn the_stage_list_shows_every_stage_and_its_event_rows() {
    let mut state = state_with(Template::Raymer(RaymerProfile::LowLevelStrike));
    let texts = render(&mut state, 2);
    for stage in model::read_plan(&state).stages {
        let label = tr_label(stage.kind.label());
        assert!(
            texts.iter().any(|text| text.contains(&label)),
            "{label} missing in {texts:?}"
        );
    }
    // The plot numbers each stage's segment like the list does.
    assert!(texts.iter().any(|text| text == "1"));
    assert!(texts.iter().any(|text| *text == tr_label("Add Stage")));
    assert!(texts.iter().any(|text| *text == tr_label("Clear Stages")));
}

#[test]
fn an_empty_plan_offers_templates_and_adding_but_nothing_to_clear() {
    let mut state = AppState::default();
    let texts = render(&mut state, 2);
    assert!(texts.iter().any(|text| *text == tr_label("Templates")));
    assert!(texts.iter().any(|text| *text == tr_label("Add Stage")));
    assert!(texts.iter().all(|text| *text != tr_label("Clear Stages")));
    assert!(!model::read_plan(&state).has_stages());
}

#[test]
fn the_page_has_one_mission_profile_card_with_the_route_row_inside() {
    let mut state = AppState::default();
    let texts = render(&mut state, 2);
    assert_eq!(
        texts
            .iter()
            .filter(|text| text.eq_ignore_ascii_case("Mission Profile"))
            .count(),
        1,
        "{texts:?}"
    );
    assert!(texts.iter().any(|text| text == "Route"), "{texts:?}");
    assert!(
        texts.iter().any(|text| text.starts_with("1 \u{2192} 2  ")),
        "the standard mission's stages are listed: {texts:?}"
    );
}

#[test]
fn adding_a_stage_before_the_landing_keeps_the_plan_valid() {
    let mut plan = template_plan(&AppState::default(), Template::Standard);
    let mut editor = EditorUi::default();
    add_stage(&mut plan, &mut editor, "Loiter", 35_000.0);
    assert!(plan.validate().is_ok());
    assert!(matches!(
        plan.stages.last().map(|stage| &stage.kind),
        Some(StageKind::Landing { .. })
    ));
    assert_eq!(editor.selected.as_deref(), Some("loiter"));
}
