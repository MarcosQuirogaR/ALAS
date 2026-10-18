// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The `UavSpec` field inventory: every scalar design parameter with its unit,
//! valid range and meaning. It is the UAV module's own inventory, independent
//! of `AlasConfig`, and the source of `docs/uav-spec-schema.md`.
//!
//! Paths use `[]` for list elements (`panels[].span_m`). Angles are degrees
//! because CAD parameter tables use degrees; the SI conversion happens where a
//! solver consumes them.

use super::layout::LayoutParams;
use super::master::{ControlSurface, EmpennageDriving, MasterParams, TailBoom, TailSurface};

/// One inventory row.
#[derive(Debug, Clone, Copy)]
pub struct ParamInfo {
    /// Path pattern.
    pub path: &'static str,
    /// Unit (`m`, `m2`, `deg`, `1`).
    pub unit: &'static str,
    /// Smallest valid value (inclusive).
    pub min: f64,
    /// Largest valid value (inclusive).
    pub max: f64,
    /// Meaning.
    pub doc: &'static str,
}

const fn p(
    path: &'static str,
    unit: &'static str,
    min: f64,
    max: f64,
    doc: &'static str,
) -> ParamInfo {
    ParamInfo {
        path,
        unit,
        min,
        max,
        doc,
    }
}

/// Valid-range table for the 0.5-25 kg, 0.9-4.5 m scope.
#[rustfmt::skip]
pub const PARAMS: &[ParamInfo] = &[
    p("root_chord_m", "m", 0.03, 1.5, "Root chord of the first panel"),
    p("panels[].span_m", "m", 0.02, 2.5, "Panel half-span along y"),
    p("panels[].taper_ratio", "1", 0.05, 1.5, "Tip chord over root chord of the panel"),
    p("panels[].sweep_le_deg", "deg", -30.0, 60.0, "Leading-edge sweep (positive aft)"),
    p("panels[].dihedral_deg", "deg", -20.0, 40.0, "Dihedral (positive tip up)"),
    p("panels[].twist_tip_deg", "deg", -10.0, 10.0, "Tip twist relative to root (positive nose-up)"),
    p("winglet.cant_deg", "deg", 0.0, 90.0, "Cant from vertical"),
    p("winglet.length_m", "m", 0.01, 0.8, "Winglet length"),
    p("winglet.root_chord_ratio", "1", 0.05, 1.0, "Winglet root chord over last-panel tip chord"),
    p("winglet.taper_ratio", "1", 0.05, 1.0, "Winglet tip chord over its root chord"),
    p("winglet.toe_deg", "deg", -10.0, 10.0, "Toe-in (positive nose-in)"),
    p("wing_xyz_m", "m", -5.0, 5.0, "Wing root leading-edge position component"),
    p("fuselage_xyz_m", "m", -5.0, 5.0, "Fuselage reference point component"),
    p("tail_xyz_m", "m", -5.0, 5.0, "Empennage mount position component"),
    p("tail_boom.length_m", "m", 0.1, 3.0, "Boom length"),
    p("tail_boom.outer_diameter_m", "m", 0.004, 0.08, "Boom outer diameter"),
    p("tail_boom.wall_m", "m", 0.0003, 0.01, "Boom wall thickness"),
    p("controls[].span_start", "1", 0.0, 1.0, "Inboard end, fraction of host half-span"),
    p("controls[].span_end", "1", 0.0, 1.0, "Outboard end, fraction of host half-span"),
    p("controls[].chord_fraction", "1", 0.05, 0.5, "Hinged chord fraction"),
    p("controls[].max_down_deg", "deg", 0.0, 60.0, "Maximum trailing-edge-down deflection"),
    p("controls[].max_up_deg", "deg", 0.0, 60.0, "Maximum trailing-edge-up deflection"),
    p("empennage.horizontal_volume", "1", 0.1, 1.5, "Horizontal tail volume coefficient (driven mode)"),
    p("empennage.vertical_volume", "1", 0.01, 0.2, "Vertical tail volume coefficient (driven mode)"),
    p("empennage.arm_m", "m", 0.1, 4.0, "Wing-to-tail aerodynamic-centre arm (driven mode)"),
    p("empennage.span_m", "m", 0.05, 2.5, "Tail surface span (explicit mode)"),
    p("empennage.root_chord_m", "m", 0.02, 0.8, "Tail surface root chord (explicit mode)"),
    p("empennage.taper_ratio", "1", 0.05, 1.5, "Tail surface taper ratio (explicit mode)"),
    p("empennage.sweep_le_deg", "deg", -20.0, 60.0, "Tail surface leading-edge sweep (explicit mode)"),
    p("empennage.incidence_deg", "deg", -10.0, 10.0, "Tail surface incidence (explicit mode)"),
    p("empennage.dihedral_deg", "deg", -60.0, 60.0, "Tail surface dihedral (explicit mode)"),
    p("layout.fuselage.length_m", "m", 0.1, 2.5, "Pod length from the nose"),
    p("layout.fuselage.width_m", "m", 0.03, 0.6, "Pod outer width"),
    p("layout.fuselage.height_m", "m", 0.03, 0.6, "Pod outer height"),
    p("layout.fuselage.wall_m", "m", 0.0002, 0.01, "Pod wall thickness"),
    p("layout.fuselage.bays[].x0_frac", "1", 0.0, 1.0, "Bay forward end, fraction of pod length"),
    p("layout.fuselage.bays[].x1_frac", "1", 0.0, 1.0, "Bay aft end, fraction of pod length"),
    p("layout.gear.main_x_m", "m", -1.0, 4.0, "Main axle x"),
    p("layout.gear.main_half_track_m", "m", 0.02, 1.0, "Main wheel lateral position (each side)"),
    p("layout.gear.main_wheel_diameter_m", "m", 0.015, 0.3, "Main wheel diameter"),
    p("layout.gear.main_wheel_width_m", "m", 0.005, 0.1, "Main tyre width"),
    p("layout.gear.aux_x_m", "m", -1.0, 4.0, "Tail or nose wheel x"),
    p("layout.gear.aux_wheel_diameter_m", "m", 0.01, 0.2, "Auxiliary wheel diameter"),
    p("layout.gear.aux_wheel_width_m", "m", 0.003, 0.08, "Auxiliary tyre width"),
    p("layout.gear.ground_z_m", "m", -1.0, -0.01, "Ground plane z (below the datum)"),
    p("layout.motor_mount.prop_plane_offset_m", "m", 0.0, 0.6, "Propeller plane offset from the wing leading edge or pod end"),
    p("layout.motor_mount.z_offset_m", "m", -0.2, 0.3, "Motor height above the wing leading-edge chord line"),
];

