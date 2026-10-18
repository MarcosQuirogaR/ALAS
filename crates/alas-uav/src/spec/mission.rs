// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mission specification: segments, decisions and the typed outputs a score
//! expression may read.
//!
//! One segment vocabulary serves the four supported competition families
//! (ACC, XtraChallenge, SAE Aero Design, AIAA DBF). Hand launch is not
//! simulated in v1 (decision D2) but `SegmentKind::Launch` keeps the interface
//! so it can be added without a schema change.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Surface the ground roll runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    /// Short grass.
    Grass,
    /// Asphalt or concrete.
    Hard,
}

/// What ends a segment.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "until", rename_all = "snake_case")]
pub enum Until {
    /// After a time, s.
    Time {
        /// Duration, s.
        seconds: f64,
    },
    /// After a ground-projected distance, m.
    Distance {
        /// Distance, m.
        meters: f64,
    },
    /// On reaching an altitude above the field, m.
    Altitude {
        /// Altitude, m.
        meters: f64,
    },
    /// After a number of laps.
    Laps {
        /// Lap count.
        count: u32,
    },
}

/// A segment of the mission.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SegmentKind {
    /// Loading and unloading on the ground (time is a score input).
    Loading {
        /// Nominal loading plus unloading time, s.
        seconds: f64,
    },
    /// Ground roll and rotation to a safe height.
    Takeoff {
        /// Runway length available, m (the announced distance when a decision).
        runway_m: f64,
        /// Surface.
        surface: Surface,
    },
    /// Climb (altitude is often traded for speed in ACC).
    Climb {
        /// End condition.
        end: Until,
    },
    /// Powered level flight, free path (ACC distance, XC cruise).
    Cruise {
        /// End condition.
        end: Until,
        /// Allowed altitude window above the field, m.
        altitude_window_m: [f64; 2],
    },
    /// Closed-circuit laps with turns (DBF, SAE, XC mast circuit).
    Laps {
        /// Lap length, m.
        lap_length_m: f64,
        /// Turn 180 degrees at each end of the straight legs.
        turn_gate: bool,
        /// Required laps.
        count: u32,
    },
    /// Unpowered glide.
    Glide {
        /// End condition.
        end: Until,
    },
    /// Landing in a zone.
    Landing {
        /// Zone length, m, if the competition defines one.
        zone_m: Option<f64>,
    },
    /// Payload release or delivery (SAE Advanced).
    Delivery,
    /// Hand, catapult or winch launch. Interface only: not simulated in v1.
    Launch {
        /// Free-text method.
        method: String,
    },
}

/// A named segment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    /// Name referenced by score terms and reports.
    pub name: String,
    /// Content.
    pub kind: SegmentKind,
}

/// Reference to a preset with its version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresetRef {
    /// Preset id (for example `acc-2026`).
    pub id: String,
    /// Preset data version.
    pub version: u32,
    /// Selected score variant id, if the preset defines variants.
    pub variant: Option<String>,
}

/// A numeric mission or course parameter in SI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamValue {
    /// Value in `unit_si`.
    pub value: f64,
    /// SI unit symbol.
    pub unit_si: String,
}

/// Decisions that are part of the score (plan section 4.3).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Decisions {
    /// Announced take-off distance, m (ACC 40/60, XC 20/40/60).
    pub announced_takeoff_m: Option<f64>,
    /// Payload count carried (cans, bottles, bags).
    pub payload_count: Option<u32>,
    /// Declared payload prediction for prediction bonuses.
    pub declared_payload_count: Option<u32>,
    /// Required probability that the announced take-off holds, (0, 1).
    pub takeoff_confidence: Option<f64>,
}

/// The mission of a design.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MissionSpec {
    /// Selected preset.
    pub preset: PresetRef,
    /// Course and rule parameters overriding the preset defaults.
    pub parameters: BTreeMap<String, ParamValue>,
    /// Score decisions.
    pub decisions: Decisions,
    /// Segments (a preset template, editable).
    pub segments: Vec<Segment>,
    /// Number of scored rounds assumed.
    pub rounds: u32,
}

/// A typed quantity the mission simulator exposes to score and rule
/// expressions. Names are stable.
#[derive(Debug, Clone, Copy)]
pub struct OutputInfo {
    /// Stable name used in expressions.
    pub name: &'static str,
    /// SI unit.
    pub unit_si: &'static str,
    /// Meaning.
    pub doc: &'static str,
}

/// All typed outputs (SI).
#[rustfmt::skip]
pub const OUTPUTS: &[OutputInfo] = &[
    OutputInfo { name: "payload_mass", unit_si: "kg", doc: "Payload mass carried" },
    OutputInfo { name: "payload_count", unit_si: "1", doc: "Cans, bottles or bags carried" },
    OutputInfo { name: "payload_volume", unit_si: "m3", doc: "Liquid payload volume" },
    OutputInfo { name: "distance_120s", unit_si: "m", doc: "Ground-projected distance in the 120 s window" },
    OutputInfo { name: "distance_90s", unit_si: "m", doc: "Ground-projected distance in the 90 s window" },
    OutputInfo { name: "time_1km", unit_si: "s", doc: "Time over the 1 km timed cruise" },
    OutputInfo { name: "energy", unit_si: "J", doc: "Battery energy used over the scored segment" },
    OutputInfo { name: "energy_per_distance", unit_si: "J/m", doc: "Energy per ground distance" },
    OutputInfo { name: "current_integral_over_limit", unit_si: "A*s", doc: "Integral of max(0, I - limit) dt" },
    OutputInfo { name: "peak_current", unit_si: "A", doc: "Maximum battery-side current" },
    OutputInfo { name: "peak_voltage", unit_si: "V", doc: "Maximum battery voltage" },
    OutputInfo { name: "peak_power", unit_si: "W", doc: "Maximum battery-side electrical power" },
    OutputInfo { name: "takeoff_distance", unit_si: "m", doc: "Ground roll to liftoff (expected value)" },
    OutputInfo { name: "climb_time", unit_si: "s", doc: "Time of the climb segment" },
    OutputInfo { name: "altitude_at_60s", unit_si: "m", doc: "Altitude 60 s after liftoff" },
    OutputInfo { name: "glide_time", unit_si: "s", doc: "Unpowered glide duration" },
    OutputInfo { name: "lap_count", unit_si: "1", doc: "Laps completed in the flight window" },
    OutputInfo { name: "loading_time", unit_si: "s", doc: "Loading plus unloading time" },
    OutputInfo { name: "empty_mass", unit_si: "kg", doc: "Empty (no payload) mass" },
    OutputInfo { name: "mtow", unit_si: "kg", doc: "Take-off mass" },
    OutputInfo { name: "span", unit_si: "m", doc: "Wing span" },
    OutputInfo { name: "wing_area", unit_si: "m2", doc: "Wing reference area" },
    OutputInfo { name: "cost", unit_si: "EUR", doc: "Bill-of-materials cost" },
    OutputInfo { name: "battery_energy", unit_si: "J", doc: "Rated pack energy" },
    OutputInfo { name: "static_margin_min", unit_si: "1", doc: "Smallest static margin over payload states" },
    OutputInfo { name: "static_margin_max", unit_si: "1", doc: "Largest static margin over payload states" },
];

/// Look up an output by name.
pub fn output(name: &str) -> Option<&'static OutputInfo> {
    OUTPUTS.iter().find(|o| o.name == name)
}
