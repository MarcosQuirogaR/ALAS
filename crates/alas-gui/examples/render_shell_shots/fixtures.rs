// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Case fixtures of the 07b shots: a mission plan and declared payload items.

use alas_gui::AppState;

/// A Raymer commercial transport plan with an aerial refuel after the cruise.
pub fn mission_plan(state: &mut AppState) {
    let mut plan = alas_config::MissionPlan::raymer(
        alas_config::RaymerProfile::CommercialTransport,
        &alas_config::TemplateInputs::from_policy(&Default::default(), 35_000.0),
    );
    plan.stages[2].events.push(alas_config::StageEvent {
        at: alas_config::EventTime::End,
        action: alas_config::EventAction::AerialRefuel { mass_kg: 2_000.0 },
    });
    state.config_values["mission"]["plan"] = serde_json::json!(plan);
    state.on_config_modified();
}

/// An office zone, a releasable drone in the hold and a movable rack.
pub fn payload_items(state: &mut AppState) {
    state.config_values["cabin"]["payload_items"] = serde_json::json!({
        "zones": [{"name": "Office", "kind": "office", "x_start_m": 12.0, "x_end_m": 16.0, "occupants": 4}],
        "point_masses": [
            {"id": "drone", "name": "Drone", "x_m": 24.0, "z_m": -0.6, "mass_kg": 1000.0, "releasable": true},
            {"id": "rack", "name": "Rack", "x_m": 9.0, "z_m": 0.5, "mass_kg": 300.0, "movable": true, "x_range_m": [6.0, 11.0]}
        ]
    });
    state.on_config_modified();
}
