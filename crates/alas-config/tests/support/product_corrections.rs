// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Explicit differences between frozen inputs and source-corrected product presets.

// Each integration-test binary compiles only the helpers its fixture needs.
#![allow(dead_code)]

use serde_json::{json, Value};

#[path = "performance_fields.rs"]
mod performance_fields;
pub use performance_fields::{declared_flap_setting_deg, published_landing_change};
#[path = "drawing_fit_2026_11.rs"]
mod drawing_fit_2026_11;
#[path = "geometry_phase1.rs"]
mod geometry_phase1;
#[path = "wing_round2.rs"]
mod wing_round2;

/// The frozen A380 engine offset below the wing reference plane, in metres.
/// Named because the lint reads 3.14 as a mistyped pi; it is a length.
#[allow(clippy::approx_constant)]
const A380_FROZEN_ENGINE_Z_M: f64 = -3.14;

/// Geometry corrections pinned by the Airbus dimension references alongside
/// the preset definitions, and the continuous-leading-edge sweep convention.
pub fn dimensions(path: &str) -> Option<(Value, Value)> {
    let base = dimensions_before_phase1(path);
    let numbers = base
        .as_ref()
        .and_then(|(old, new)| Some((old.as_f64()?, new.as_f64()?)));
    let phase1 = geometry_phase1::moved(path, numbers);
    // The 2026-11 drawing passes supersede phase 1 on the paths they list.
    match drawing_fit_2026_11::moved(path, phase1.or(numbers)).or(phase1) {
        Some((old, new)) => Some((json!(old), json!(new))),
        None => base,
    }
}

