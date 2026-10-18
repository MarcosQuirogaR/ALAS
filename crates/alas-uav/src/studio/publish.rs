// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Publications derived from the master parameters (UAV-PLAN 2.2).
//!
//! The skeleton publishes named datums; parts reference them by name only, so
//! changing a master parameter and calling [`refresh`] regenerates the datums
//! (spar axes, hinge lines, servo seats, bays, frames, gear and motor
//! stations) and moves every part that references them.
//!
//! Reserved prefixes of derived names: `WS_`, `FS_`, `ES_`, `WMS_`, `EMS_`,
//! `MLGS_`, `PS_`. [`refresh`] deletes and recreates exactly those; custom
//! publications must use another prefix (for example `USR_`).
//! Mirrored items carry the suffix `_R` (starboard, +y) and `_L` (port).

use crate::airframe::geometry::{wing_planform, AirframeGeometry, Planform, TailShape};
use crate::airframe::inertia::Vec3;
use crate::spec::master::HostSurface;
use crate::spec::skeleton::{Datum, Publication, Skeleton};
use crate::spec::topology::MotorLayout;
use crate::spec::UavSpec;

use super::geom::{extent, station};
use super::StudioError;

const RESERVED: [&str; 7] = ["WS_", "FS_", "ES_", "WMS_", "EMS_", "MLGS_", "PS_"];

/// Whether `name` belongs to the derived (regenerated) namespace.
pub fn is_derived_name(name: &str) -> bool {
    RESERVED.iter().any(|p| name.starts_with(p))
}

/// A propeller plane (normal along x).
#[derive(Debug, Clone, PartialEq)]
pub struct MotorStation {
    /// `R`, `L` or `C`.
    pub name: String,
    /// Propeller-plane centre, m.
    pub plane_m: Vec3,
    /// Motor behind the propeller plane (pusher) instead of ahead of it.
    pub pusher: bool,
}

fn point(owner: Skeleton, name: &str, xyz_m: Vec3) -> Publication {
    Publication {
        name: name.to_owned(),
        owner,
        datum: Datum::Point { xyz_m },
    }
}

fn axis(owner: Skeleton, name: &str, a: Vec3, b: Vec3) -> Publication {
    let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let n = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-12);
    Publication {
        name: name.to_owned(),
        owner,
        datum: Datum::Axis {
            origin_m: a,
            direction: [d[0] / n, d[1] / n, d[2] / n],
        },
    }
}

fn mirror(p: Vec3) -> Vec3 {
    [p[0], -p[1], p[2]]
}

/// Motor stations of the topology.
///
/// # Errors
///
/// [`StudioError::Missing`] when a layout block the motor layout needs is absent.
pub fn motor_stations(spec: &UavSpec) -> Result<Vec<MotorStation>, StudioError> {
    let l = &spec.master.layout;
    let mount = l
        .motor_mount
        .as_ref()
        .ok_or(StudioError::Missing("layout.motor_mount"))?;
    let wing = wing_planform(&spec.master)?;
    Ok(match spec.topology.motors {
        MotorLayout::Twin { span_fraction, .. } => {
            let y = span_fraction * extent(&wing);
            let st = station(&wing, y).ok_or_else(|| StudioError::Invalid("motor y".into()))?;
            let p = [
                st.le_m[0] - mount.prop_plane_offset_m,
                st.le_m[1],
                st.le_m[2] + mount.z_offset_m,
            ];
            vec![
                MotorStation {
                    name: "R".into(),
                    plane_m: p,
                    pusher: false,
                },
                MotorStation {
                    name: "L".into(),
                    plane_m: mirror(p),
                    pusher: false,
                },
            ]
        }
        MotorLayout::SingleTractor => {
            if l.fuselage.is_none() {
                return Err(StudioError::Missing("layout.fuselage"));
            }
            let o = spec.master.fuselage_xyz_m;
            vec![MotorStation {
                name: "C".into(),
                plane_m: [o[0] - mount.prop_plane_offset_m, o[1], o[2]],
                pusher: false,
            }]
        }
        MotorLayout::SinglePusher => {
            let p = match &l.fuselage {
                Some(f) => {
                    let o = spec.master.fuselage_xyz_m;
                    [o[0] + f.length_m + mount.prop_plane_offset_m, o[1], o[2]]
                }
                None => {
                    let root = station(&wing, 0.0)
                        .ok_or_else(|| StudioError::Invalid("wing root".into()))?;
                    [
                        root.le_m[0] + root.chord_m + mount.prop_plane_offset_m,
                        0.0,
                        root.le_m[2] + mount.z_offset_m,
                    ]
                }
            };
            vec![MotorStation {
                name: "C".into(),
                plane_m: p,
                pusher: true,
            }]
        }
    })
}

