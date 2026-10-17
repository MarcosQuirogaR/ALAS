// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Field inputs added after the frozen translated configuration was recorded.

// Each parity binary compiles only the helpers needed by its fixture.
#![allow(dead_code)]

use serde_json::{json, Value};

pub const NATIVE_FIELDS: [&str; 19] = [
    "improved_climb_v2",
    "geometry_scaled_clmax",
    "cruise_thrust_from_deck",
    "cl_max_land_source",
    "engine_match_tolerance",
    "go_around_spool_thrust_fraction",
    "landing_flap_delta_cd",
    "landing_gear_delta_cd",
    "takeoff_gear_delta_cd",
    "v2_min_vsr_factor",
    "wet_landing_runway",
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

/// The frozen landing-distance coefficient of one loaded case and its
/// replacement. The frozen presets used 0.63 for the widebody class and 0.60
/// elsewhere; the product uses one value calibrated on certified landing field
/// lengths (see `PerformanceConfig::default`).
pub fn k_land_change(case: &str) -> (Value, Value) {
    let frozen = match case {
        "A340-300" | "A380-800" | "B787-9" | "DC-10" | "AVE" | "preset_then_field" => 0.63,
        _ => 0.60,
    };
    (json!(frozen), json!(0.505))
}

/// Preserve the frozen input while checking its replacement from published
/// landing evidence rather than copying a computed coefficient into a ledger.
/// The independent landing-reference tests verify the lift balance and source
/// conditions; these checks additionally cover registry and saved-file loading.
pub fn published_landing_change(path: &str) -> Option<(Value, Value)> {
    let (case, key) = path.split_once(".performance.")?;
    if key == "k_land" {
        return Some(k_land_change(case));
    }
    if let Some(change) = configuration_drag_change(case, key) {
        return Some(change);
    }
    let name = match case {
        "preset_only" => "A220-300",
        "preset_then_field" => "B787-9",
        name => name,
    };
    // Take-off CLmax from the declared high-lift system (fix lane CLmax-TO,
    // 2026-10-16; Raymer 5th ed. Eq. 12.21 / Table 12.2): replaces the frozen
    // 2.10 class default for every jet with a declared system.
    if key == "cl_max_to" {
        let preset = alas_config::presets::get(name).ok()?;
        let parts = alas_config::presets::preset_breakdown(preset)?;
        return Some((json!(2.10), json!(parts.takeoff())));
    }
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

/// Fix lane A1 (2026-10): the take-off flap increment replaces the frozen
/// 0.025 / 0.030 class scalar (Roskam, Airplane Design Part I, Table 3.6 and
/// the component methods of Roskam VI Sec. 4.6, Raymer 6th ed. Eq. 12.61-12.62,
/// Torenbeek App. G-4), and the approach-speed factor becomes the CS 25.125
/// VREF floor of 1.23 VSR0 for every case, not only those with a published
/// landing reference.
fn configuration_drag_change(case: &str, key: &str) -> Option<(Value, Value)> {
    match key {
        "oei_climb_delta_cd" => {
            let frozen = match case {
                "A340-300" | "A380-800" | "B787-9" | "DC-10" | "AVE" | "preset_then_field" => 0.030,
                _ => 0.025,
            };
            Some((json!(frozen), json!(0.015)))
        }
        "vapp_vstall_land_factor" => Some((json!(1.30), json!(1.23))),
        _ => None,
    }
}
