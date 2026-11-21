// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

use side_view::SideView;

fn movable_mass_in_the_hold(state: &mut AppState) -> (SideView, PayloadItemsConfig) {
    let config = state.typed_config().expect("config");
    let view = side_view::build(state, &config).expect("view");
    let x = view.cabin_x[1] - 3.0;
    let z = f64::midpoint(view.hold_floor_z(x), view.hold_ceiling_z(x));
    let mut items = PayloadItemsConfig::default();
    let index = placement::place_point_mass(&mut items, &view, x, z).unwrap();
    items.point_masses[index].mass_kg = 3_000.0;
    placement::set_point_mass_movable(&mut items.point_masses[index], &view, true);
    (view, items)
}

#[test]
fn the_balance_target_moves_a_movable_item_and_the_view_reports_the_outcome() {
    let mut state = AppState::default();
    let (view, items) = movable_mass_in_the_hold(&mut state);
    assert!(
        view.zero_fuel_cg.is_some(),
        "the mass estimate gives the operating empty mass"
    );
    write_items(&mut state, &items);
    crate::views::mission_stage_editor::set_layout_settings(&mut state, false, Some(40.0));
    let config = state.typed_config().unwrap();
    let after = side_view::build(&state, &config).expect("after");
    let balance = after
        .balance
        .expect("a movable item with a mass estimate balances");
    assert!(balance.settled || balance.saturated);
    let balanced = after
        .items
        .iter()
        .find(|item| item.declared == "mass-1")
        .unwrap();
    assert!(
        (balanced.x - items.point_masses[0].x_m).abs() > 0.05,
        "the item moved"
    );
    let range = items.point_masses[0].x_range_m.unwrap();
    assert!(balanced.x >= range[0] - 1e-6 && balanced.x <= range[1] + 1e-6);
    // Locking the layout stops it.
    crate::views::mission_stage_editor::set_layout_settings(&mut state, true, Some(40.0));
    let config = state.typed_config().unwrap();
    let locked = side_view::build(&state, &config).expect("locked");
    assert!(locked.balance.is_none());
    // The balance moved the zero-fuel CG toward its target (the locked view is
    // the same mass estimate with the item where it was declared).
    let target = balance.target_x;
    let unbalanced_gap = (locked.zero_fuel_cg.unwrap() - target).abs();
    let balanced_gap = (after.zero_fuel_cg.unwrap() - target).abs();
    assert!(
        balanced_gap < unbalanced_gap,
        "{balanced_gap} against {unbalanced_gap}"
    );
    let unmoved = locked
        .items
        .iter()
        .find(|item| item.declared == "mass-1")
        .unwrap();
    assert!((unmoved.x - items.point_masses[0].x_m).abs() < 1e-6);
}

#[test]
fn the_side_view_draws_the_doors_of_the_geometry_the_layout_uses() {
    let state = AppState::default();
    let config = state.typed_config().unwrap();
    let view = side_view::build(&state, &config).unwrap();
    assert!(!view.doors.is_empty(), "declared doors of the aircraft");
    assert!(view
        .doors
        .iter()
        .all(|x| *x >= view.cabin_x[0] - 1.0 && *x <= view.cabin_x[1] + 1.0));
}

