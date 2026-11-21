// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Preset geometry re-read from the manufacturer drawings after geometry phase 1,
//! as `(frozen reference, current)`.
//!
//! Every row names the drawing it was read from, as the preset's own comment states it. Units: metres unless
//! the path says otherwise.
//!
//! Deliberately absent, so they stay on their earlier entries and their checks
//! fail until a built-overlay review settles them:
//! - DC-10 tailplane root and tip chords 8.53 / 2.93 m and wing
//!   datum 22.594 m; the DC-10 wing chords are held in
//!   the source-correction ledger with them.
//! - A380-800 tailplane tip chord 3.80 m against the printed 3.72 m.
//! - A220-300 fin offset 7.69 m, root chord 5.09 m and tip chord 1.80 m from a
//!   work-in-progress fit (its own comment reads a drawn 5.25 m root chord; the
//!   overlay study read fin LE 30.1 m and tip chord 2.0 m).
//! - B787-9 tailplane tip chord 1.43 m against the 1.49 m reading,
//!   and the tip leading edge 8.58 m derived from it (tip TE held at 62.81 m).
//! - A320-200 break and tip chords 3.70 / 1.58 m (no printed value),
//!   with the A320 span conflict of `preset_dimension_corrections`.
//!
//! `moved(path, base)` keeps the frozen side of the earlier ledger entry and
//! replaces only the current-preset side, or records the pair for paths that
//! had no entry (frozen side = the frozen reference fixture value).

// Each integration-test binary compiles only the helpers its fixture needs.
#![allow(dead_code)]

