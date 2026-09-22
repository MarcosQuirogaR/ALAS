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
        // Airbus A380 AC Rev 20 Dec 01/25, Subject 2-2-0,
        // FIGURE-2-2-0-991-001-A01 Sheet 1 of 2: 30.37 m tailplane span.
        "A380-800.geometry.empennage.hstab_tip_le_m[1]" => (12.5, 15.185),
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
        // Standard gravity: the frozen two-decimal 9.81 corrected to
        // `alas_units::STANDARD_GRAVITY` (9.80665), the CODATA/exact
        // definitional value that already drives every lbf conversion and
        // the ISA elsewhere in the program. Physics review v1.2, finding
        // F5. Every registered preset shares this default.
        "AVE.requirements.gravity_m_s2"
        | "A340-300.requirements.gravity_m_s2"
        | "A380-800.requirements.gravity_m_s2"
        | "B787-9.requirements.gravity_m_s2"
        | "A320-200.requirements.gravity_m_s2"
        | "A220-300.requirements.gravity_m_s2"
        | "DC-10.requirements.gravity_m_s2"
        | "empty.requirements.gravity_m_s2"
        | "preset_only.requirements.gravity_m_s2"
        | "preset_then_field.requirements.gravity_m_s2"
        | "tuple_field_from_a_list.requirements.gravity_m_s2"
        | "airports.requirements.gravity_m_s2"
        | "unknown_preset.requirements.gravity_m_s2"
        | "deep_partial.requirements.gravity_m_s2" => (9.81, 9.806_65),
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
    (matches!(key, "fuel_policy" | "fuel_tanks" | "downstream") && !path.contains('.'))
        || (path.ends_with(".landing_gear")
            && matches!(
                key,
                "reference_wheelbase_m"
                    | "reference_station_frame"
                    | "reference_station_fuselage_length_m"
                    | "reference_nlg_x_fraction"
                    | "reference_mlg_x_fractions"
                    | "reference_body_wheelbase_m"
                    | "reference_track_m"
                    | "mlg_strut_bogie_wheels"
            ))
        || (path.ends_with(".optimizer") && matches!(key, "objective" | "design_space"))
        || (path.ends_with(".mass_model")
            && matches!(
                key,
                "schema_version"
                    | "mass_architecture"
                    | "flops_transport"
                    | "geometric_component_stations"
                    | "structural_mass_method"
                    | "propulsion_mass_method"
                    | "flops_structure"
            ))
        || (path.ends_with(".geometry.engine")
            && matches!(key, "turbofan" | "turboprop" | "propulsion_technology"))
        // The conceptual free-turbine design cycle. The frozen Python
        // propulsion configuration is turbofan-only and declares neither
        // field, so no saved file can disagree about a value it never had.
        // Their defaults are pinned by `parity_config`'s
        // `the_native_turboprop_design_inputs_are_absent_upstream_and_pinned_here`.
        || (path.ends_with(".propulsion_cycle")
            && matches!(
                key,
                "turboprop_overall_pressure_ratio" | "turboprop_turbine_inlet_temperature_k"
            ))
        || (path.ends_with(".mission")
            && matches!(
                key,
                "use_airway_endpoint_coordinates" | "max_airway_stretch"
            ))
        // Whether a preset's climb and descent rungs are stated in calibrated
        // or true airspeed. The frozen configuration has one ladder in literal
        // true airspeed and no way to say otherwise, so this is a native
        // addition rather than a disagreement about a value.
        || (path.ends_with(".mission.profile") && key == "climb_descent_speed_reference")
        || (path.ends_with(".optimizer.solver")
            && matches!(
                key,
                "finite_difference_step"
                    | "constraint_tolerance"
                    | "convergence_stagnation_generations"
            ))
        || (path.ends_with(".drag_model") && key == "exclude_buried_main_wing_area")
        // The cargo capacity objective (clarified ledger App Features 2,
        // decision D10) is a requested target the frozen configuration has no
        // field for: a native addition rather than a disagreement about a
        // value. Every registered preset leaves it at its disabled default,
        // so no preset's resolved cargo target moves; `alas-opt`'s
        // `cargo_target_objective` tests pin that.
        || (path.ends_with(".requirements") && key == "cargo_objective_kg")
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
    // Every flown speed, rate and rung boundary a preset declares for its own
    // route, not only the three cruise speeds. The frozen configuration has no
    // per-aircraft operational profile at all: it carries one ladder, stated
    // in *literal true airspeeds* and written for the AVE reference
    // aircraft's FL390/M0.84 design point. A true airspeed is not a flight
    // condition, so that ladder means something different at every other
    // preset's cruise level - on the A320-200 at FL280 its 250 m/s upper
    // climb rung is about 178 m/s equivalent, and the mission deck refuses it
    // with 57 046 N of drag against 56 106 N of maximum-climb rating. The
    // presets that declare their own calibrated ladder (the ATR 72-600, and
    // now the A320-200) are therefore compared against *their own* declared
    // operational default, which is the authority here, rather than against a
    // frozen number that was never about them.
    if let Some(key) = field.strip_prefix("mission.profile.") {
        let profile = serde_json::to_value(&defaults.profile).ok()?;
        return profile.get(key).cloned();
    }
    Some(match field {
        "departure_airport" => json!(defaults.departure_airport),
        "arrival_airport" => json!(defaults.arrival_airport),
        // The hold architecture is declared once, in
        // `preset_flops::declared_cargo_loading`, and read by both
        // `planning_cabin_config` and the FLOPS container tare. This used to
        // name the A220-300 alone because that was the only preset carrying
        // the bulk declaration; the declaration now also covers the ATR 72-600
        // (no lower hold at all) and the A320-200, so the correction reads the
        // product's own value instead of repeating one preset's name. The
        // value itself is pinned by that declaration's own tests, not here.
        "cabin.cargo.lower_deck_uld" => json!(preset.planning_cabin_config().cargo.lower_deck_uld),
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
