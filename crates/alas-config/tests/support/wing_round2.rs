// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Preset geometry values moved by geometry round 2 (wing lane), as `(frozen reference, current)`.
//!
//! Sources and fits: the wing thickness distribution follows the NASA Common Research Model
//! (Vassberg et al., AIAA 2008-6919, Table 2); dihedral heights and the tailplane height and
//! dihedral are fitted to the upper edge of the manufacturer front views, the tail trailing-edge
//! sweeps to the plan views at the published tail areas, the wing tip chords close the published
//! reference area behind a swept-back tip, and each engine offset moves with the wing leading edge
//! so the nacelle keeps its height. The study reports carry the before/after measurements.

// Each integration-test binary compiles only the helpers its fixture needs.
#![allow(dead_code)]

/// The saved-file and loading fixtures load these presets: `preset_only` is the A220-300 and
/// `preset_then_field` the B787-9.
fn canonical(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("preset_only.") {
        format!("A220-300.{rest}")
    } else if let Some(rest) = path.strip_prefix("preset_then_field.") {
        format!("B787-9.{rest}")
    } else {
        path.to_owned()
    }
}

/// `(frozen reference value, current preset value)` of a path moved by geometry round 2.
#[allow(clippy::approx_constant)] // -3.14 m is the A380 engine height, not pi
pub fn lookup(path: &str) -> Option<(f64, f64)> {
    // The saved-file fixtures carry their own ledger for these two paths.
    if (path.starts_with("preset_only.") || path.starts_with("preset_then_field."))
        && (path.ends_with(".geometry.wing.root_datum_x_m")
            || path.ends_with(".geometry.engine.inlet_x_offset_m"))
    {
        return None;
    }
    Some(match canonical(path).as_str() {
        "A220-300.design_vector.break_chord_m" => (3.5, 3.5336599999999994),
        "A220-300.geometry.empennage.hstab_tip_le_m[2]" => (0.5, 0.71),
        "A220-300.geometry.empennage.hstab_z_m" => (0.7, 0.62),
        "A220-300.geometry.engine.z_m" => (-1.71, -1.211),
        "A220-300.geometry.wing.break_z_m" => (-0.2, -0.2598),
        "A220-300.geometry.wing.root_datum_x_m" => (13.3, 12.7791),
        "A220-300.geometry.wing.root_z_m" => (-1.0, -1.0651),
        "A220-300.geometry.wing.tip_z_m" => (1.5, 1.132),
        "A320-200.geometry.empennage.hstab_tip_le_m[2]" => (0.5, 0.67),
        "A320-200.geometry.wing.tip_z_m" => (1.5, 0.3262),
        "A340-300.design_vector.break_chord_m" => (6.5, 6.56232),
        "A340-300.design_vector.sweep_deg" => (30.0, 32.06841507580666),
        "A340-300.geometry.empennage.hstab_tip_le_m[2]" => (0.8, 1.08),
        "A340-300.geometry.empennage.hstab_z_m" => (1.0, 1.51),
        "A340-300.geometry.engine.inlet_x_offset_m" => (3.0, 4.2648),
        "A340-300.geometry.engine.z_m" => (-1.94, -1.2742),
        "A340-300.geometry.wing.break_z_m" => (-0.3, -1.01),
        "A340-300.geometry.wing.root_datum_x_m" => (22.0, 22.368),
        "A340-300.geometry.wing.tip_z_m" => (2.0, 1.2296),
        "A380-800.design_vector.sweep_deg" => (33.5, 36.37689299819862),
        "A380-800.geometry.empennage.hstab_tip_le_m[2]" => (1.2, 1.53),
        "A380-800.geometry.empennage.hstab_z_m" => (1.5, 0.96),
        "A380-800.geometry.engine.inlet_x_offset_m" => (4.5, 6.3683),
        "A380-800.geometry.engine.z_m" => (-3.14, -0.82),
        "A380-800.geometry.wing.root_datum_x_m" => (26.7, 23.6541), // step B w1: inboard LE crank, was 25.0368
        "A380-800.geometry.empennage.hstab_root_chord_m" => (9.0, 10.0), // step B w1: drawn root chord, was 9.0
        "A380-800.geometry.wing.tip_z_m" => (3.0, -0.9704),
        "B787-9.design_vector.break_chord_m" => (6.5, 6.554279999999997),
        "B787-9.design_vector.sweep_deg" => (32.2, 34.74104381154634),
        "B787-9.geometry.empennage.hstab_tip_le_m[2]" => (0.8, 1.19),
        "B787-9.geometry.empennage.hstab_z_m" => (1.0, 0.57),
        "B787-9.geometry.engine.inlet_x_offset_m" => (3.5, 6.1466),
        "B787-9.geometry.engine.z_m" => (-2.55, -1.828),
        "B787-9.geometry.wing.break_z_m" => (-0.2, -0.0866),
        "B787-9.geometry.wing.root_datum_x_m" => (21.0, 21.5737),
        "B787-9.geometry.wing.root_z_m" => (-1.8, -1.521),
        "B787-9.geometry.wing.tip_z_m" => (2.5, 2.3813),
        "DC-10.geometry.empennage.hstab_tip_le_m[2]" => (0.8, 1.75),
        "DC-10.geometry.empennage.hstab_z_m" => (1.0, 0.87),
        "DC-10.geometry.engine.z_m" => (-2.12, -1.659),
        "DC-10.geometry.wing.break_z_m" => (-0.3, -0.7528),
        "DC-10.geometry.wing.root_z_m" => (-1.6, -2.67),
        "DC-10.geometry.wing.tip_z_m" => (2.0, 0.9391),
        _ => return None,
    })
}
