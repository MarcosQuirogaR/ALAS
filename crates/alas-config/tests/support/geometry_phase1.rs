// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Preset geometry values moved by geometry phase 1 (2026-10-10 .. 2026-10-12), re-pinned 2026-10-13.
//!
//! The owner-accepted changes are the fuselage and nacelle lofts (V2 nacelles: engine offsets,
//! inlet positions and heights), the tail planforms fitted to the manufacturer drawings (root and
//! tip chords, offsets from the tail tip, tip leading-edge stations, fin height), the wing station
//! fits (root and break chords, break span fractions, tip heights including the raked tips) and
//! the A320 sharklet span including the device (35.8 m published; the planar wing proper is
//! 34.108 m). Each value was read from the failing parity run of the integration tip after the
//! geometry freeze (`p2-repin-2026-10-13.md` lists old and new values with units: metres unless the
//! path says otherwise, dimensionless fractions for `*_fraction`).
//!
//! `moved(path, base)` keeps the frozen (reference) side of an earlier ledger entry and replaces
//! only the current-preset side, or records the pair for paths that had no entry.

// Each integration-test binary compiles only the helpers its fixture needs.
#![allow(dead_code)]

/// `(path, frozen reference value when the path had no earlier ledger entry, current value)`.
const MOVED: &[(&str, Option<f64>, f64)] = &[
    ("A220-300.design_vector.break_chord_m", None, 3.7),
    ("A220-300.design_vector.root_chord_m", Some(5.8), 7.071),
    (
        "A220-300.geometry.empennage.hstab_offset_from_tail_m",
        None,
        6.4,
    ),
    ("A220-300.geometry.empennage.hstab_root_chord_m", None, 3.43),
    ("A220-300.geometry.empennage.hstab_tip_chord_m", None, 1.28),
    ("A220-300.geometry.empennage.hstab_tip_le_m[0]", None, 4.27),
    (
        "A220-300.geometry.empennage.vstab_offset_from_tail_m",
        None,
        8.61,
    ),
    ("A220-300.geometry.empennage.vstab_root_chord_m", None, 5.64),
    ("A220-300.geometry.empennage.vstab_tip_chord_m", None, 1.97),
    ("A220-300.geometry.empennage.vstab_tip_le_m[0]", None, 5.68),
    ("A220-300.geometry.empennage.vstab_tip_le_m[2]", None, 5.89),
    ("A220-300.geometry.engine.inlet_x_offset_m", None, 3.8371),
    ("A220-300.geometry.engine.z_m", None, -1.281),
    (
        "A220-300.geometry.wing.break_span_fraction",
        Some(0.37),
        0.34,
    ),
    ("A220-300.geometry.wing.break_z_m", None, -0.3461),
    ("A220-300.geometry.wing.tip_z_m", None, 2.15),
    ("A320-200.design_vector.span_m", None, 35.8),
    (
        "A320-200.geometry.empennage.hstab_offset_from_tail_m",
        None,
        6.04,
    ),
    ("A320-200.geometry.empennage.hstab_root_chord_m", None, 3.8),
    ("A320-200.geometry.empennage.hstab_tip_chord_m", None, 1.18),
    ("A320-200.geometry.empennage.hstab_tip_le_m[0]", None, 4.01),
    ("A320-200.geometry.empennage.hstab_tip_le_m[2]", None, 0.736),
    (
        "A320-200.geometry.empennage.vstab_offset_from_tail_m",
        None,
        7.66,
    ),
    ("A320-200.geometry.empennage.vstab_root_chord_m", None, 5.51),
    ("A320-200.geometry.empennage.vstab_tip_chord_m", None, 1.75),
    ("A320-200.geometry.engine.inlet_x_offset_m", None, 3.246),
    ("A320-200.geometry.engine.z_m", None, -1.33),
    ("A340-300.design_vector.break_chord_m", None, 7.7),
    ("A340-300.design_vector.root_chord_m", Some(12.0), 12.26),
    ("A340-300.design_vector.tip_chord_m", None, 2.35),
    (
        "A340-300.geometry.empennage.hstab_offset_from_tail_m",
        None,
        8.4,
    ),
    (
        "A340-300.geometry.empennage.hstab_root_chord_m",
        Some(6.5),
        5.36,
    ),
    ("A340-300.geometry.empennage.hstab_tip_chord_m", None, 2.0),
    (
        "A340-300.geometry.empennage.vstab_offset_from_tail_m",
        None,
        11.19,
    ),
    ("A340-300.geometry.empennage.vstab_root_chord_m", None, 7.79),
    ("A340-300.geometry.empennage.vstab_tip_chord_m", None, 3.08),
    ("A340-300.geometry.empennage.vstab_tip_le_m[0]", None, 8.105),
    ("A340-300.geometry.empennage.vstab_tip_le_m[2]", None, 8.42),
    ("A340-300.geometry.wing.break_z_m", None, -1.11),
    (
        "A380-800.geometry.empennage.hstab_offset_from_tail_m",
        None,
        15.42,
    ),
    ("A380-800.geometry.empennage.hstab_root_chord_m", None, 9.75),
    ("A380-800.geometry.empennage.hstab_tip_chord_m", None, 3.67),
    (
        "A380-800.geometry.empennage.vstab_root_chord_m",
        None,
        12.04,
    ),
    ("A380-800.geometry.empennage.vstab_tip_chord_m", None, 4.67),
    ("A380-800.geometry.empennage.vstab_tip_le_m[0]", None, 14.1),
    ("A380-800.geometry.engine.inlet_x_offset_m", None, 6.1183),
    ("A380-800.geometry.engine.z_m", None, -0.52),
    ("A380-800.geometry.wing.tip_z_m", None, 1.505),
    ("AVE.engine_spanwise_positions[0]", Some(9.8), 10.7),
    ("AVE.engine_spanwise_positions[1]", Some(-9.8), -10.7),
    (
        "AVE.geometry.empennage.hstab_offset_from_tail_m",
        Some(10.7),
        11.76,
    ),
    ("AVE.geometry.empennage.hstab_root_chord_m", Some(8.0), 6.98),
    ("AVE.geometry.empennage.hstab_tip_chord_m", Some(2.2), 1.66),
    ("AVE.geometry.empennage.hstab_tip_le_m[0]", Some(7.5), 9.94),
    (
        "AVE.geometry.empennage.hstab_tip_le_m[1]",
        Some(11.0),
        12.19,
    ),
    (
        "AVE.geometry.empennage.vstab_offset_from_tail_m",
        Some(12.2),
        13.63,
    ),
    ("AVE.geometry.empennage.vstab_root_chord_m", Some(9.5), 8.96),
    ("AVE.geometry.empennage.vstab_tip_chord_m", Some(3.2), 2.65),
    ("AVE.geometry.empennage.vstab_tip_le_m[0]", Some(9.0), 10.3),
    ("AVE.geometry.empennage.vstab_tip_le_m[2]", Some(9.8), 10.51),
    ("AVE.geometry.empennage.vstab_z_m", Some(2.0), 3.3),
    ("AVE.geometry.engine.inlet_x_offset_m", Some(4.2), 6.9),
    (
        "AVE.geometry.engine.spanwise_positions_m[0]",
        Some(9.8),
        10.7,
    ),
    (
        "AVE.geometry.engine.spanwise_positions_m[1]",
        Some(-9.8),
        -10.7,
    ),
    ("AVE.geometry.engine.z_m", Some(-3.13), -1.901),
    ("B787-9.design_vector.break_chord_m", None, 6.4692),
    ("B787-9.design_vector.root_chord_m", Some(12.6), 13.3663),
    (
        "B787-9.geometry.empennage.hstab_offset_from_tail_m",
        None,
        9.56,
    ),
    (
        "B787-9.geometry.empennage.hstab_root_chord_m",
        Some(6.5),
        6.57,
    ),
    (
        "B787-9.geometry.empennage.hstab_tip_chord_m",
        Some(1.8),
        1.75,
    ),
    ("B787-9.geometry.empennage.hstab_tip_le_m[0]", None, 8.47),
    (
        "B787-9.geometry.empennage.vstab_offset_from_tail_m",
        None,
        11.22,
    ),
    ("B787-9.geometry.empennage.vstab_root_chord_m", None, 7.46),
    ("B787-9.geometry.empennage.vstab_tip_chord_m", None, 2.28),
    ("B787-9.geometry.empennage.vstab_tip_le_m[0]", None, 8.53),
    ("B787-9.geometry.empennage.vstab_tip_le_m[2]", None, 8.55),
    ("B787-9.geometry.engine.z_m", None, -1.7153),
    ("B787-9.geometry.wing.tip_z_m", None, 2.65),
    (
        "DC-10.geometry.empennage.vstab_offset_from_tail_m",
        None,
        13.52,
    ),
    ("DC-10.geometry.empennage.vstab_root_chord_m", None, 7.72),
    ("DC-10.geometry.empennage.vstab_tip_chord_m", None, 2.84),
    ("DC-10.geometry.empennage.vstab_tip_le_m[0]", None, 8.87),
    ("DC-10.geometry.engine.inlet_x_offset_m", None, 6.25),
    ("DC-10.geometry.engine.z_m", None, -1.745),
    (
        "DC-10.geometry.wing.break_span_fraction",
        Some(0.35),
        0.3747,
    ),
    ("DC-10.geometry.wing.break_z_m", None, -0.6569),
    ("DC-10.geometry.wing.root_datum_x_m", None, 22.83),
    ("DC-10.geometry.wing.root_z_m", None, -1.5),
    ("DC-10.geometry.wing.tip_z_m", None, 0.75),
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

/// The geometry phase 1 value of `path`: `(frozen reference, current preset value)`.
///
/// `base` is the earlier ledger entry of the path, if any; its frozen side is kept.
pub fn moved(path: &str, base: Option<(f64, f64)>) -> Option<(f64, f64)> {
    let canonical = canonical(path);
    let (_, frozen, current) = MOVED.iter().find(|(name, _, _)| *name == canonical)?;
    let frozen = match (frozen, base) {
        (Some(frozen), _) => *frozen,
        (None, Some((old, _))) => old,
        (None, None) => {
            panic!("{path}: a moved value needs an earlier ledger entry or a recorded frozen value")
        }
    };
    Some((frozen, *current))
}
