// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Explicit differences between frozen inputs and source-corrected product presets.

// Each integration-test binary compiles only the helpers its fixture needs.
#![allow(dead_code)]

use serde_json::{json, Value};

/// Geometry corrections pinned by the Airbus dimension references alongside
/// the preset definitions, and the continuous-leading-edge sweep convention.
pub fn dimensions(path: &str) -> Option<(Value, Value)> {
    let (old, new) = match path {
        "A340-300.geometry.empennage.hstab_tip_le_m[1]" => (9.0, 9.7),
        "A380-800.design_vector.sweep_deg" => (33.5, 36.429_099_956_878_43),
        "DC-10.design_vector.sweep_deg" => (35.0, 38.371_897_798_492_91),
        "A320-200.design_vector.break_chord_m" => (3.8, 3.432),
        "A320-200.design_vector.root_chord_m" => (6.1, 7.333),
        "A320-200.design_vector.sweep_deg" => (25.0, 27.0),
        "A320-200.design_vector.tip_chord_m" => (1.2, 1.4),
        "A320-200.engine_spanwise_positions[0]"
        | "A320-200.geometry.engine.spanwise_positions_m[0]" => (5.5, 5.755),
        "A320-200.engine_spanwise_positions[1]"
        | "A320-200.geometry.engine.spanwise_positions_m[1]" => (-5.5, -5.755),
        "A320-200.geometry.empennage.hstab_root_chord_m" => (4.0, 3.831),
        "A320-200.geometry.empennage.hstab_tip_chord_m" => (1.2, 1.149),
        "A320-200.geometry.empennage.hstab_tip_le_m[0]" => (3.5, 3.631),
        "A320-200.geometry.empennage.hstab_tip_le_m[1]" => (6.0, 6.225),
        "A320-200.geometry.empennage.vstab_root_chord_m" => (5.2, 5.444),
        "A320-200.geometry.empennage.vstab_tip_chord_m" => (1.8, 1.884),
        "A320-200.geometry.empennage.vstab_tip_le_m[0]" => (5.0, 5.06),
        "A320-200.geometry.empennage.vstab_tip_le_m[2]" => (5.8, 5.87),
        "A320-200.geometry.fuselage.height_m" => return Some((Value::Null, json!(4.14))),
        "A320-200.geometry.wing.break_span_fraction" => (0.37, 0.34),
        "A320-200.geometry.wing.root_datum_x_m" => (12.9, 12.913),
        "A320-200.requirements.max_structural_payload_kg" => (19_900.0, 21_256.0),
        _ => return None,
    };
    Some((json!(old), json!(new)))
}

/// The preset's engine copy must match the independently tested catalogue.
/// These fields were GE9X defaults in the frozen presets, despite their selector.
pub fn engine_copy(path: &str) -> Option<Value> {
    let (case, key) = path.split_once(".geometry.engine.")?;
    let preset = match case {
        "preset_only" => "A220-300",
        "preset_then_field" => "B787-9",
        name if alas_config::presets::get(name).is_ok() => name,
        _ => return None,
    };
    let name = alas_config::presets::get(preset).ok()?.engine_name;
    let engine = alas_config::engines::get(name).ok()?;
    Some(match key {
        // The product binds the typed turbofan rating, which prefers the
        // identity-qualified ICAO LTO rating over the legacy design scalar.
        "thrust_kn" => json!(engine
            .turbofan_spec()
            .map_or(engine.thrust_kn, |typed| typed.rated_thrust_kn)),
        "bypass_ratio" => json!(engine.bypass_ratio),
        "cruise_tsfc_kg_kgf_hr" => json!(engine.cruise_tsfc_kg_kgf_hr),
        "fan_diameter_m" => json!(engine.fan_diameter_m),
        "fan_pressure_ratio" => json!(engine.fan_pressure_ratio),
        "overall_pressure_ratio" => json!(engine.overall_pressure_ratio),
        "turbine_inlet_temp_k" => json!(engine.turbine_inlet_temp_k),
        "radius_scale_m" => json!(engine.nacelle_max_radius_m),
        "nacelle_profile" => json!(engine.nacelle_profile()),
        "part_power_fuel_flow_ratios" => json!(engine.part_power_fuel_flow_ratios),
        "part_power_source" => json!(engine.part_power_source),
        _ => return None,
    })
}

/// Additive propulsion fields are validated by the active-binding unit tests.
pub fn native_field(path: &str, key: &str) -> bool {
    (matches!(key, "fuel_policy" | "fuel_tanks") && !path.contains('.'))
        || (path.ends_with(".optimizer") && key == "objective")
        || (path.ends_with(".mass_model") && key == "geometric_component_stations")
        || (path.ends_with(".geometry.engine")
            && matches!(key, "turbofan" | "turboprop" | "propulsion_technology"))
        || (path.ends_with(".mission")
            && matches!(
                key,
                "use_airway_endpoint_coordinates" | "max_airway_stretch"
            ))
        || (path.ends_with(".optimizer.solver") && key == "enforce_physical_constraints")
        || (path.ends_with(".drag_model") && key == "exclude_buried_main_wing_area")
}

/// Named operational defaults are product additions, with their inputs and
/// route provenance checked by the preset and route integration tests.
pub fn operational(path: &str) -> Option<Value> {
    let (case, field) = path.split_once('.')?;
    let name = match case {
        "preset_only" => "A220-300",
        "preset_then_field" => "B787-9",
        name => name,
    };
    let preset = alas_config::presets::get(name).ok()?;
    let defaults = preset.operational_mission_defaults();
    Some(match field {
        "departure_airport" => json!(defaults.departure_airport),
        "arrival_airport" => json!(defaults.arrival_airport),
        "mission.profile.cruise_1_air_speed_m_s" => json!(defaults.profile.cruise_1_air_speed_m_s),
        "mission.profile.cruise_2_air_speed_m_s" => json!(defaults.profile.cruise_2_air_speed_m_s),
        "mission.profile.cruise_3_air_speed_m_s" => json!(defaults.profile.cruise_3_air_speed_m_s),
        "cabin.cargo.lower_deck_uld" if name == "A220-300" => json!("BLK"),
        _ => return None,
    })
}

/// Source-constrained side-of-body corrections have no Python counterpart.
pub fn added_planform(path: &str) -> Option<Value> {
    match path {
        "A320-200.geometry.wing.side_of_body_chord_ratio" => Some(json!(0.827_405)),
        "A380-800.geometry.wing.side_of_body_chord_ratio" => Some(json!(0.789_394_889_312_722_8)),
        "DC-10.geometry.wing.side_of_body_chord_ratio" => Some(json!(0.888_392_857_142_857_5)),
        "A320-200.geometry.wing.kink_span_fraction" => Some(json!(0.34)),
        "A320-200.geometry.wing.side_of_body_span_fraction" => Some(json!(0.1103)),
        _ => None,
    }
}
