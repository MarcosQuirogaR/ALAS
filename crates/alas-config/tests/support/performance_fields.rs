// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Field inputs added after the frozen translated configuration was recorded.

// Each parity binary compiles only the helpers needed by its fixture.
#![allow(dead_code)]

use serde_json::{json, Value};

pub const NATIVE_FIELDS: [&str; 9] = [
    "cl_max_land_source",
    "cl_max_to_source",
    "legacy_field_correlations",
    "propeller_dry_landing_distance_share",
    "propeller_landing_deceleration_g",
    "propeller_landing_mean_drag_to_weight",
    "propeller_takeoff_inertia_distance_m",
    "propeller_takeoff_rolling_friction",
    "propeller_takeoff_stop_deceleration_g",
];

pub fn is_native_field(path: &str, key: &str) -> bool {
    NATIVE_FIELDS.contains(&key)
        && (path == "PerformanceConfig"
            || path.ends_with(".performance")
            || path.starts_with("performance."))
}

/// Preserve the frozen input while checking its replacement from published
/// landing evidence rather than copying a computed coefficient into a ledger.
/// The independent landing-reference tests verify the lift balance and source
/// conditions; these checks additionally cover registry and saved-file loading.
pub fn published_landing_change(path: &str) -> Option<(Value, Value)> {
    let (case, key) = path.split_once(".performance.")?;
    let name = match case {
        "preset_only" => "A220-300",
        "preset_then_field" => "B787-9",
        name => name,
    };
    let source = alas_config::presets::published_landing_reference(name)?;
    match key {
        "cl_max_land" => {
            let frozen = match name {
                "A220-300" | "A320-200" => 2.90,
                "A340-300" | "A380-800" | "B787-9" | "DC-10" => 2.95,
                "ATR72-600" => 2.20,
                _ => return None,
            };
            Some((json!(frozen), json!(source.cl_max_land())))
        }
        "vapp_vstall_land_factor" => Some((json!(1.30), json!(1.23))),
        _ => None,
    }
}