/// Look up a pattern.
pub fn param(path: &str) -> Option<&'static ParamInfo> {
    PARAMS.iter().find(|info| info.path == path)
}

/// Every scalar of `master` as `(concrete path, pattern, value)`.
pub fn scalars(master: &MasterParams) -> Vec<(String, &'static str, f64)> {
    let mut out = vec![(
        "root_chord_m".to_owned(),
        "root_chord_m",
        master.root_chord_m,
    )];
    for (i, panel) in master.panels.iter().enumerate() {
        for (name, pattern, value) in [
            ("span_m", "panels[].span_m", panel.span_m),
            ("taper_ratio", "panels[].taper_ratio", panel.taper_ratio),
            ("sweep_le_deg", "panels[].sweep_le_deg", panel.sweep_le_deg),
            ("dihedral_deg", "panels[].dihedral_deg", panel.dihedral_deg),
            (
                "twist_tip_deg",
                "panels[].twist_tip_deg",
                panel.twist_tip_deg,
            ),
        ] {
            out.push((format!("panels[{i}].{name}"), pattern, value));
        }
    }
    if let Some(w) = &master.winglet {
        for (pattern, value) in [
            ("winglet.cant_deg", w.cant_deg),
            ("winglet.length_m", w.length_m),
            ("winglet.root_chord_ratio", w.root_chord_ratio),
            ("winglet.taper_ratio", w.taper_ratio),
            ("winglet.toe_deg", w.toe_deg),
        ] {
            out.push((pattern.to_owned(), pattern, value));
        }
    }
    for (pattern, xyz) in [
        ("wing_xyz_m", master.wing_xyz_m),
        ("fuselage_xyz_m", master.fuselage_xyz_m),
        ("tail_xyz_m", master.tail_xyz_m),
    ] {
        for (axis, value) in xyz.iter().enumerate() {
            out.push((format!("{pattern}[{axis}]"), pattern, *value));
        }
    }
    if let Some(TailBoom {
        length_m,
        outer_diameter_m,
        wall_m,
    }) = &master.tail_boom
    {
        out.push(("tail_boom.length_m".into(), "tail_boom.length_m", *length_m));
        out.push((
            "tail_boom.outer_diameter_m".into(),
            "tail_boom.outer_diameter_m",
            *outer_diameter_m,
        ));
        out.push(("tail_boom.wall_m".into(), "tail_boom.wall_m", *wall_m));
    }
    for (i, c) in master.controls.iter().enumerate() {
        out.extend(control_scalars(i, c));
    }
    match &master.empennage {
        EmpennageDriving::Driven {
            horizontal_volume,
            vertical_volume,
            arm_m,
        } => {
            out.push((
                "empennage.horizontal_volume".into(),
                "empennage.horizontal_volume",
                *horizontal_volume,
            ));
            out.push((
                "empennage.vertical_volume".into(),
                "empennage.vertical_volume",
                *vertical_volume,
            ));
            out.push(("empennage.arm_m".into(), "empennage.arm_m", *arm_m));
        }
        EmpennageDriving::Explicit {
            horizontal,
            vertical,
        } => {
            for (name, surface) in [("horizontal", horizontal), ("vertical", vertical)] {
                if let Some(s) = surface {
                    out.extend(tail_scalars(name, s));
                }
            }
        }
    }
    out.extend(layout_scalars(&master.layout));
    out
}

