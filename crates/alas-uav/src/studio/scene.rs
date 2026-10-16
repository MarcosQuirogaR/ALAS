// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The component-box scene: plain data a viewer draws (the GUI preview draws
//! three orthographic views). Body axes, x aft, y starboard, z up, m.

use alas_uav_db::Database;

use crate::airframe::geometry::{AirframeGeometry, Planform, TailShape};
use crate::airframe::inertia::Vec3;
use crate::airframe::mass::LoadCase;
use crate::airframe::MassInputs;
use crate::spec::skeleton::Datum;
use crate::spec::UavSpec;

use super::check::{check, Disc, PackReport, Placed};
use super::geom::station;
use super::markers::{markers, Marker};
use super::publish::motor_stations;
use super::StudioError;

/// A closed outline of a lifting surface.
#[derive(Debug, Clone, PartialEq)]
pub struct Outline {
    /// Surface name.
    pub name: String,
    /// Vertices in order, m.
    pub points_m: Vec<Vec3>,
}

/// A line (thrust line, tail arm).
#[derive(Debug, Clone, PartialEq)]
pub struct SceneLine {
    /// Label.
    pub label: String,
    /// Start, m.
    pub a_m: Vec3,
    /// End, m.
    pub b_m: Vec3,
}

/// Everything a viewer needs.
#[derive(Debug, Clone)]
pub struct Scene {
    /// Placed part boxes.
    pub parts: Vec<Placed>,
    /// Propeller discs.
    pub discs: Vec<Disc>,
    /// Pod envelope, if any.
    pub envelope: Option<(Vec3, Vec3)>,
    /// Bay boxes `(name, min, max)`.
    pub bays: Vec<(String, Vec3, Vec3)>,
    /// Lifting-surface outlines.
    pub outlines: Vec<Outline>,
    /// Boom axis end points.
    pub boom: Option<(Vec3, Vec3)>,
    /// Lines.
    pub lines: Vec<SceneLine>,
    /// CG and NP markers.
    pub markers: Vec<Marker>,
    /// Static margin of the first load case.
    pub static_margin: f64,
    /// The pack report behind the boxes.
    pub report: PackReport,
}

fn outline(name: &str, pf: &Planform) -> Vec<Outline> {
    let mut sides = vec![(name.to_owned(), 1.0)];
    if pf.mirror {
        sides.push((format!("{name} (port)"), -1.0));
    }
    let mut out = Vec::new();
    for (n, sign) in sides {
        let mut le = Vec::new();
        let mut te = Vec::new();
        let mut acc = 0.0;
        let push = |s: f64, le: &mut Vec<Vec3>, te: &mut Vec<Vec3>| {
            if let Some(st) = station(pf, s) {
                let m = |p: Vec3| [p[0], sign * p[1], p[2]];
                le.push(m(st.le_m));
                te.push(m(st.at_chord_fraction(1.0)));
            }
        };
        push(0.0, &mut le, &mut te);
        for p in &pf.panels {
            acc += p.span_m;
            push(acc, &mut le, &mut te);
        }
        te.reverse();
        le.extend(te);
        out.push(Outline {
            name: n,
            points_m: le,
        });
    }
    out
}

/// Build the scene.
///
/// # Errors
///
/// Failures of the geometry, mass or packaging models.
pub fn build_scene(
    spec: &UavSpec,
    db: &Database,
    inputs: &MassInputs,
    cases: &[LoadCase],
) -> Result<Scene, StudioError> {
    let report = check(spec, db)?;
    let mk = markers(spec, db, inputs, cases)?;
    let g = AirframeGeometry::from_master(&spec.master, &spec.topology, &TailShape::default())?;
    let mut outlines = outline("wing", &g.wing);
    if let Some(h) = &g.horizontal {
        outlines.extend(outline("horizontal tail", &h.planform));
    }
    if let Some(v) = &g.vertical {
        outlines.extend(outline("fin", &v.planform));
    }
    let find_box = |name: &str| {
        spec.subskeletons
            .values()
            .flatten()
            .find(|p| p.name == name)
            .and_then(|p| match &p.datum {
                Datum::Box { min_m, max_m } => Some((*min_m, *max_m)),
                _ => None,
            })
    };
    let bays = spec
        .master
        .layout
        .fuselage
        .iter()
        .flat_map(|f| f.bays.iter())
        .filter_map(|b| {
            find_box(&format!("FS_Bay_{}", b.name)).map(|(a, c)| (b.name.clone(), a, c))
        })
        .collect();
    let boom = spec.master.tail_boom.as_ref().map(|b| {
        let end = spec.master.tail_xyz_m;
        ([end[0] - b.length_m, end[1], end[2]], end)
    });
    let mut lines = Vec::new();
    if spec.master.layout.motor_mount.is_some() {
        for m in motor_stations(spec)? {
            let dir = if m.pusher { 1.0 } else { -1.0 };
            lines.push(SceneLine {
                label: format!("thrust line {}", m.name),
                a_m: m.plane_m,
                b_m: [m.plane_m[0] - dir * 0.3, m.plane_m[1], m.plane_m[2]],
            });
        }
    }
    let pick = |k| {
        mk.markers
            .iter()
            .find(|m| m.kind == k)
            .map(|m| m.position_m)
    };
    if let (Some(a), Some(b)) = (
        pick(super::markers::MarkerKind::WingAc),
        pick(super::markers::MarkerKind::TailAc),
    ) {
        lines.push(SceneLine {
            label: "tail arm".into(),
            a_m: a,
            b_m: [b[0], a[1], a[2]],
        });
    }
    Ok(Scene {
        parts: report.parts.clone(),
        discs: report.discs.clone(),
        envelope: find_box("FS_Envelope"),
        bays,
        outlines,
        boom,
        lines,
        static_margin: mk.stability.first().map_or(f64::NAN, |s| s.static_margin),
        markers: mk.markers,
        report,
    })
}