/// Ledger entries before geometry phase 1 (`geometry_phase1` supersedes the paths it lists).
fn dimensions_before_phase1(path: &str) -> Option<(Value, Value)> {
    if let Some((old, new)) = wing_round2::lookup(path) {
        return Some((json!(old), json!(new)));
    }
    // This default differs for every preset, so it is matched by suffix
    // rather than one entry per preset name. The class floor is the lowest
    // nose-gear share at a certified most-aft CG in a sample of 13 airliner
    // families (3.52 %; Raymer's 5 % is a design guideline); the frozen 2% floor
    // left the "Min Nose Load" CG-envelope boundary well inside the aerodynamic
    // aft limit for most configured presets.
    if path.ends_with(".mass_model.pct_load_nlg_min") {
        return Some((json!(0.02), json!(0.0352)));
    }
    // Tail effectiveness: unset in the product, computed from the geometry
    // (DATCOM 4.4.1 wing wake, Roskam Part VI Eqs. 8.34-8.41); the frozen
    // value was 0.90.
    if path.ends_with(".analysis.tail_efficiency") {
        return Some((json!(0.90), Value::Null));
    }
    // Viscous margin: sourced leakage and protuberance allowance (see
    // `DEFAULT_VISCOUS_MARGIN`).
    if path.ends_with(".drag_model.viscous_margin") {
        // Unset: the propulsion class picks the Raymer Table 12.8 row at analysis time.
        return Some((json!(1.10), Value::Null));
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
        // Figure 2-3-0-991-001-A01 (MRW, aft CG): fin tip 24.12 m, which
        // puts the root line at 3.245 m; wing tip W2 5.21 m and
        // nacelles N1 1.08 m and N2 1.90 m on the ground-shape wing.
        "A380-800.engine_spanwise_positions[0]"
        | "A380-800.geometry.engine.spanwise_positions_m[0]" => (10.0, 14.8),
        "A380-800.engine_spanwise_positions[1]"
        | "A380-800.geometry.engine.spanwise_positions_m[1]" => (-10.0, -14.8),
        "A380-800.engine_spanwise_positions[2]"
        | "A380-800.geometry.engine.spanwise_positions_m[2]" => (18.5, 25.7),
        "A380-800.engine_spanwise_positions[3]"
        | "A380-800.geometry.engine.spanwise_positions_m[3]" => (-18.5, -25.7),
        "A380-800.geometry.engine.inlet_x_offset_m" => (4.5, 6.285),
        "A380-800.geometry.engine.z_m" => (A380_FROZEN_ENGINE_Z_M, -0.82),
        "A380-800.geometry.wing.break_z_m" => (-0.4, -2.5),
        "A380-800.geometry.wing.tip_z_m" => (3.0, -0.876),
        "A380-800.geometry.empennage.hstab_offset_from_tail_m" => (11.0, 15.45),
        "A380-800.geometry.empennage.hstab_tip_chord_m" => (2.5, 3.72),
        "A380-800.geometry.empennage.hstab_tip_le_m[0]" => (8.5, 11.57),
        "A380-800.geometry.empennage.vstab_offset_from_tail_m" => (13.0, 18.79),
        "A380-800.geometry.empennage.vstab_root_chord_m" => (11.0, 14.08),
        "A380-800.geometry.empennage.vstab_tip_chord_m" => (3.5, 4.40),
        "A380-800.geometry.empennage.vstab_tip_le_m[0]" => (10.0, 12.06),
        "A380-800.geometry.empennage.vstab_tip_le_m[2]" => (11.0, 14.59),
        "A380-800.geometry.empennage.vstab_z_m" => (2.5, 3.245),
        "DC-10.design_vector.sweep_deg" => (35.0, 37.926_177_529_939_146),
        // No fin station is published; the frozen offset left the 10.5 m
        // root chord 0.5 m past the tail tip, and this one keeps it over the
        // body.
        "DC-10.geometry.empennage.vstab_offset_from_tail_m" => (10.0, 13.6),
        "DC-10.geometry.empennage.vstab_root_chord_m" => (10.5, 5.0),
        "DC-10.geometry.empennage.vstab_tip_chord_m" => (3.8, 2.9),
        "DC-10.geometry.empennage.vstab_tip_le_m[0]" => (7.5, 9.25),
        "DC-10.geometry.empennage.vstab_tip_le_m[2]" => (9.5, 9.3),
        "DC-10.geometry.empennage.vstab_z_m" => (2.2, 3.1),
        // DAC-67803A Rev A Figure 2.2 plan and side view: the underwing
        // nacelle inlet face is 19.7 m aft of the nose, 6.0 m ahead of the
        // wing leading edge at the 8.18 m engine station (the frozen 3.5 m
        // put it at 22.2 m).
        "DC-10.geometry.engine.inlet_x_offset_m" => (3.5, 6.0),
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
        // crown, 2.28 m tip chord. Figure 2-3-0-991-005-A01 (aft CG): wing
        // tip W2 5.94 m and nacelles N1 1.28 m and N2 2.35 m above the
        // ground over the 2.13 m forward belly (F2).
        "A340-300.engine_spanwise_positions[0]"
        | "A340-300.geometry.engine.spanwise_positions_m[0]" => (7.5, 9.37),
        "A340-300.engine_spanwise_positions[1]"
        | "A340-300.geometry.engine.spanwise_positions_m[1]" => (-7.5, -9.37),
        "A340-300.engine_spanwise_positions[2]"
        | "A340-300.geometry.engine.spanwise_positions_m[2]" => (14.0, 19.27),
        "A340-300.engine_spanwise_positions[3]"
        | "A340-300.geometry.engine.spanwise_positions_m[3]" => (-14.0, -19.27),
        "A340-300.geometry.engine.inlet_x_offset_m" => (3.0, 4.227),
        "A340-300.geometry.engine.z_m" => (-1.94, -1.2742),
        "A340-300.geometry.wing.break_z_m" => (-0.3, -0.922),
        "A340-300.geometry.wing.tip_z_m" => (2.0, 1.315),
        "A340-300.geometry.empennage.hstab_offset_from_tail_m" => (9.0, 8.42),
        "A340-300.geometry.empennage.hstab_tip_chord_m" => (1.8, 2.02),
        "A340-300.geometry.empennage.hstab_tip_le_m[0]" => (6.0, 6.43),
        "A340-300.geometry.empennage.vstab_offset_from_tail_m" => (10.5, 11.24),
        "A340-300.geometry.empennage.vstab_root_chord_m" => (8.0, 7.78),
        "A340-300.geometry.empennage.vstab_tip_chord_m" => (2.8, 2.28),
        "A340-300.geometry.empennage.vstab_tip_le_m[0]" => (7.5, 8.14),
        "A340-300.geometry.empennage.vstab_tip_le_m[2]" => (8.5, 8.3),
        "A340-300.geometry.empennage.vstab_z_m" => (1.8, 3.02),
        // Airbus A220 ACP Issue 013, DM BD500-A-J06-10-00-00AAA-030A-A,
        // Figure 1 sheet 2: 29.5 deg straight leading edge on the plan view.
        "A220-300.design_vector.sweep_deg" => (25.0, 29.5),
        // Same document: body height 146.5 in (locator B); engine
        // centreline 5.44 m from locators P, Q and BB, inlet 12.17 m (K
        // table value), nacelle 22.9 in above the ground over the 1.664 m
        // belly; tailplane 36.6 m^2 (Table 6) over 12.263 m (D) with its tip
        // trailing edge 4.94 m aft of the root leading edge (V) at 37.82 m
        // (H); fin 28.2 m^2 to the fuselage axis (Table 6), root on the
        // crown, tip 11.578 m above the ground (C). The saved-file case
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
        | "preset_only.geometry.engine.inlet_x_offset_m" => (2.3, 3.6871),
        "A220-300.geometry.engine.z_m" | "preset_only.geometry.engine.z_m" => (-1.71, -1.211),
        "A220-300.geometry.empennage.hstab_offset_from_tail_m"
        | "preset_only.geometry.empennage.hstab_offset_from_tail_m" => (5.0, 6.55),
        "A220-300.geometry.empennage.hstab_root_chord_m"
        | "preset_only.geometry.empennage.hstab_root_chord_m" => (3.8, 4.629),
        "A220-300.geometry.empennage.hstab_tip_chord_m"
        | "preset_only.geometry.empennage.hstab_tip_chord_m" => (1.1, 1.340),
        "A220-300.geometry.empennage.hstab_tip_le_m[0]"
        | "preset_only.geometry.empennage.hstab_tip_le_m[0]" => (3.2, 4.26),
        "A220-300.geometry.empennage.hstab_tip_le_m[1]"
        | "preset_only.geometry.empennage.hstab_tip_le_m[1]" => (5.5, 6.1315),
        "A220-300.geometry.empennage.vstab_root_chord_m"
        | "preset_only.geometry.empennage.vstab_root_chord_m" => (5.0, 4.31),
        "A220-300.geometry.empennage.vstab_tip_chord_m"
        | "preset_only.geometry.empennage.vstab_tip_chord_m" => (1.6, 2.0),
        "A220-300.geometry.empennage.vstab_tip_le_m[2]"
        | "preset_only.geometry.empennage.vstab_tip_le_m[2]" => (5.5, 6.193),
        "A220-300.geometry.empennage.vstab_z_m" | "preset_only.geometry.empennage.vstab_z_m" => {
            (1.0, 1.9605)
        }
        // NASA Common Research Model, AIAA 2008-6919 Table 2 (the widebody
        // wing NASA/TP-20210023843 Table I sets beside the 787-9): 0.115
        // area-weighted t/c, which the 12 % root section reproduces in a loft
        // that holds the root section to the kink; the 14 % section gave 0.133.
        "B787-9.geometry.wing.root_airfoil" => return Some((json!("sc20614"), json!("sc20612"))),
        // Tail section thickness read off the manufacturer front views: A340 AC Rev 33
        // FIGURE-2-2-0-991-007-A01 sheet 1, HTP t/c 0.10; A380 AC Rev 20 FIGURE-2-2-0-991-001-A01
        // sheet 1, HTP and fin depth over chord about 0.10; 787 ACAP D6-58333 Rev Q 2.2.2,
        // tailplane depth 0.46 m at y 3.2 m. The 12 % section drew each about 0.1 m too deep.
        "A340-300.geometry.empennage.tail_airfoil"
        | "A380-800.geometry.empennage.tail_airfoil"
        | "B787-9.geometry.empennage.tail_airfoil"
        | "preset_then_field.geometry.empennage.tail_airfoil" => {
            return Some((json!("naca0012"), json!("naca0010")))
        }
        // The DC-10's Douglas DSMA-496/-521/-522 root and DSMA-519/-520 tip
        // sections (Lednicer, "The Incomplete Guide to Airfoil Usage")
        // predate the supercritical sections and are unpublished; Obert,
        // "Aerodynamic Design of Transport Aircraft" (2009), p. 207, prints
        // them as peaky sections with very little rear loading and draws the
        // basic one (Fig. 24.23, p. 208, from AIAA paper 69-830). That
        // section, digitised as DC10-BASIC, replaces the drawn aft-loaded
        // SC(2)-0612/-0410 pair at root and tip; the thickness stations set
        // the t/c.
        "DC-10.geometry.wing.root_airfoil" => return Some((json!("sc20612"), json!("DC10-BASIC"))),
        "DC-10.geometry.wing.tip_airfoil" => return Some((json!("sc20410"), json!("DC10-BASIC"))),
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
        // top line, so the fin root sits on the crown.
        "A320-200.geometry.empennage.vstab_z_m" => (1.2, 2.17),
        "A320-200.geometry.empennage.hstab_root_chord_m" => (4.0, 3.740),
        "A320-200.geometry.empennage.hstab_tip_chord_m" => (1.2, 1.24),
        "A320-200.geometry.empennage.hstab_tip_le_m[0]" => (3.5, 4.02),
        "A320-200.geometry.empennage.hstab_offset_from_tail_m" => (5.5, 6.06),
        "A320-200.geometry.empennage.hstab_tip_le_m[1]" => (6.0, 6.225),
        "A320-200.geometry.empennage.vstab_root_chord_m" => (5.2, 5.444),
        "A320-200.geometry.empennage.vstab_tip_chord_m" => (1.8, 1.884),
        "A320-200.geometry.empennage.vstab_tip_le_m[0]" => (5.0, 5.06),
        "A320-200.geometry.empennage.vstab_tip_le_m[2]" => (5.8, 5.87),
        "A320-200.geometry.fuselage.height_m" => return Some((Value::Null, json!(4.14))),
        "A320-200.geometry.wing.break_span_fraction" => (0.37, 0.379_3),
        // Airbus A320 AC Jun 01/24, FIGURE-2-3-0-991-029-A01 sheet 2 (MRW,
        // aft CG): sharklet bottom 4.009 m and fuselage bottom 1.792 m above
        // the ground set the 5.1 deg dihedral; the CFM56-5B nacelle low point
        // 0.577 m sets the engine axis under the lowered wing.
        "A320-200.geometry.wing.break_z_m" => (-0.2, -0.62),
        "A320-200.geometry.wing.tip_z_m" => (1.5, 0.33),
        "A320-200.geometry.engine.z_m" => (-1.71, -1.48),
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
        // ground over the 1.75 m forward-belly clearance, the fin root line
        // on the crown. The saved-file case
        // `preset_then_field` loads this preset.
        "B787-9.design_vector.fuselage_length_m" => (62.81, 62.00),
        // Section 2.2.2: body width 18 ft 11 in and height 19 ft 6 in.
        "B787-9.geometry.fuselage.diameter_m"
        | "preset_then_field.geometry.fuselage.diameter_m" => (5.94, 5.77),
        "B787-9.geometry.fuselage.height_m" | "preset_then_field.geometry.fuselage.height_m" => {
            return Some((Value::Null, json!(5.94)))
        }
        "B787-9.geometry.engine.z_m" | "preset_then_field.geometry.engine.z_m" => (-2.55, -1.828),
        "B787-9.geometry.empennage.hstab_offset_from_tail_m"
        | "preset_then_field.geometry.empennage.hstab_offset_from_tail_m" => (9.5, 9.75),
        "B787-9.geometry.empennage.hstab_tip_le_m[1]"
        | "preset_then_field.geometry.empennage.hstab_tip_le_m[1]" => (9.5, 9.905),
        // Fin positions fitted to the manufacturers' side-view drawings (A320 AC
        // Fig 2-2-0-991-004-A01, A220 ACP Fig 1 sheet 1, 787 Rev Q 2.2.2): the
        // frozen offsets put the fin 1.1-1.5 m too far aft, its tip trailing
        // edge past the tail cone.
        "A320-200.geometry.empennage.vstab_offset_from_tail_m" => (6.5, 7.7),
        "A220-300.geometry.empennage.vstab_tip_le_m[0]"
        | "preset_only.geometry.empennage.vstab_tip_le_m[0]" => (4.5, 5.38),
        "B787-9.geometry.empennage.hstab_tip_le_m[0]"
        | "preset_then_field.geometry.empennage.hstab_tip_le_m[0]" => (6.0, 8.76),
        "B787-9.geometry.empennage.vstab_root_chord_m"
        | "preset_then_field.geometry.empennage.vstab_root_chord_m" => (8.0, 7.5),
        "B787-9.geometry.empennage.vstab_tip_chord_m"
        | "preset_then_field.geometry.empennage.vstab_tip_chord_m" => (2.8, 2.4),
        "B787-9.geometry.empennage.vstab_tip_le_m[0]"
        | "preset_then_field.geometry.empennage.vstab_tip_le_m[0]" => (7.5, 8.5),
        "A220-300.geometry.empennage.vstab_offset_from_tail_m"
        | "preset_only.geometry.empennage.vstab_offset_from_tail_m" => (6.0, 8.64),
        "B787-9.geometry.empennage.vstab_offset_from_tail_m"
        | "preset_then_field.geometry.empennage.vstab_offset_from_tail_m" => (10.5, 11.19),
        "B787-9.geometry.empennage.vstab_tip_le_m[2]"
        | "preset_then_field.geometry.empennage.vstab_tip_le_m[2]" => (8.5, 9.12),
        "B787-9.geometry.empennage.vstab_z_m"
        | "preset_then_field.geometry.empennage.vstab_z_m" => (1.8, 3.17),
        // Nose tip heights of the drawing-fitted noses: the tip
        // height read from each manufacturer drawing; the frozen generic nose
        // droop was a single class estimate.
        "AVE.geometry.fuselage.nose_z_m" => (-0.5, 0.25),
        "A340-300.geometry.fuselage.nose_z_m" => (-0.4, 0.25),
        "A380-800.geometry.fuselage.nose_z_m" => (-0.6, -1.02),
        "B787-9.geometry.fuselage.nose_z_m" | "preset_then_field.geometry.fuselage.nose_z_m" => {
            (-0.4, -0.71)
        }
        "A320-200.geometry.fuselage.nose_z_m" => (-0.3, -0.57),
        "A220-300.geometry.fuselage.nose_z_m" | "preset_only.geometry.fuselage.nose_z_m" => {
            (-0.2, -1.01)
        }
        "DC-10.geometry.fuselage.nose_z_m" => (-0.4, 0.53),
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
/// lift-to-drag objective read, and the Korn technology factor the declared
/// airfoil class replaced. The load boundary drops the same keys.
pub fn retired_reference_key(path: &str, key: &str) -> bool {
    path.ends_with(".drag_model") && alas_config::RETIRED_DRAG_MODEL_KEYS.contains(&key)
        || path.ends_with(".solver") && alas_config::RETIRED_SOLVER_KEYS.contains(&key)
        || path.ends_with(".weights") && alas_config::RETIRED_WEIGHT_KEYS.contains(&key)
        || path.ends_with(".propulsion_cycle")
            && alas_config::RETIRED_PROPULSION_CYCLE_KEYS.contains(&key)
        || path.ends_with(".design_space") && alas_config::RETIRED_DESIGN_SPACE_KEYS.contains(&key)
}

/// Additive propulsion fields are validated by the active-binding unit tests.
pub fn native_field(path: &str, key: &str) -> bool {
    performance_fields::is_native_field(path, key)
        || (key == "toda_override_m" && !path.contains('.'))
        // The CS 25.337(b) switch that ties the ultimate load factor to the
        // design MTOW: a native product addition (default off, the frozen
        // declared value), checked by the `DesignRequirements` unit tests.
        || (key == "ultimate_load_factor_from_mtow"
            && (path.ends_with("DesignRequirements") || path.ends_with(".requirements")))
        // Lift-dependent viscous drag fraction: a native product addition
        // (default 0 = the frozen lumped-margin convention).
        || (key == "compressible_trim"
            && (path.ends_with("AnalysisConfig") || path.ends_with(".analysis")))
        || (key == "payload_items" && (path.ends_with("CabinConfig") || path.ends_with(".cabin")))
        // A freighter's main-deck cargo door height: a native, sourced addition.
        || (key == "main_deck_door_clear_height_m" && path.ends_with(".cargo"))
        // The freighter doors the mass ledger places: native, sourced additions.
        || (matches!(
            key,
            "main_deck_door_clear_width_m"
                | "main_deck_door_station_m"
                | "nose_cargo_door_width_m"
                | "nose_cargo_door_height_m"
        ) && path.ends_with(".cargo"))
        // The seats a converted freighter's systems keep: a native, sourced addition.
        || (key == "converted_freighter_seats" && path.ends_with(".cargo"))
        || (key == "cabin_section"
            && (path.ends_with("FuselageConfig") || path.ends_with(".fuselage")))
        || (key == "lift_dependent_viscous_fraction"
            && (path.ends_with("DragModelConfig") || path.ends_with(".drag_model")))
        || (matches!(
            key,
            "vstab_root_le_x_m"
                | "hstab_root_le_x_m"
                | "hstab_root_station_y_m"
                | "vstab_dorsal_length_m"
                | "vstab_dorsal_x_start_m"
                | "vstab_dorsal_height_m"
                | "vstab_dorsal_shoulder"
                | "vstab_dorsal_seat_rise_m"
                | "vstab_dorsal_in_aero"
                | "vstab_airfoil"
        ) && (path.ends_with("EmpennageConfig") || path.ends_with(".empennage")))
        || (matches!(key, "fuel_policy" | "fuel_tanks" | "downstream") && !path.contains('.'))
        // Native rudder deflection limit of the engine-out yaw balance.
        || (key == "rudder_max_deflection_deg"
            && (path.ends_with("ControlSurfacesConfig") || path.ends_with(".control_surfaces")))
        // Native inboard aileron run (zero chord: none).
        || (key.starts_with("inboard_aileron_")
            && (path.ends_with("ControlSurfacesConfig") || path.ends_with(".control_surfaces")))
        // The drawn tail-engine pod of a trijet; the frozen schema places the
        // centreline engine by a rule (`None` keeps that rule).
        || (key == "centre_engine" && (path.ends_with("EngineConfig") || path.ends_with(".engine")))
        || (key == "nacelle_model" && (path.ends_with("EngineConfig") || path.ends_with(".engine")))
        || (key == "z_per_engine_m" && (path.ends_with("EngineConfig") || path.ends_with(".engine")))
        || (key == "inlet_x_per_engine_m" && (path.ends_with("EngineConfig") || path.ends_with(".engine")))
        // Optional belly-upsweep length; the frozen schema has no such field.
        || (key == "belly_upsweep_length_m"
            && (path.ends_with("FuselageConfig") || path.ends_with(".fuselage")))
        // Optional shaped-nose parameters of the fuselage; the frozen
        // Python schema has no such fields (all unset = ellipsoid nose).
        || (key.starts_with("nose_")
            && key != "nose_z_m"
            && (path.ends_with("FuselageConfig") || path.ends_with(".fuselage")))
        // Optional upper-deck hump and upper deck (all unset = no hump).
        || ((key.starts_with("hump_") || key.starts_with("upper_deck_"))
            && (path.ends_with("FuselageConfig") || path.ends_with(".fuselage")))
        // Optional shaped aft body (all unset = tailcone law).
        || ((key.starts_with("aft_") || key.starts_with("tail_end_") || key == "hold_interior_end_x_m")
            && (path.ends_with("FuselageConfig") || path.ends_with(".fuselage")))
        // Optional lofted outer surface (hidden; unset = parametric laws).
        || (key == "loft"
            && (path.ends_with("FuselageConfig") || path.ends_with(".fuselage")))
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
                    | "mlg_tire"
                    | "nlg_tire"
                    // Dynamic nose-braking, tip-back and
                    // ground-clearance additions the frozen configuration
                    // has no equivalent field for at all.
                    | "nlg_dynamic_braking_decel_g"
                    | "tire_dynamic_rating_factor"
                    | "min_tip_back_deg"
                    | "required_rotation_angle_deg"
                    | "required_rotation_basis"
                    | "fuselage_ground_clearance_m"
                    // The rotation and landing-trim forward-CG
                    // criteria's own conceptual-design inputs. The frozen
                    // configuration has no field for them at all.
                    | "rotation_pitch_acceleration_deg_s2"
                    | "pitch_radius_of_gyration_frac_mac"
                    | "takeoff_stabilizer_nose_up_deg"
                    | "trimmable_stabilizer"
                    | "stabilizer_nose_up_travel_deg"
                    | "elevator_up_travel_deg"
                    | "cl_ground_attitude_frac_of_cl_max_to"
                    | "rotation_rolling_friction_coefficient"
            ))
        // Spanwise dihedral and thickness stations and the swept-back tip of the
        // main wing: native additions, omitted from serialization when unset;
        // their behaviour is pinned by the wing-profile
        // tests in `alas-geom` and `alas-config`.
        || (matches!(
            key,
            "dihedral_stations"
                | "thickness_stations"
                | "section_stations"
                | "tip_rake_start_span_fraction"
                | "te_break_span_fraction"
                | "te_break_chord_offset_m"
                | "te_break_2_span_fraction"
                | "te_break_2_chord_offset_m"
                | "edge_stations"
                | "tip_rake_tip_chord_m"
                | "tip_rake_reference_tip_chord_m"
                | "wing_profile_interpolation"
                | "flight_root_rise_m"
                | "nacelles_hold_ground_height"
                | "wingtip_device"
                | "inboard_le_sweep_deg"
                | "reference_wing"
                | "drag_divergence_law"
        ) && (path.ends_with("WingConfig") || path.ends_with(".wing")))
        || (matches!(
            key,
            "hstab_edge_stations"
                | "vstab_edge_stations"
                | "vstab_thickness_stations"
                | "vstab_reference_end_fraction"
                | "ventral_fin_stations"
        )
            && (path.ends_with("EmpennageConfig") || path.ends_with(".empennage")))
        || (path.ends_with(".drag_model") && key == "schema_version")
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
        // The deployed high-lift chord extension the neutral-point high-lift
        // condition reads; the frozen configuration has no such field.
        || (path.ends_with(".performance") && key == "high_lift_chord_extension")
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
        || (path.ends_with(".mission.profile")
            && matches!(key, "climb_descent_speed_reference" | "climb_mach" | "descent_mach" | "max_operating_mach" | "max_operating_cas_m_s" | "vmo_schedule" | "mmo_schedule" | "vfe_landing_cas_m_s" | "takeoff_roll_time_s"))
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
        // The optional initial-cruise-altitude requirement has no frozen field;
        // every registered preset leaves it unset, so nothing is required of it.
        || (path.ends_with(".requirements")
            && matches!(key, "initial_cruise_altitude_m" | "initial_cruise_mass_kg"))
        // The optional maximum-landing-weight requirement has no frozen field;
        // every registered preset leaves it unset, so the declared MLW governs.
        || (path.ends_with(".requirements") && key == "mlw_kg")
        // The optional maximum-zero-fuel-weight requirement has no frozen field;
        // every registered preset leaves it unset, so the declared MZFW governs.
        || (path.ends_with(".requirements") && key == "mzfw_kg")
        // MMO as the design cruise Mach MC has no frozen field; the registry fills
        // it from each preset's declared operating limits (`design_speeds`).
        || (path.ends_with(".requirements") && key == "max_operating_mach")
        // The CS 25.335(b)(1)(ii) flag has no frozen field; the registry sets it
        // from each preset's sourced certification basis (`high_speed_protection`).
        || (path.ends_with(".requirements") && key == "high_speed_protection")
}