/// `(path, frozen value when the path had no earlier entry, current value, commit: source)`.
#[rustfmt::skip]
const MOVED: &[(&str, Option<f64>, f64, &str)] = &[
    // Airbus A340-200/-300 AC Rev 33, FIGURE-2-2-0-991-007-A01 (vector silhouette).
    ("A340-300.design_vector.root_chord_m", None, 12.08, "5d4d38db8: sheet 2, inboard TE line carried to y = 0"),
    ("A340-300.design_vector.break_chord_m", None, 7.408, "5d4d38db8: sheet 2, kink at the TE break y 9.25 m"),
    ("A340-300.design_vector.tip_chord_m", None, 2.5, "5d4d38db8: sheet 2 prints 2.5 m at the winglet root"),
    ("A340-300.design_vector.sweep_deg", None, 32.0, "5d4d38db8: sheet 2, one straight LE, slope 0.6247"),
    ("A340-300.geometry.wing.root_datum_x_m", None, 22.487, "5d4d38db8: sheet 2, straight LE extended to y = 0"),
    ("A340-300.geometry.wing.root_z_m", Some(-1.8), -1.285, "8a35bb7d9: sheet 1 front-view line less the linear tip rise (Fig. 2-3-0-991-005-A01 W2)"),
    ("A340-300.geometry.wing.break_z_m", None, -0.2909, "8a35bb7d9: ground shape at the 9.25 m kink"),
    ("A340-300.geometry.empennage.hstab_offset_from_tail_m", None, 7.58, "e301d05a8: body ends at the printed 62.84 m, tailplane station kept"),
    ("A340-300.geometry.empennage.hstab_z_m", None, 1.46, "eb7c7b36d: sheet 1 side and front views at the 10 % section"),
    ("A340-300.geometry.empennage.vstab_offset_from_tail_m", None, 10.37, "e301d05a8: body ends at the printed 62.84 m, fin station kept"),
    ("A340-300.geometry.empennage.vstab_tip_chord_m", None, 3.10, "e301d05a8: printed tip chord, sheet 1"),
    ("A340-300.geometry.empennage.vstab_tip_le_m[0]", None, 8.09, "e301d05a8: printed tip LE 60.56 m, sheet 1"),
    ("A340-300.geometry.empennage.vstab_tip_le_m[2]", None, 8.30, "e301d05a8: 8.3 m of fin above the crown, sheet 1"),
    ("A340-300.geometry.engine.inlet_x_offset_m", None, 4.36, "94ad129e4: sheet 2 inlets 22.39 m and 28.96 m against the 32.0 deg LE"),
    ("A340-300.geometry.fuselage.tailcone_length_m", Some(12.0), 11.18, "e301d05a8: A340-200/-300 AC 2-2-0 sheet 1 printed 62.84 m body length, tail-cone start held at 51.66 m (63.66 - 12.0), so 62.84 - 51.66 m"),
    // Airbus A380 AC Rev 20, FIGURE-2-2-0-991-001-A01.
    ("A380-800.design_vector.sweep_deg", None, 36.31, "887c224ec: sheet 2 vector outline, outboard LE"),
    ("A380-800.geometry.wing.root_datum_x_m", None, 23.85, "887c224ec: A380 AC Rev 20 Fig 2-2-0-991-001-A01 sheet 2 vector outline, inboard LE 39.55 deg through 20.80 m at y 5.39 m carried to y = 0: x 16.35 m from the nose = root datum + the -7.5 m wing_x_shift"),
    ("A380-800.geometry.wing.break_z_m", None, -0.3399, "8a35bb7d9: ground shape = flight line less the linear tip rise"),
    ("A380-800.geometry.wing.tip_z_m", None, 1.46, "887c224ec: sheet 1 front view"),
    ("A380-800.geometry.empennage.hstab_offset_from_tail_m", None, 13.09, "e164e5950: body ends at the printed 70.40 m tail cut, tailplane station kept"),
    ("A380-800.geometry.empennage.hstab_tip_le_m[2]", None, 1.70, "887c224ec: tailplane centre line, 6.2 deg dihedral"),
    ("A380-800.geometry.empennage.hstab_z_m", None, 1.05, "b9774f6a2: sheet 1 side outboard lines and front surfaces"),
    ("A380-800.geometry.empennage.vstab_offset_from_tail_m", None, 16.46, "e164e5950: body ends at the printed 70.40 m tail cut, fin station kept"),
    ("A380-800.geometry.empennage.vstab_root_chord_m", None, 12.07, "887c224ec: sheet 1 side view"),
    ("A380-800.geometry.empennage.vstab_tip_chord_m", None, 4.60, "887c224ec: sheet 1 side view"),
    ("A380-800.geometry.empennage.vstab_tip_le_m[0]", None, 14.21, "887c224ec: sheet 1 side view"),
    ("A380-800.geometry.engine.inlet_x_offset_m", None, 6.41, "d7010707f: nacelle forward faces at the printed stations"),
    ("A380-800.geometry.fuselage.tailcone_length_m", Some(15.0), 12.67, "e164e5950: A380 AC Rev 20 Fig 2-2-0-991-001-A01 sheet 1 printed 70.40 m tail cut, no exhaust cone; tail-cone start held at 57.73 m (72.73 - 15.0), so 70.40 - 57.73 m"),
    // Boeing 787 ACAP D6-58333 Rev Q, section 2.2.2 general dimensions.
    ("B787-9.design_vector.root_chord_m", None, 13.62, "0dad0dd6a: datum chord, side-of-body chord 11.76 m"),
    ("B787-9.design_vector.break_chord_m", None, 6.75, "0dad0dd6a: LE 27.52 m, TE 34.27 m on the drawn rounded kink"),
    ("B787-9.design_vector.tip_chord_m", Some(1.6), 2.67, "0dad0dd6a: printed extreme tip, LE 40.69 m, TE 43.36 m"),
    ("B787-9.geometry.wing.root_datum_x_m", None, 21.314, "0dad0dd6a: one straight LE line"),
    ("B787-9.geometry.wing.root_z_m", None, -1.371, "d632e7c4c: height-line datum, panels inside the body flat at the side-of-body height"),
    ("B787-9.geometry.wing.break_z_m", None, 0.063, "0dad0dd6a: front view upper and lower lines at y = 11 m"),
    ("B787-9.geometry.wing.tip_z_m", None, 2.62, "0dad0dd6a: front view, upper line at the tip"),
    ("B787-9.geometry.empennage.hstab_offset_from_tail_m", None, 9.39, "0dad0dd6a: plan view"),
    ("B787-9.geometry.empennage.hstab_root_chord_m", None, 6.32, "0dad0dd6a: plan view"),
    ("B787-9.geometry.empennage.hstab_tip_le_m[2]", None, 1.40, "70d30985b: between the front and side views"),
    ("B787-9.geometry.empennage.vstab_offset_from_tail_m", None, 10.79, "0dad0dd6a: side view, tip trailing point at the printed 62.00 m"),
    ("B787-9.geometry.empennage.vstab_root_chord_m", None, 7.52, "0dad0dd6a: side view, TE at the root line"),
    ("B787-9.geometry.empennage.vstab_tip_le_m[2]", None, 8.45, "0dad0dd6a: side view"),
    ("B787-9.geometry.engine.inlet_x_offset_m", None, 5.888, "0dad0dd6a: plan view, inlet face 20.80 m aft of the nose"),
    ("B787-9.geometry.engine.z_m", None, -1.96, "0dad0dd6a: engines lowered to the drawn heights"),
    // Airbus A320 AC (Jun 01/24 - Jul 01/26), FIGURE-2-2-0-991-004-A01.
    ("A320-200.geometry.wing.root_datum_x_m", None, 11.958, "ca2b0f753: plan view, inboard LE crank removed"),
    ("A320-200.geometry.wing.root_z_m", Some(-1.2), -0.9468, "8a35bb7d9: ground shape = flight line less the linear tip rise"),
    ("A320-200.geometry.wing.break_z_m", None, -0.4391, "8a35bb7d9: ground shape = flight line less the linear tip rise"),
    ("A320-200.geometry.empennage.hstab_offset_from_tail_m", None, 5.874, "ca2b0f753: sheet 4 plan view, vector straight edges"),
    ("A320-200.geometry.empennage.hstab_root_chord_m", None, 3.833, "ca2b0f753: sheet 4 plan view, edges at y = 0"),
    ("A320-200.geometry.empennage.hstab_tip_chord_m", None, 1.185, "60936ed4c: sheet 4 plan view, drawn tip chord"),
    ("A320-200.geometry.empennage.hstab_tip_le_m[0]", None, 3.979, "ca2b0f753: sheet 4 plan view"),
    ("A320-200.geometry.empennage.hstab_z_m", Some(0.8), 0.700, "60936ed4c: sheet 3 front view, upper surface at eta 0.55-0.85"),
    ("A320-200.geometry.empennage.vstab_offset_from_tail_m", None, 7.65, "60936ed4c: sheet 3 side view, vector LE and TE lines"),
    ("A320-200.geometry.empennage.vstab_tip_chord_m", None, 1.77, "ca2b0f753: side view"),
    ("A320-200.geometry.empennage.vstab_tip_le_m[0]", None, 5.04, "ca2b0f753: side view"),
    ("A320-200.geometry.empennage.vstab_z_m", None, 2.06, "60936ed4c: side view, drawn tip z 7.945 m, 5.87 m height kept"),
    ("A320-200.geometry.engine.inlet_x_offset_m", None, 3.31, "ca2b0f753: sheets 3-4 vector, lip x 11.56-11.60 m"),
    ("A320-200.geometry.engine.z_m", None, -1.28, "ca2b0f753: sheet 3 cowl, axis z -1.96 m"),
    // Airbus A220 ACP Issue 013, DM BD500-A-J06-10-00-00AAA-030A-A, Figure 1.
    ("A220-300.design_vector.span_m", Some(35.1), 34.984, "aff855c59: ACP Table 3, locator E, 1377.32 in baseline span"),
    ("A220-300.design_vector.root_chord_m", None, 7.125, "6f72f8b2f: ACP Fig 1 plan view"),
    ("A220-300.design_vector.break_chord_m", None, 3.58, "6f72f8b2f: plan view, TE corner at y 6.32 m"),
    ("A220-300.design_vector.tip_chord_m", Some(1.1), 1.275, "6f72f8b2f: plan view, straight-edge tip chord"),
    ("A220-300.geometry.wing.break_span_fraction", None, 0.36, "6f72f8b2f: plan view, drawn TE corner y 6.32 m"),
    ("A220-300.geometry.wing.root_datum_x_m", None, 12.775, "6f72f8b2f: plan view LE line through 14.01 m at y 2.2 m"),
    ("A220-300.geometry.wing.root_z_m", None, -1.13, "6f72f8b2f: front view, line of the 0.10 and 0.20 stations"),
    ("A220-300.geometry.wing.break_z_m", None, -0.25, "6f72f8b2f: front view, crown-registered upper surface"),
    ("A220-300.geometry.wing.tip_z_m", None, 2.30, "6f72f8b2f: front view"),
    ("A220-300.geometry.empennage.hstab_offset_from_tail_m", None, 6.31, "f9709382f: plan view, tip TE 37.82 m (locator H)"),
    ("A220-300.geometry.empennage.hstab_tip_le_m[2]", None, 0.77, "6f72f8b2f: front view tailplane dihedral"),
    ("A220-300.geometry.empennage.hstab_z_m", None, 0.66, "6f72f8b2f: side and crown-registered front views"),
    ("A220-300.geometry.empennage.vstab_tip_le_m[0]", None, 5.08, "6f72f8b2f: side view fin LE and TE rows"),
    ("A220-300.geometry.engine.inlet_x_offset_m", None, 3.681, "6f72f8b2f: LE at the engine 15.851 m less the lip station"),
    ("A220-300.geometry.engine.z_m", None, -1.27, "6f72f8b2f: front and side views, printed clearance G 0.582 m"),
    // Boeing DC/MD-10 ACAP DAC-67803A Rev A, Figure 2.2.
    ("DC-10.design_vector.sweep_deg", None, 38.31, "b36af3a7f: plan view, LE slope 0.790"),
    ("DC-10.design_vector.fuselage_length_m", Some(55.35), 51.97, "3a9958a2c: printed 170 ft 6 in hull; 181 ft 7.2 in is the pod nozzle"),
    ("DC-10.geometry.wing.break_span_fraction", None, 0.379, "b36af3a7f: Yehudi TE kink y 9.55 m"),
    ("DC-10.geometry.wing.break_z_m", None, -0.677, "b36af3a7f: front view LE heights"),
    ("DC-10.geometry.wing.tip_z_m", None, 0.45, "b36af3a7f: front view LE heights"),
    ("DC-10.geometry.empennage.hstab_offset_from_tail_m", None, 9.42, "3a9958a2c: tailplane root LE 42.55 m aft of the nose"),
    ("DC-10.geometry.empennage.hstab_tip_le_m[0]", None, 9.24, "b36af3a7f: plan view"),
    ("DC-10.geometry.empennage.hstab_tip_le_m[2]", None, 1.85, "b36af3a7f: front view"),
    ("DC-10.geometry.empennage.hstab_z_m", None, 0.40, "b36af3a7f: side view"),
    ("DC-10.geometry.empennage.vstab_offset_from_tail_m", None, 10.53, "3a9958a2c: fin tip held at 50.74 m on the 51.97 m hull"),
    ("DC-10.geometry.empennage.vstab_root_chord_m", None, 8.40, "033aefac1: fin refit to the ACAP drawing"),
    ("DC-10.geometry.empennage.vstab_tip_chord_m", None, 2.80, "033aefac1: fin refit to the ACAP drawing"),
    ("DC-10.geometry.empennage.vstab_tip_le_m[0]", None, 9.30, "9a6a69e1c: fin root pulled forward to the drawing"),
    ("DC-10.geometry.engine.inlet_x_offset_m", None, 5.85, "033aefac1: nacelle refit to the ACAP drawing"),
    ("DC-10.geometry.engine.z_m", None, -1.872, "d0ae19864: side and front views, Section 2.3 clearance N 0.86-1.09 m"),
    ("DC-10.geometry.fuselage.tailcone_length_m", Some(11.5), 8.12, "3a9958a2c: cabin end held at 43.85 m on the printed hull"),
];

/// Saved-file and loading fixtures load these presets under their fixture names.
fn canonical(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("preset_only.") {
        format!("A220-300.{rest}")
    } else if let Some(rest) = path.strip_prefix("preset_then_field.") {
        format!("B787-9.{rest}")
    } else {
        path.to_owned()
    }
}

/// The drawing-fit value of `path`: `(frozen reference, current preset value)`.
///
/// `base` is the earlier ledger entry of the path, if any; its frozen side is kept.
pub fn moved(path: &str, base: Option<(f64, f64)>) -> Option<(f64, f64)> {
    let canonical = canonical(path);
    let (_, frozen, current, _) = MOVED.iter().find(|(name, ..)| *name == canonical)?;
    let frozen = match (frozen, base) {
        (Some(frozen), _) => *frozen,
        (None, Some((old, _))) => old,
        // A saved-file alias whose fixture keeps its own ledger for this path
        // (see `wing_round2::lookup`) is not moved here.
        (None, None) if canonical != path => return None,
        (None, None) => {
            panic!("{path}: a moved value needs an earlier ledger entry or a recorded frozen value")
        }
    };
    Some((frozen, *current))
}
