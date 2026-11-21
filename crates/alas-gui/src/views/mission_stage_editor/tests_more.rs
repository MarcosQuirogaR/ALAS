// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;
use alas_config::StageKind;

fn commercial() -> MissionPlan {
    template_plan(
        &AppState::default(),
        Template::Raymer(RaymerProfile::CommercialTransport),
    )
}

#[test]
fn a_template_or_the_clear_keeps_the_settings_the_payload_page_owns() {
    let mut plan = template_plan(&AppState::default(), Template::Standard);
    plan.lock_layout = true;
    plan.balance_target_pct_mac = Some(27.5);
    plan.takeoff_fuel_kg = Some(9_000.0);
    plan.contingency_fraction = 0.05;
    let raymer = template_plan(
        &AppState::default(),
        Template::Raymer(RaymerProfile::AirSuperiority),
    );
    replace_stages(&mut plan, raymer);
    assert_eq!(plan.name, "Air Superiority");
    assert!(plan.lock_layout);
    assert_eq!(plan.balance_target_pct_mac, Some(27.5));
    assert_eq!(plan.takeoff_fuel_kg, Some(9_000.0));
    assert_eq!(plan.contingency_fraction, 0.05);
    replace_stages(&mut plan, MissionPlan::default());
    assert!(!plan.has_stages());
    assert!(plan.lock_layout && plan.balance_target_pct_mac == Some(27.5));
    assert!(!plan.is_empty(), "the settings keep the node");
}

#[test]
fn agl_altitudes_follow_the_field_the_engine_measures_from() {
    let mut plan = commercial();
    let approach = plan.position("attempt-to-land").unwrap();
    if let StageKind::Approach {
        field_elevation_ft, ..
    } = &mut plan.stages[approach].kind
    {
        *field_elevation_ft = Some(2_000.0);
    }
    let go_around = plan.position("climb-after-go-around").unwrap();
    // A go-around stated above the field climbs 3 000 ft above the field the
    // approach was made to (the engine measures from there after an approach).
    plan.stages[go_around].kind = StageKind::GoAround {
        altitude_ft: 3_000.0,
        agl: true,
    };
    let segments = model::profile(&plan, 0.0, 0.0, 3_000.0);
    assert_eq!(segments[go_around].altitude_ft[1], 5_000.0);
    let divert = plan.position("divert").unwrap();
    assert!(
        segments[divert].via_ft.is_some(),
        "the diversion holds its own level"
    );
}

#[test]
fn a_diversion_is_drawn_as_a_climb_a_held_level_and_a_descent() {
    let plan = commercial();
    let segments = model::profile(&plan, 0.0, 0.0, 3_000.0);
    let divert = &segments[plan.position("divert").unwrap()];
    let corners = plot::corners(divert);
    assert_eq!(corners.len(), 4);
    assert!(corners[1].1 > corners[0].1);
    assert_eq!(corners[1].1, corners[2].1);
    assert!(corners[3].1 < corners[2].1);
    assert!(corners.windows(2).all(|pair| pair[0].0 <= pair[1].0));
}

#[test]
fn a_fuel_transfer_offers_the_tanks_of_the_aircraft() {
    let state = AppState::default();
    let ids = model::tank_ids(&state, &egui::Context::default());
    assert!(ids.len() >= 2, "{ids:?}");
    assert!(ids.iter().all(|id| !id.is_empty()));
}

#[test]
fn the_layout_settings_of_a_payload_file_round_trip_through_the_plan() {
    let mut state = AppState::default();
    set_layout_settings(&mut state, true, Some(31.0));
    let captured = capture_layout_settings(&state);
    set_layout_settings(&mut state, false, None);
    assert_eq!(layout_settings(&state), (false, None));
    store_layout_settings(&mut state.config_values, &captured);
    assert_eq!(layout_settings(&state), (true, Some(31.0)));
}

#[test]
fn a_diversion_ends_at_the_profiles_approach_height_as_the_engine_flies_it() {
    let plan = commercial();
    let divert = plan.position("divert").unwrap();
    // The engine ends the diversion at the profile's fourth descent rung (above
    // mean sea level) or the field, whichever is higher.
    let at_rung = model::profile(&plan, 0.0, 0.0, 3_000.0);
    assert_eq!(at_rung[divert].altitude_ft[1], 3_000.0);
    let high_field = model::profile(&plan, 0.0, 5_000.0, 3_000.0);
    assert_eq!(high_field[divert].altitude_ft[1], 5_000.0);
}

/// Holds the process-wide interface language for one test and restores it.
struct LanguageGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl LanguageGuard {
    fn spanish() -> Self {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let lock = LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        alas_i18n::es::install();
        alas_i18n::set_language(Some("es"));
        Self { _lock: lock }
    }
}

impl Drop for LanguageGuard {
    fn drop(&mut self) {
        alas_i18n::set_language(Some("en"));
    }
}

#[test]
fn the_summary_of_a_route_cruise_is_translated() {
    let _spanish = LanguageGuard::spanish();
    let text = model::summary(&StageKind::Cruise {
        extent: alas_config::CruiseExtent::Remainder,
        altitude_ft: None,
        agl: false,
        speed_m_s: None,
    });
    assert_ne!(
        text, "Route",
        "the summary goes through the translation boundary"
    );
}

#[test]
fn a_payload_release_stage_numbers_the_items_it_drops() {
    let mut plan = commercial();
    let cruise = plan.position("cruise-out").unwrap();
    plan.stages[cruise].events.push(alas_config::StageEvent {
        at: alas_config::EventTime::End,
        action: alas_config::EventAction::PayloadRelease {
            items: vec!["store-a".to_owned()],
        },
    });
    let item = |id: &str, releasable| release::PayloadItem {
        id: id.to_owned(),
        name: id.to_owned(),
        mass_kg: 100.0,
        releasable,
    };
    let items = [item("store-a", true), item("store-b", true)];
    let found = release::release_stages(&plan, &items);
    assert_eq!(found.get("store-a"), Some(&(cruise + 1)));
    assert_eq!(found.get("store-b"), None, "no stage drops it");

    // A release that names nothing drops every releasable item still carried.
    let landing = plan.position("attempt-to-land").unwrap();
    plan.stages[landing].events.push(alas_config::StageEvent {
        at: alas_config::EventTime::End,
        action: alas_config::EventAction::PayloadRelease { items: Vec::new() },
    });
    let items = [
        item("store-a", true),
        item("store-b", true),
        item("fixed", false),
    ];
    let found = release::release_stages(&plan, &items);
    assert_eq!(found.get("store-a"), Some(&(cruise + 1)));
    assert_eq!(found.get("store-b"), Some(&(landing + 1)));
    assert_eq!(found.get("fixed"), None, "a fixed item never leaves");
}