/// Cargo deck ULD formats: both
/// decks select their unit-load-device format automatically to maximise cargo
/// and space occupation, so the product default is "AUTO" where the frozen
/// Python reference fixed PMC / LD3. Returns (frozen, product); a preset that
/// declares its own format (the A400M's "463L" main deck, a declared lower
/// hold) keeps that declaration as the product value.
pub fn cargo_deck_uld(path: &str) -> Option<(Value, Value)> {
    let (case, field) = path.split_once('.')?;
    let frozen = match field {
        "cabin.cargo.main_deck_uld" => "PMC",
        "cabin.cargo.lower_deck_uld" => "LD3",
        _ => return None,
    };
    let name = match case {
        "preset_only" => "A220-300",
        "preset_then_field" => "B787-9",
        name => name,
    };
    let cargo = match alas_config::presets::get(name) {
        Ok(preset) => preset.planning_cabin_config().cargo,
        Err(_) => alas_config::CargoDeckConfig::default(),
    };
    let product = if field.ends_with("main_deck_uld") {
        cargo.main_deck_uld
    } else {
        cargo.lower_deck_uld
    };
    Some((json!(frozen), json!(product)))
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
        // The cruise speeds follow the requirement's Mach (one source of
        // truth): the `preset_then_field` document edits it to 0.80.
        if case == "preset_then_field"
            && key.starts_with("cruise_")
            && key.ends_with("_air_speed_m_s")
        {
            let sound = alas_atmo::us1976_compute_values(defaults.cruise_altitude_m, 0.0)
                .speed_of_sound_m_s;
            return Some(json!(0.80 * sound));
        }
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

/// The design dive speed a preset derives from its declared VMO/MMO by the CS 25.335(b) rule of
/// its certification basis (`alas_config::design_speeds`), replacing the frozen hand-set VD. The
/// values are pinned against an independent mirror by the `design_speeds` unit tests, not here.
pub fn design_dive_speed(path: &str) -> Option<Value> {
    let case = path.strip_suffix(".requirements.dive_speed_m_s")?;
    let name = match case {
        "preset_only" => "A220-300",
        "preset_then_field" => "B787-9",
        name => name,
    };
    let preset = alas_config::presets::get(name).ok()?;
    let profile = preset.operational_mission_defaults().profile;
    alas_config::design_speeds::dive_speed_from_operating_limits_for(
        &profile,
        preset.requirements.upset_criterion(),
    )?;
    Some(json!(preset.requirements.dive_speed_m_s))
}

/// Lower holds a preset declares by station (`presets::declared_lower_holds`):
/// the frozen configuration carries no declared hold compartments.
pub fn declared_lower_holds(path: &str) -> Option<Value> {
    let case = path.strip_suffix(".cabin.cargo.hold_compartments")?;
    let name = match case {
        "preset_only" => "A220-300",
        "preset_then_field" => "B787-9",
        name => name,
    };
    serde_json::to_value(alas_config::presets::declared_lower_holds(name)?).ok()
}

/// Source-constrained side-of-body corrections have no Python counterpart.
pub fn added_planform(path: &str) -> Option<Value> {
    match path {
        // Airbus A320 AC FIGURE-2-2-0-991-004-A01 plan view, side-of-body chord
        // before the fillet (was 0.852 98).
        "A320-200.geometry.wing.side_of_body_chord_ratio" => Some(json!(0.8726)),
        // Airbus A340-200/-300 AC Rev 33 FIGURE-2-2-0-991-007-A01 sheet 2:
        // 10.5 m printed from the LE line to the TE at the fuselage side.
        "A340-300.geometry.wing.side_of_body_chord_ratio" => Some(json!(0.874)),
        // Airbus A380 AC Rev 20, Figure 2-2-0-991-001-A01 sheet 2: 17.68 m
        // (printed 17.67 m) on the drawn inboard LE line over the root datum
        // (was 0.897 945 on the 19.678 m datum).
        "A380-800.geometry.wing.side_of_body_chord_ratio" => Some(json!(0.8985)),
        "DC-10.geometry.wing.side_of_body_chord_ratio" => Some(json!(0.925_625_096_354_822)),
        // A220 ACP Fig 1 plan view: TE x 19.90 m less the LE line 13.768 m =
        // 6.132 m over the 7.125 m datum chord (was 0.8595).
        "A220-300.geometry.wing.side_of_body_chord_ratio"
        | "preset_only.geometry.wing.side_of_body_chord_ratio" => Some(json!(0.8606)),
        "A320-200.geometry.wing.kink_span_fraction" => Some(json!(0.379_3)),
        "A320-200.geometry.wing.side_of_body_span_fraction" => Some(json!(0.115_84)),
        // Airbus A380 AC Rev 20, Figure 2-2-0-991-001-A01: half the 7.14 m body.
        "A380-800.geometry.wing.side_of_body_span_fraction" => Some(json!(3.57 / 39.875)),
        // Boeing 787 ACAP D6-58333 Rev Q section 2.2.2 plan view: half the
        // printed 18 ft 11 in (5.77 m) body over the 30.06 m semispan; the
        // drawn inboard trailing edge 33.71 m there, 11.895 m from the built
        // leading edge, over the 13.62 m root datum (was the derived chord
        // at the class 0.10).
        "B787-9.geometry.wing.side_of_body_span_fraction" => Some(json!(2.885 / 30.06)),
        "B787-9.geometry.wing.side_of_body_chord_ratio" => Some(json!(0.8734)),
        // Airbus A380 AC Rev 20, Figure 2-2-0-991-001-A01 sheet 1 (front
        // view): wing centre at the fence 1.48 m against the 1.46 m tip_z,
        // 0.02 m over the 39.875 m semispan (was 0.002 733, a
        // held-flight balance value). The over 4 m take-off flex of Airbus A380
        // Facts and Figures (February 2022) is validated, never lofted.
        "A380-800.geometry.wing.flight_tip_rise_semispan_fraction" => Some(json!(0.0005)),
        // Geometry round 2: the static ground shape stays at its published clearances and the
        // flight shape carries the drawing fit (root and tip rises, interior station rises).
        // Drawing passes: A320 AC FIGURE-2-3-0-991-029-A01 tip
        // clearance against the fitted line (was 0.016 681);
        // A340 AC FIGURE-2-2-0-991-007-A01 sheet 1 front-view upper line, 4.37 deg
        // (was 0.013 999).
        "A320-200.geometry.wing.flight_tip_rise_semispan_fraction" => Some(json!(0.02372)),
        "A340-300.geometry.wing.flight_tip_rise_semispan_fraction" => Some(json!(0.0069)),
        _ => None,
    }
}

/// The declared section generation, which replaced the frozen global
/// `drag_model.korn_technology_factor` and so has no Python counterpart. The
/// class follows the Shevell-Kroo crest-critical Mach method (Kroo,
/// AA241 notes sec. 4.3; Shevell 1980; Torenbeek 1982) and the
/// era of the wing: peaky for the 1960s DC-10 and 747, supercritical for the
/// A320 and A330/A340 wings, advanced supercritical for wings first flown from
/// 2000 on (A380, 787, A350, A220, E2, C919, A400M and the clean-sheet AVE),
/// conventional for the non-transonic ATR 72 (each preset's
/// `airfoil_class_source`).
pub fn declared_airfoil_class(path: &str) -> Option<Value> {
    let aircraft = path.strip_suffix(".geometry.wing.airfoil_class")?;
    Some(json!(match aircraft {
        "ATR72-600" => "conventional",
        "DC-10" | "B747-400" => "peaky",
        "AVE" | "A380-800" | "B787-9" | "A220-300" | "A350-900" | "E195-E2" | "C919" | "A400M"
        | "preset_only" | "preset_then_field" => "supercritical_advanced",
        _ => "supercritical",
    }))
}
