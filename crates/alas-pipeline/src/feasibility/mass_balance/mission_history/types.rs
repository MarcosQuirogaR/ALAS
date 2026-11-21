// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The published shape of the mission mass history.

use alas_config::EventTime;

/// One instant of the flown plan: leg end or event.
#[derive(Debug, Clone, PartialEq)]
pub struct MissionCgPoint {
    /// What the instant is (a stage and its end, or an event).
    pub label: String,
    /// The stage it belongs to.
    pub stage: Option<usize>,
    /// Flight time, s.
    pub time_s: f64,
    /// Distance flown, m.
    pub range_m: f64,
    /// Aircraft mass from the ledger, kg.
    pub mass_kg: f64,
    /// Centre of gravity, geometry frame, m.
    pub cg_m: [f64; 3],
    /// Longitudinal centre of gravity, percent MAC of the envelope's frame.
    pub cg_pct_mac: f64,
    /// Forward limit of the envelope at this mass, percent MAC.
    pub forward_limit_pct_mac: f64,
    /// Aft limit of the envelope at this mass, percent MAC.
    pub aft_limit_pct_mac: f64,
    /// Whether the centre of gravity is between the limits.
    pub within_envelope: bool,
    /// Usable fuel on board, kg.
    pub fuel_kg: f64,
    /// Fuel in the trim tanks, kg.
    pub trim_fuel_kg: f64,
    /// Fuel in each tank, kg.
    pub tank_fuel_kg: Vec<(String, f64)>,
}

/// One event with the aircraft before and after it.
#[derive(Debug, Clone, PartialEq)]
pub struct MissionEventRecord {
    /// The stage it belongs to.
    pub stage: usize,
    /// Start or end of the stage.
    pub at: EventTime,
    /// What happened, with the numbers.
    pub description: String,
    /// Mass before, kg.
    pub mass_before_kg: f64,
    /// Mass after, kg.
    pub mass_after_kg: f64,
    /// Centre of gravity before, percent MAC.
    pub cg_before_pct_mac: f64,
    /// Centre of gravity after, percent MAC.
    pub cg_after_pct_mac: f64,
    /// Whether the state after the event is inside the envelope.
    pub within_envelope: bool,
}

/// One stage of the flown plan.
#[derive(Debug, Clone, PartialEq)]
pub struct MissionStageResult {
    /// The stage id.
    pub id: String,
    /// The kind of stage.
    pub label: &'static str,
    /// Fuel burned in its legs, kg.
    pub fuel_kg: f64,
    /// Time flown, s.
    pub time_s: f64,
    /// Distance flown, m.
    pub distance_m: f64,
    /// Mass at its first leg's start, kg.
    pub mass_start_kg: f64,
    /// Mass at its last leg's end, kg.
    pub mass_end_kg: f64,
    /// Centre of gravity at its end, percent MAC.
    pub cg_end_pct_mac: f64,
    /// Most forward and most aft centre of gravity in the stage, percent MAC.
    pub cg_range_pct_mac: [f64; 2],
    /// Whether every instant of the stage is inside the envelope.
    pub within_envelope: bool,
    /// The critical-field result, for a take-off stage that names a field.
    pub field: Option<crate::feasibility::StageFieldCheck>,
}

/// The mass and centre-of-gravity history of a flown plan.
#[derive(Debug, Clone, PartialEq)]
pub struct MissionHistory {
    /// Every leg end and event, in flight order.
    pub points: Vec<MissionCgPoint>,
    /// The events, in flight order.
    pub events: Vec<MissionEventRecord>,
    /// The stages, in flight order.
    pub stages: Vec<MissionStageResult>,
    /// Operations that could not be carried out in full (a transfer into a
    /// full tank, fuel received with no room), one line each.
    pub notes: Vec<String>,
    /// The longitudinal centre of gravity at the start and the end of each
    /// flown leg, m (geometry axes), in the order of the mission's segments.
    pub leg_cg_x_m: Vec<[f64; 2]>,
    /// Largest difference between the flown mass and the ledger's, kg.
    pub max_mass_mismatch_kg: f64,
}