fn wing_pubs(
    spec: &UavSpec,
    wing: &Planform,
    out: &mut Vec<Publication>,
) -> Result<(), StudioError> {
    let o = Skeleton::Wing;
    let bad = |what: &str| StudioError::Invalid(what.to_owned());
    let root = station(wing, 0.0).ok_or_else(|| bad("wing root"))?;
    let half = extent(wing);
    let tip = station(wing, half).ok_or_else(|| bad("wing tip"))?;
    out.push(point(o, "WS_Root_LE", root.le_m));
    out.push(point(o, "WS_Tip_LE", tip.le_m));
    let c = wing.mac_m();
    let s_mac = wing.s_mac_m();
    let at = station(wing, s_mac).ok_or_else(|| bad("wing MAC"))?;
    out.push(point(
        o,
        "WS_AC",
        [wing.x_le_mac_m() + 0.25 * c, s_mac, at.le_m[2]],
    ));
    let mut acc = 0.0;
    for (i, p) in spec.master.panels.iter().enumerate() {
        let a = station(wing, acc).ok_or_else(|| bad("panel"))?;
        acc += p.span_m;
        let b = station(wing, acc).ok_or_else(|| bad("panel"))?;
        out.push(point(o, &format!("WS_Panel_{i}_Break"), b.le_m));
        out.push(axis(
            o,
            &format!("WS_Spar_{i}_Axis"),
            a.at_chord_fraction(0.25),
            b.at_chord_fraction(0.25),
        ));
    }
    Ok(())
}

fn control_pubs(
    spec: &UavSpec,
    wing: &Planform,
    geometry: &AirframeGeometry,
    out: &mut Vec<Publication>,
) -> Result<(), StudioError> {
    for (k, c) in spec.master.controls.iter().enumerate() {
        let no_host = |what: &str| StudioError::Invalid(format!("controls[{k}]: no {what}"));
        let (pf, owner, tag): (&Planform, Skeleton, &str) = match c.host {
            HostSurface::Wing => (wing, Skeleton::WingMech, "WMS"),
            HostSurface::HorizontalTail => (
                &geometry
                    .horizontal
                    .as_ref()
                    .ok_or_else(|| no_host("horizontal tail"))?
                    .planform,
                Skeleton::EmpennageMech,
                "EMS",
            ),
            HostSurface::VerticalTail => (
                &geometry
                    .vertical
                    .as_ref()
                    .ok_or_else(|| no_host("vertical tail"))?
                    .planform,
                Skeleton::EmpennageMech,
                "EMS",
            ),
        };
        let half = extent(pf);
        let bad = || StudioError::Invalid(format!("controls[{k}] is outside its host"));
        let a = station(pf, c.span_start * half).ok_or_else(bad)?;
        let b = station(pf, c.span_end * half).ok_or_else(bad)?;
        let hinge = 1.0 - c.chord_fraction;
        let (pa, pb) = (a.at_chord_fraction(hinge), b.at_chord_fraction(hinge));
        let mid = [
            0.5 * (pa[0] + pb[0]),
            0.5 * (pa[1] + pb[1]),
            0.5 * (pa[2] + pb[2]),
        ];
        if pf.mirror {
            for (suffix, flip) in [("R", false), ("L", true)] {
                let m = |p: Vec3| if flip { mirror(p) } else { p };
                out.push(axis(
                    owner,
                    &format!("{tag}_Hinge_{k}_Axis_{suffix}"),
                    m(pa),
                    m(pb),
                ));
                out.push(point(
                    owner,
                    &format!("{tag}_Servo_{k}_Seat_{suffix}"),
                    m(mid),
                ));
            }
        } else {
            out.push(axis(owner, &format!("{tag}_Hinge_{k}_Axis"), pa, pb));
            out.push(point(owner, &format!("{tag}_Servo_{k}_Seat"), mid));
        }
    }
    Ok(())
}