#[test]
fn a_saved_payload_layout_restores_items_lock_and_target() {
    let root = std::env::temp_dir().join(format!(
        "alas-payload-roundtrip-{}-{}",
        std::process::id(),
        crate::user_files::now_unix()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let mut state = AppState {
        tool_locator: alas_exec::ToolLocator::new(root.join("app"), root.join("data")),
        ..AppState::default()
    };
    state.boot_frames_remaining = 0;
    let (_, items) = movable_mass_in_the_hold(&mut state);
    write_items(&mut state, &items);
    crate::views::mission_stage_editor::set_layout_settings(&mut state, true, Some(33.0));
    let kind = crate::file_library::ObjectKind::PayloadLayout;
    assert!(state.library_save(kind, "Roundtrip", ""));
    // Close: the layout is cleared and the settings reset.
    write_items(&mut state, &PayloadItemsConfig::default());
    crate::views::mission_stage_editor::set_layout_settings(&mut state, false, None);
    assert!(read_items(&state).is_empty());
    let entry = crate::file_library::list(&state.data_dir(), kind)
        .into_iter()
        .next()
        .expect("a saved layout");
    assert!(state.library_open(&entry));
    assert_eq!(read_items(&state), items);
    assert_eq!(
        crate::views::mission_stage_editor::layout_settings(&state),
        (true, Some(33.0))
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_saved_mission_plan_restores_its_stages() {
    let root = std::env::temp_dir().join(format!(
        "alas-plan-roundtrip-{}-{}",
        std::process::id(),
        crate::user_files::now_unix()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let mut state = AppState {
        tool_locator: alas_exec::ToolLocator::new(root.join("app"), root.join("data")),
        ..AppState::default()
    };
    state.boot_frames_remaining = 0;
    let plan = alas_config::MissionPlan::raymer(
        alas_config::RaymerProfile::CommercialTransport,
        &alas_config::TemplateInputs::from_policy(&Default::default(), 35_000.0),
    );
    state.config_values["mission"]["plan"] = serde_json::json!(plan);
    let kind = crate::file_library::ObjectKind::MissionPlan;
    assert!(state.library_save(kind, "Roundtrip", ""));
    state.config_values["mission"]["plan"] = serde_json::Value::Null;
    state.config_values["mission"]
        .as_object_mut()
        .unwrap()
        .remove("plan");
    let entry = crate::file_library::list(&state.data_dir(), kind)
        .into_iter()
        .next()
        .expect("a saved plan");
    assert!(state.library_open(&entry));
    let restored: alas_config::MissionPlan =
        serde_json::from_value(state.config_values["mission"]["plan"].clone()).unwrap();
    assert_eq!(restored, plan);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_total_payload_counts_declared_point_masses_and_zone_occupants() {
    let mut state = AppState::default();
    let config = state.typed_config().expect("config");
    let before = side_view::build(&state, &config).expect("before");
    let x = before.cabin_x[1] - 1.0;
    let z = f64::midpoint(before.floor_z(x), before.ceiling_z(x));
    let mut items = PayloadItemsConfig::default();
    let index = placement::place_point_mass(&mut items, &before, x, z).unwrap();
    items.point_masses[index].mass_kg = 5_000.0;
    write_items(&mut state, &items);
    let config = state.typed_config().expect("config with item");
    let after = side_view::build(&state, &config).expect("after");
    assert!(
        (after.payload_mass - before.payload_mass - 5_000.0).abs() < 1.0,
        "{} -> {}",
        before.payload_mass,
        after.payload_mass
    );
}

#[test]
fn a_zones_occupants_count_in_the_total_payload_and_its_furnishing_in_the_oew_estimate() {
    let mut state = AppState::default();
    let config = state.typed_config().expect("config");
    let base = side_view::build(&state, &config).expect("base");
    let x = base.cabin_x[0] + 1.0;
    let z = f64::midpoint(base.floor_z(x), base.ceiling_z(x));
    let mut items = PayloadItemsConfig::default();
    let index = placement::place_zone(&mut items, &base, x, z).unwrap();
    items.zones[index].occupants = 0;
    items.zones[index].furnishing_kg_per_m2 = 0.0;
    write_items(&mut state, &items);
    let config = state.typed_config().expect("config");
    let bare = side_view::build(&state, &config).expect("bare");
    items.zones[index].occupants = 4;
    items.zones[index].furnishing_kg_per_m2 = 20.0;
    write_items(&mut state, &items);
    let config = state.typed_config().expect("config");
    let full = side_view::build(&state, &config).expect("full");
    let occupant_kg =
        config.requirements.passenger_mass_kg - config.cabin.passenger.checked_bag_mass_kg;
    assert!(
        (full.payload_mass - bare.payload_mass - 4.0 * occupant_kg).abs() < 1.0,
        "{} -> {} for 4 x {occupant_kg} kg",
        bare.payload_mass,
        full.payload_mass
    );
    assert!(
        full.oew_kg
            .zip(bare.oew_kg)
            .is_some_and(|(a, b)| a > b + 1.0),
        "furnishing in the OEW: {:?} -> {:?}",
        bare.oew_kg,
        full.oew_kg
    );
}

#[test]
fn a_custom_payload_takes_point_masses_and_zones_anywhere_in_the_fuselage_and_counts_them() {
    let mut state = AppState::default();
    let mut items = PayloadItemsConfig {
        custom_only: true,
        ..PayloadItemsConfig::default()
    };
    write_items(&mut state, &items);
    let config = state.typed_config().expect("config");
    let empty =
        side_view::build(&state, &config).expect("an empty custom payload still has a view");
    assert!(empty.payload_mass.abs() < 1e-6, "{}", empty.payload_mass);
    let x = f64::midpoint(empty.cabin_x[0], empty.cabin_x[1]);
    let z = f64::midpoint(empty.floor_z(x), empty.ceiling_z(x));
    let zone = placement::place_zone(&mut items, &empty, x, z).expect("a zone in the cabin");
    items.zones[zone].occupants = 2;
    let index = placement::place_point_mass(&mut items, &empty, x + 6.0, z).expect("a point mass");
    items.point_masses[index].mass_kg = 5_000.0;
    write_items(&mut state, &items);
    let config = state.typed_config().expect("config");
    let placed = side_view::build(&state, &config).expect("placed");
    assert!(placed.payload_mass > 5_000.0, "{}", placed.payload_mass);
    // A point mass in the belly or the nose is also accepted.
    let [x0, _] = empty.x_range();
    let z_nose = f64::midpoint(
        empty.at(&empty.top, x0 + 1.0),
        empty.at(&empty.bottom, x0 + 1.0),
    );
    assert!(placement::place_point_mass(&mut items, &empty, x0 + 1.0, z_nose).is_ok());
    // Front view: inside the section, and just outside its skin (pulled onto it).
    assert!(placement::place_point_mass_at(&mut items, &empty, x, 0.0, z).is_ok());
    let half_width = empty.at(&empty.widths, x) / 2.0;
    assert!(placement::place_point_mass_at(&mut items, &empty, x, half_width + 0.2, z).is_ok());
}