fn layout_scalars(l: &LayoutParams) -> Vec<(String, &'static str, f64)> {
    let mut out = Vec::new();
    let mut put =
        |pattern: &'static str, value: f64| out.push((pattern.to_owned(), pattern, value));
    if let Some(f) = &l.fuselage {
        put("layout.fuselage.length_m", f.length_m);
        put("layout.fuselage.width_m", f.width_m);
        put("layout.fuselage.height_m", f.height_m);
        put("layout.fuselage.wall_m", f.wall_m);
    }
    if let Some(g) = &l.gear {
        put("layout.gear.main_x_m", g.main_x_m);
        put("layout.gear.main_half_track_m", g.main_half_track_m);
        put("layout.gear.main_wheel_diameter_m", g.main_wheel_diameter_m);
        put("layout.gear.main_wheel_width_m", g.main_wheel_width_m);
        put("layout.gear.aux_x_m", g.aux_x_m);
        put("layout.gear.aux_wheel_diameter_m", g.aux_wheel_diameter_m);
        put("layout.gear.aux_wheel_width_m", g.aux_wheel_width_m);
        put("layout.gear.ground_z_m", g.ground_z_m);
    }
    if let Some(m) = &l.motor_mount {
        put(
            "layout.motor_mount.prop_plane_offset_m",
            m.prop_plane_offset_m,
        );
        put("layout.motor_mount.z_offset_m", m.z_offset_m);
    }
    if let Some(f) = &l.fuselage {
        for (i, b) in f.bays.iter().enumerate() {
            out.push((
                format!("layout.fuselage.bays[{i}].x0_frac"),
                "layout.fuselage.bays[].x0_frac",
                b.x0_frac,
            ));
            out.push((
                format!("layout.fuselage.bays[{i}].x1_frac"),
                "layout.fuselage.bays[].x1_frac",
                b.x1_frac,
            ));
        }
    }
    out
}

fn control_scalars(i: usize, c: &ControlSurface) -> Vec<(String, &'static str, f64)> {
    [
        ("span_start", "controls[].span_start", c.span_start),
        ("span_end", "controls[].span_end", c.span_end),
        (
            "chord_fraction",
            "controls[].chord_fraction",
            c.chord_fraction,
        ),
        ("max_down_deg", "controls[].max_down_deg", c.max_down_deg),
        ("max_up_deg", "controls[].max_up_deg", c.max_up_deg),
    ]
    .into_iter()
    .map(|(name, pattern, value)| (format!("controls[{i}].{name}"), pattern, value))
    .collect()
}

fn tail_scalars(which: &str, s: &TailSurface) -> Vec<(String, &'static str, f64)> {
    [
        ("span_m", "empennage.span_m", s.span_m),
        ("root_chord_m", "empennage.root_chord_m", s.root_chord_m),
        ("taper_ratio", "empennage.taper_ratio", s.taper_ratio),
        ("sweep_le_deg", "empennage.sweep_le_deg", s.sweep_le_deg),
        ("incidence_deg", "empennage.incidence_deg", s.incidence_deg),
        ("dihedral_deg", "empennage.dihedral_deg", s.dihedral_deg),
    ]
    .into_iter()
    .map(|(name, pattern, value)| (format!("empennage.{which}.{name}"), pattern, value))
    .collect()
}

/// Markdown reference table (committed in `docs/uav-spec-schema.md`).
pub fn render_markdown() -> String {
    let mut out = String::from("| path | unit | range | meaning |\n|---|---|---|---|\n");
    for info in PARAMS {
        out.push_str(&format!(
            "| `{}` | {} | {} .. {} | {} |\n",
            info.path, info.unit, info.min, info.max, info.doc
        ));
    }
    out.push_str("\n| mission output | SI unit | meaning |\n|---|---|---|\n");
    for o in super::mission::OUTPUTS {
        out.push_str(&format!("| `{}` | {} | {} |\n", o.name, o.unit_si, o.doc));
    }
    out
}