fn fuselage_pubs(spec: &UavSpec, out: &mut Vec<Publication>) {
    let o = Skeleton::Fuselage;
    let m = &spec.master;
    if let Some(f) = &m.layout.fuselage {
        let n = m.fuselage_xyz_m;
        out.push(point(o, "FS_Nose", n));
        let (hw, hh) = (0.5 * f.width_m, 0.5 * f.height_m);
        out.push(Publication {
            name: "FS_Envelope".into(),
            owner: o,
            datum: Datum::Box {
                min_m: [n[0], n[1] - hw, n[2] - hh],
                max_m: [n[0] + f.length_m, n[1] + hw, n[2] + hh],
            },
        });
        for b in &f.bays {
            let mut x0 = n[0] + b.x0_frac * f.length_m;
            let mut x1 = n[0] + b.x1_frac * f.length_m;
            if b.x0_frac <= 0.0 {
                x0 += f.wall_m;
            }
            if b.x1_frac >= 1.0 {
                x1 -= f.wall_m;
            }
            out.push(Publication {
                name: format!("FS_Bay_{}", b.name),
                owner: o,
                datum: Datum::Box {
                    min_m: [x0, n[1] - hw + f.wall_m, n[2] - hh + f.wall_m],
                    max_m: [x1, n[1] + hw - f.wall_m, n[2] + hh - f.wall_m],
                },
            });
            out.push(Publication {
                name: format!("FS_Frame_{}", b.name),
                owner: o,
                datum: Datum::Plane {
                    origin_m: [n[0] + b.x0_frac * f.length_m, n[1], n[2]],
                    normal: [1.0, 0.0, 0.0],
                },
            });
        }
    }
    if let Some(b) = &m.tail_boom {
        let end = m.tail_xyz_m;
        let root = [end[0] - b.length_m, end[1], end[2]];
        out.push(axis(o, "FS_Boom_Axis", root, end));
    }
}

fn gear_pubs(spec: &UavSpec, out: &mut Vec<Publication>) {
    let Some(g) = &spec.master.layout.gear else {
        return;
    };
    let o = Skeleton::Gear;
    let zm = g.ground_z_m + 0.5 * g.main_wheel_diameter_m;
    out.push(point(
        o,
        "MLGS_Main_R",
        [g.main_x_m, g.main_half_track_m, zm],
    ));
    out.push(point(
        o,
        "MLGS_Main_L",
        [g.main_x_m, -g.main_half_track_m, zm],
    ));
    out.push(point(
        o,
        "MLGS_Aux",
        [g.aux_x_m, 0.0, g.ground_z_m + 0.5 * g.aux_wheel_diameter_m],
    ));
    out.push(Publication {
        name: "MLGS_Ground".into(),
        owner: o,
        datum: Datum::Plane {
            origin_m: [0.0, 0.0, g.ground_z_m],
            normal: [0.0, 0.0, 1.0],
        },
    });
}

/// The derived publications of the design.
///
/// # Errors
///
/// Non-physical geometry, or a control surface on an absent host.
pub fn derive(spec: &UavSpec) -> Result<Vec<Publication>, StudioError> {
    let geometry =
        AirframeGeometry::from_master(&spec.master, &spec.topology, &TailShape::default())?;
    let wing = geometry.wing.clone();
    let mut out = Vec::new();
    wing_pubs(spec, &wing, &mut out)?;
    control_pubs(spec, &wing, &geometry, &mut out)?;
    out.push(point(
        Skeleton::Empennage,
        "ES_Mount",
        spec.master.tail_xyz_m,
    ));
    if let Some(h) = &geometry.horizontal {
        out.push(point(
            Skeleton::Empennage,
            "ES_H_AC",
            h.planform.aerodynamic_centre_m(),
        ));
    }
    if let Some(v) = &geometry.vertical {
        out.push(point(
            Skeleton::Empennage,
            "ES_V_AC",
            v.planform.aerodynamic_centre_m(),
        ));
    }
    fuselage_pubs(spec, &mut out);
    gear_pubs(spec, &mut out);
    if spec.master.layout.motor_mount.is_some() {
        for m in motor_stations(spec)? {
            let n = format!("PS_Motor_{}", m.name);
            out.push(Publication {
                name: format!("{n}_Plane"),
                owner: Skeleton::Propulsion,
                datum: Datum::Plane {
                    origin_m: m.plane_m,
                    normal: [1.0, 0.0, 0.0],
                },
            });
            out.push(axis(
                Skeleton::Propulsion,
                &format!("{n}_Axis"),
                m.plane_m,
                [m.plane_m[0] + 1.0, m.plane_m[1], m.plane_m[2]],
            ));
        }
    }
    Ok(out)
}

/// Replace the derived publications of `spec` by freshly derived ones; custom
/// publications are kept.
///
/// # Errors
///
/// As [`derive`].
pub fn refresh(spec: &mut UavSpec) -> Result<(), StudioError> {
    let fresh = derive(spec)?;
    for list in spec.subskeletons.values_mut() {
        list.retain(|p| !is_derived_name(&p.name));
    }
    for p in fresh {
        spec.subskeletons.entry(p.owner).or_default().push(p);
    }
    spec.subskeletons.retain(|_, v| !v.is_empty());
    Ok(())
}

/// Look up a publication by name.
pub fn find<'a>(spec: &'a UavSpec, name: &str) -> Option<&'a Publication> {
    spec.subskeletons
        .values()
        .flatten()
        .find(|p| p.name == name)
}
