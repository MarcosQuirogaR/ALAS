// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Explicit differences between frozen inputs and source-corrected product presets.

// Each integration-test binary compiles only the helpers its fixture needs.
#![allow(dead_code)]

use serde_json::{json, Value};

/// The frozen A380 engine offset below the wing reference plane, in metres.
/// Named because the lint reads 3.14 as a mistyped pi; it is a length.
#[allow(clippy::approx_constant)]
const A380_FROZEN_ENGINE_Z_M: f64 = -3.14;

/// Geometry corrections pinned by the Airbus dimension references alongside
/// the preset definitions, and the continuous-leading-edge sweep convention.
pub fn dimensions(path: &str) -> Option<(Value, Value)> {
    // This default differs for every preset, so it is matched by suffix
    // rather than one entry per preset name. Raymer ch.11 / Torenbeek
    // recommend ~6-8% minimum nose-gear load for steering/braking authority;
    // the frozen 2% floor left the "Min Nose Load" CG-envelope boundary well
    // inside the aerodynamic aft limit for most configured presets.
    if path.ends_with(".mass_model.pct_load_nlg_min") {
        return Some((json!(0.02), json!(0.06)));
    }
    let (old, new) = match path {
        "A340-300.geometry.empennage.hstab_tip_le_m[1]" => (9.0, 9.7),
        // Airbus A380 AC Rev 20 Dec 01/25, Subject 2-2-0,
        // FIGURE-2-2-0-991-001-A01 Sheet 1 of 2: 30.37 m tailplane span.
        "A380-800.geometry.empennage.hstab_tip_le_m[1]" => (12.5, 15.185),
        "A380-800.design_vector.sweep_deg" => (33.5, 36.419_887_695_518_42),
        // Airbus A380 AC Rev 20, Figure 2-2-0-991-001-A01 sheet 1 (front
        // view): engine centrelines 29.6 m and 51.4 m apart; sheet 2 (plan
        // view): inlets 22.23 m and 29.94 m aft of the nose, tailplane tip
        // leading edge 11.57 m aft of its 57.28 m root and 3.72 m tip chord;
        // sheet 1 (side view): fin root leading edge 53.94 m, 14.08 m root
        // chord, tip 12.06 m aft of the root, 70.4 m tip trailing edge.
        // Figure 2-3-0-991-001-A01 (MRW, aft CG): fin tip 24.12 m, reached
        // by the span from the estimated root; nacelles N1 1.08 m and N2
        // 1.90 m, met on average by one offset under the flight-shape wing.
        "A380-800.engine_spanwise_positions[0]"
        | "A380-800.geometry.engine.spanwise_positions_m[0]" => (10.0, 14.8),
        "A380-800.engine_spanwise_positions[1]"
        | "A380-800.geometry.engine.spanwise_positions_m[1]" => (-10.0, -14.8),
        "A380-800.engine_spanwise_positions[2]"
        | "A380-800.geometry.engine.spanwise_positions_m[2]" => (18.5, 25.7),
        "A380-800.engine_spanwise_positions[3]"
        | "A380-800.geometry.engine.spanwise_positions_m[3]" => (-18.5, -25.7),
        "A380-800.geometry.engine.inlet_x_offset_m" => (4.5, 6.285),
        "A380-800.geometry.engine.z_m" => (A380_FROZEN_ENGINE_Z_M, -3.384),
        "A380-800.geometry.empennage.hstab_offset_from_tail_m" => (11.0, 15.45),
        "A380-800.geometry.empennage.hstab_tip_chord_m" => (2.5, 3.72),
        "A380-800.geometry.empennage.hstab_tip_le_m[0]" => (8.5, 11.57),
        "A380-800.geometry.empennage.vstab_offset_from_tail_m" => (13.0, 18.79),
        "A380-800.geometry.empennage.vstab_root_chord_m" => (11.0, 14.08),
        "A380-800.geometry.empennage.vstab_tip_chord_m" => (3.5, 4.40),
        "A380-800.geometry.empennage.vstab_tip_le_m[0]" => (10.0, 12.06),
        "A380-800.geometry.empennage.vstab_tip_le_m[2]" => (11.0, 15.335),
        "DC-10.design_vector.sweep_deg" => (35.0, 37.926_177_529_939_146),
        // Quarter-chord sweeps entered as leading-edge angles, converted
        // through the outboard taper: NASA/TP-20210023843 (2022) Table I,
        // 787-9 c/4 32 deg (32.2 deg in the preset's open database; the
        // Boeing D6-58333 Rev Q section 2.2.2 plan view reads a 34.7 deg
        // leading edge) and A330-200/300 c/4 30 deg, the A340-200/300 wing.
        "B787-9.design_vector.sweep_deg" => (32.2, 34.714_008_340_548_3),
        "A340-300.design_vector.sweep_deg" => (30.0, 32.037_361_654_529_35),
        // Airbus A340-200/-300 AC Rev 33 FIGURE-2-2-0-991-007-A01 sheet 2:
        // 2.5 m streamwise tip chord at the winglet root.
        "A340-300.design_vector.tip_chord_m" => (1.8, 2.5),
        // Same figure, sheet 1 (front view): engine centrelines 18.74 m and
        // 38.54 m apart; sheet 2: inlets 22.39 m and 28.96 m aft of the nose,
        // tailplane tip leading edge 6.43 m aft of its 55.24 m root and tip
        // trailing edge 63.69 m; sheet 1 (side view): fin root leading edge
        // 52.42 m, 7.78 m root chord, tip 8.14 m aft and 8.3 m above the
        // crown, reached by the span from the estimated root, 2.28 m tip
        // chord. Figure 2-3-0-991-005-A01 (aft CG): wing tip W2 5.94 m and
        // nacelles N1 1.28 m and N2 2.35 m above the ground over the 2.13 m
        // forward belly (F2).
        "A340-300.engine_spanwise_positions[0]"
        | "A340-300.geometry.engine.spanwise_positions_m[0]" => (7.5, 9.37),
        "A340-300.engine_spanwise_positions[1]"
        | "A340-300.geometry.engine.spanwise_positions_m[1]" => (-7.5, -9.37),
        "A340-300.engine_spanwise_positions[2]"
        | "A340-300.geometry.engine.spanwise_positions_m[2]" => (14.0, 19.27),
        "A340-300.engine_spanwise_positions[3]"
        | "A340-300.geometry.engine.spanwise_positions_m[3]" => (-14.0, -19.27),
        "A340-300.geometry.engine.inlet_x_offset_m" => (3.0, 4.227),
        "A340-300.geometry.engine.z_m" => (-1.94, -1.436),
        "A340-300.geometry.wing.break_z_m" => (-0.3, -0.922),
        "A340-300.geometry.wing.tip_z_m" => (2.0, 1.315),
        "A340-300.geometry.empennage.hstab_offset_from_tail_m" => (9.0, 8.42),
        "A340-300.geometry.empennage.hstab_tip_chord_m" => (1.8, 2.02),
        "A340-300.geometry.empennage.hstab_tip_le_m[0]" => (6.0, 6.43),
        "A340-300.geometry.empennage.vstab_offset_from_tail_m" => (10.5, 11.24),
        "A340-300.geometry.empennage.vstab_root_chord_m" => (8.0, 7.78),
        "A340-300.geometry.empennage.vstab_tip_chord_m" => (2.8, 2.28),
        "A340-300.geometry.empennage.vstab_tip_le_m[0]" => (7.5, 8.14),
        "A340-300.geometry.empennage.vstab_tip_le_m[2]" => (8.5, 9.52),
        // Airbus A220 ACP Issue 013, DM BD500-A-J06-10-00-00AAA-030A-A,
        // Figure 1 sheet 2: 29.5 deg straight leading edge on the plan view.
        "A220-300.design_vector.sweep_deg" => (25.0, 29.5),
        // Same document: body height 146.5 in (locator B); engine
        // centreline 5.44 m from locators P, Q and BB, inlet 12.17 m (K
        // table value), nacelle 22.9 in above the ground over the 1.664 m
        // belly; tailplane 36.6 m^2 (Table 6) over 12.263 m (D) with its tip
        // trailing edge 4.94 m aft of the root leading edge (V) at 37.82 m
        // (H); fin chords from the 28.2 m^2 Table 6 area taken to the fuselage
        // axis, tip 11.578 m above the ground (C) reached by the span from
        // the estimated root. The saved-file case
        // `preset_only` loads this preset.
        "A220-300.geometry.fuselage.height_m" | "preset_only.geometry.fuselage.height_m" => {
            return Some((Value::Null, json!(3.721)))
        }
        "A220-300.engine_spanwise_positions[0]"
        | "A220-300.geometry.engine.spanwise_positions_m[0]"
        | "preset_only.geometry.engine.spanwise_positions_m[0]" => (5.2, 5.44),
        "A220-300.engine_spanwise_positions[1]"
        | "A220-300.geometry.engine.spanwise_positions_m[1]"
        | "preset_only.geometry.engine.spanwise_positions_m[1]" => (-5.2, -5.44),
        "A220-300.geometry.engine.inlet_x_offset_m"
        | "preset_only.geometry.engine.inlet_x_offset_m" => (2.3, 3.668),
        "A220-300.geometry.engine.z_m" | "preset_only.geometry.engine.z_m" => (-1.71, -1.410),
        "A220-300.geometry.empennage.hstab_offset_from_tail_m"
        | "preset_only.geometry.empennage.hstab_offset_from_tail_m" => (5.0, 5.82),
        "A220-300.geometry.empennage.hstab_root_chord_m"
        | "preset_only.geometry.empennage.hstab_root_chord_m" => (3.8, 4.629),
        "A220-300.geometry.empennage.hstab_tip_chord_m"
        | "preset_only.geometry.empennage.hstab_tip_chord_m" => (1.1, 1.340),
        "A220-300.geometry.empennage.hstab_tip_le_m[0]"
        | "preset_only.geometry.empennage.hstab_tip_le_m[0]" => (3.2, 3.60),
        "A220-300.geometry.empennage.hstab_tip_le_m[1]"
        | "preset_only.geometry.empennage.hstab_tip_le_m[1]" => (5.5, 6.1315),
        "A220-300.geometry.empennage.vstab_root_chord_m"
        | "preset_only.geometry.empennage.vstab_root_chord_m" => (5.0, 4.516),
        "A220-300.geometry.empennage.vstab_tip_chord_m"
        | "preset_only.geometry.empennage.vstab_tip_chord_m" => (1.6, 1.616),
        "A220-300.geometry.empennage.vstab_tip_le_m[2]"
        | "preset_only.geometry.empennage.vstab_tip_le_m[2]" => (5.5, 7.1535),
        // NASA Common Research Model, AIAA 2008-6919 Table 2 (the widebody
        // wing NASA/TP-20210023843 Table I sets beside the 787-9): 0.115
        // area-weighted t/c, which the 12 % root section reproduces in a loft
        // that holds the root section to the kink; the 14 % section gave 0.133.
        "B787-9.geometry.wing.root_airfoil" => return Some((json!("sc20614"), json!("sc20612"))),
        // Boeing DC-10 ACAP DAC-67803A Rev A, Figure 2.2 (Series 30 general
        // dimensions): printed 71 ft 2 in (21.69 m) tailplane span; chords,
        // sweep and root station read off the plan view's 5 ft grid.
        "DC-10.geometry.empennage.hstab_tip_le_m[1]" => (8.5, 10.845),
        "DC-10.geometry.empennage.hstab_tip_le_m[0]" => (5.8, 9.39),
        "DC-10.geometry.empennage.hstab_root_chord_m" => (7.2, 8.93),
        "DC-10.geometry.empennage.hstab_tip_chord_m" => (2.0, 3.07),
        "DC-10.geometry.empennage.hstab_offset_from_tail_m" => (8.5, 13.63),
        // Same figure: underwing engines 26 ft 10 in (8.18 m) from the
        // centreline; the centre engine stays on the centreline.
        "DC-10.engine_spanwise_positions[0]" | "DC-10.geometry.engine.spanwise_positions_m[0]" => {
            (8.8, 8.18)
        }
        "DC-10.engine_spanwise_positions[1]" | "DC-10.geometry.engine.spanwise_positions_m[1]" => {
            (-8.8, -8.18)
        }
        // Airbus A320 AC Jun 01/24, FIGURE-2-2-0-991-004-A01: the 34.10 m
        // planar span (wing-tip-fence sheet 1; the 35.80 m of sheet 3 is over
        // the sharklets), the 1.64 m chord at the 16.29 m aileron-end station
        // (the equivalent-trapezoid tip chord) and the 27.1 deg straight
        // leading edge of the sheet 4 plan view; centreline and kink
        // chords and the kink station close 122.6 m^2 and the EASA.A.064
        // 4.1935 m MAC with an unswept inboard trailing edge.
        "A320-200.design_vector.span_m" => (35.8, 34.10),
        "A320-200.design_vector.break_chord_m" => (3.8, 3.566),
        "A320-200.design_vector.root_chord_m" => (6.1, 6.875),
        "A320-200.design_vector.sweep_deg" => (25.0, 27.1),
        "A320-200.design_vector.tip_chord_m" => (1.2, 1.64),
        "A320-200.engine_spanwise_positions[0]"
        | "A320-200.geometry.engine.spanwise_positions_m[0]" => (5.5, 5.755),
        "A320-200.engine_spanwise_positions[1]"
        | "A320-200.geometry.engine.spanwise_positions_m[1]" => (-5.5, -5.755),
        // Sheet 2 (plan view, drawing read): 1.24 m tailplane tip chord and
        // tip leading edge 3.31 m aft of the root's; the root chord closes
        // 31.0 m^2. Sheet 1: the 5.87 m fin height runs from the fuselage
        // top line; the span from the estimated root holds that tip height.
        "A320-200.geometry.empennage.hstab_root_chord_m" => (4.0, 3.740),
        "A320-200.geometry.empennage.hstab_tip_chord_m" => (1.2, 1.24),
        "A320-200.geometry.empennage.hstab_tip_le_m[0]" => (3.5, 3.31),
        "A320-200.geometry.empennage.hstab_tip_le_m[1]" => (6.0, 6.225),
        "A320-200.geometry.empennage.vstab_root_chord_m" => (5.2, 5.444),
        "A320-200.geometry.empennage.vstab_tip_chord_m" => (1.8, 1.884),
        "A320-200.geometry.empennage.vstab_tip_le_m[0]" => (5.0, 5.06),
        "A320-200.geometry.empennage.vstab_tip_le_m[2]" => (5.8, 6.84),
        "A320-200.geometry.fuselage.height_m" => return Some((Value::Null, json!(4.14))),
        "A320-200.geometry.wing.break_span_fraction" => (0.37, 0.379_3),
        // Airbus A320 AC Jun 01/24, FIGURE-2-3-0-991-029-A01 sheet 2 (MRW,
        // aft CG): sharklet bottom 4.009 m and fuselage bottom 1.792 m above
        // the ground set the 5.1 deg dihedral; the CFM56-5B nacelle low point
        // 0.577 m sets the engine axis under the lowered wing.
        "A320-200.geometry.wing.break_z_m" => (-0.2, -0.62),
        "A320-200.geometry.wing.tip_z_m" => (1.5, 0.33),
        "A320-200.geometry.engine.z_m" => (-1.71, -1.50),
        // Airbus A320 AC Jun 01/24, FIGURE-2-2-0-991-004-A01 sheet 4: CFM56
        // nacelle front 11.19 m aft of the nose.
        "A320-200.geometry.engine.inlet_x_offset_m" => (2.5, 3.646),
        // Boeing 787 ACAP D6-58333 Rev Q section 2.2.2 plan view: engine
        // centreline 9.91 m outboard, nacelle inlet face 20.80 m aft of the
        // nose.
        "B787-9.engine_spanwise_positions[0]"
        | "B787-9.geometry.engine.spanwise_positions_m[0]" => (9.5, 9.91),
        "B787-9.engine_spanwise_positions[1]"
        | "B787-9.geometry.engine.spanwise_positions_m[1]" => (-9.5, -9.91),
        "B787-9.geometry.engine.inlet_x_offset_m" => (3.5, 6.118),
        // Boeing 787 ACAP D6-58333 Rev Q section 2.2.2 (PDF p.22): 65 ft 0 in
        // tailplane span with its tip trailing edge at the 206 ft 1 in overall
        // length, which is longer than the EASA TCDS 62.0014 m body. Section
        // 2.3.2 (PDF p.25): GEnx nacelle 0.69 m and fin top 16.81 m above the
        // ground over the 1.75 m forward-belly clearance, the fin reaching it
        // by its span from the estimated root. The saved-file case
        // `preset_then_field` loads this preset.
        "B787-9.design_vector.fuselage_length_m" => (62.81, 62.00),
        // Section 2.2.2: body width 18 ft 11 in and height 19 ft 6 in.
        "B787-9.geometry.fuselage.diameter_m"
        | "preset_then_field.geometry.fuselage.diameter_m" => (5.94, 5.77),
        "B787-9.geometry.fuselage.height_m" | "preset_then_field.geometry.fuselage.height_m" => {
            return Some((Value::Null, json!(5.94)))
        }
        "B787-9.geometry.engine.z_m" | "preset_then_field.geometry.engine.z_m" => (-2.55, -1.821),
        "B787-9.geometry.empennage.hstab_offset_from_tail_m"
        | "preset_then_field.geometry.empennage.hstab_offset_from_tail_m" => (9.5, 6.99),
        "B787-9.geometry.empennage.hstab_tip_le_m[1]"
        | "preset_then_field.geometry.empennage.hstab_tip_le_m[1]" => (9.5, 9.905),
        "B787-9.geometry.empennage.vstab_offset_from_tail_m"
        | "preset_then_field.geometry.empennage.vstab_offset_from_tail_m" => (10.5, 9.69),
        "B787-9.geometry.empennage.vstab_tip_le_m[2]"
        | "preset_then_field.geometry.empennage.vstab_tip_le_m[2]" => (8.5, 10.49),
        "A320-200.geometry.wing.root_datum_x_m" => (12.9, 11.891),
        // Wing roots re-anchored so the model quarter-MAC point sits on the
        // manufacturer weight-and-balance one, derived by two-point statics
        // from each airport-planning document's section 7 gear loads (A220,
        // A340 and B787 at their converted leading-edge sweeps).
        "A220-300.geometry.wing.root_datum_x_m" => (13.3, 12.760),
        "A340-300.geometry.wing.root_datum_x_m" => (22.0, 22.341),
        "A380-800.geometry.wing.root_datum_x_m" => (26.7, 24.930),
        "B787-9.geometry.wing.root_datum_x_m" => (21.0, 21.552),
        "DC-10.geometry.wing.root_datum_x_m" => (21.4, 22.58),
        "A320-200.requirements.max_structural_payload_kg" => (19_900.0, 21_256.0),
        // Standard gravity: the frozen two-decimal 9.81 corrected to
        // `alas_units::STANDARD_GRAVITY` (9.80665), the CODATA/exact
        // definitional value that already drives every lbf conversion and
        // the ISA elsewhere in the program. Every registered preset shares this default.
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

/// A key the frozen reference carries that the product schema removed: the
/// solver `strategy` and the objective weights only the retired weighted
/// lift-to-drag objective read. The load boundary drops the same keys.
pub fn retired_reference_key(path: &str, key: &str) -> bool {
    path.ends_with(".solver") && alas_config::RETIRED_SOLVER_KEYS.contains(&key)
        || path.ends_with(".weights") && alas_config::RETIRED_WEIGHT_KEYS.contains(&key)
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
                    // Dynamic nose-braking, tip-back and
                    // ground-clearance additions the frozen configuration
                    // has no equivalent field for at all.
                    | "nlg_dynamic_braking_decel_g"
                    | "tire_dynamic_rating_factor"
                    | "min_tip_back_deg"
                    | "required_rotation_angle_deg"
                    | "fuselage_ground_clearance_m"
                    // The rotation and landing-trim forward-CG
                    // criteria's own conceptual-design inputs. The frozen
                    // configuration has no field for them at all.
                    | "rotation_pitch_acceleration_deg_s2"
                    | "pitch_radius_of_gyration_frac_mac"
                    | "cl_ground_attitude_frac_of_cl_max_to"
                    | "rotation_rolling_friction_coefficient"
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
                    // The separate handling/steering nose-load
                    // ceiling, distinct from tire-capacity strength. The
                    // frozen configuration has no field for it at all.
                    | "pct_load_nlg_max_handling"
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
        // The cargo capacity objective is a requested target the frozen configuration has no
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
        "A320-200.geometry.wing.side_of_body_chord_ratio" => Some(json!(0.852_98)),
        // Airbus A380 AC Rev 20, Figure 2-2-0-991-001-A01 sheet 2: 17.67 m
        // chord at the side of the body.
        "A380-800.geometry.wing.side_of_body_chord_ratio" => Some(json!(0.985_244_323_994_896_8)),
        "DC-10.geometry.wing.side_of_body_chord_ratio" => Some(json!(0.925_625_096_354_822)),
        "A320-200.geometry.wing.kink_span_fraction" => Some(json!(0.379_3)),
        "A320-200.geometry.wing.side_of_body_span_fraction" => Some(json!(0.115_84)),
        // Airbus A380 AC Rev 20, Figure 2-2-0-991-001-A01: half the 7.14 m body.
        "A380-800.geometry.wing.side_of_body_span_fraction" => Some(json!(3.57 / 39.875)),
        _ => None,
    }
}
