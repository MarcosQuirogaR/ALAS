// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The drag-relevant hull (fuselage, gear, nacelles, motor stations) derived
//! from the layout block, and the explicit empennage of a driven design.
//! This closes the M2 gap "hull dimensions are caller inputs until the
//! airframe supplies them".
//!
//! Estimates (all ESTIMATES, flagged): fuselage wetted area
//! `0.85 * perimeter * length` (taper allowance), perimeter `2 (w + h)`;
//! equivalent diameter `sqrt(4 w h / pi)`; main-gear legs are flat spring legs
//! 12 mm wide (Raymer Table 12.6, `CD = 1.4` on width times length);
//! nacelles are bluff cylinders (`CD = 0.8`). A retractable gear moves the main
//! wheels and legs to ground-only drag (the hull supports all or nothing); its
//! mechanism mass belongs in the tree.

use crate::aero::drag_lowre::{Component, Junction, Shape};
use crate::airframe::geometry::{AirframeGeometry, TailRole, TailShape};
use crate::coupled::{wheel, Fuselage, Hull, MotorStation as DragStation};
use crate::spec::master::{EmpennageDriving, TailSurface};
use crate::spec::topology::{GearKind, MotorLayout};
use crate::spec::UavSpec;

use super::geom::station;
use super::publish::motor_stations;
use super::StudioError;

/// Flat spring leg width, m (ESTIMATE, as the M2 blind builds).
pub const LEG_WIDTH_M: f64 = 0.012;

/// Derive the hull. `motor_diameter_m` sizes the nacelles of a wing-mounted
/// twin (`None`: no nacelle item).
///
/// # Errors
///
/// Missing layout blocks the topology needs.
pub fn derive_hull(spec: &UavSpec, motor_diameter_m: Option<f64>) -> Result<Hull, StudioError> {
    let l = &spec.master.layout;
    let mut hull = Hull::default();
    if let Some(f) = &l.fuselage {
        hull.fuselage = Some(Fuselage {
            length_m: f.length_m,
            diameter_m: (4.0 * f.width_m * f.height_m / std::f64::consts::PI).sqrt(),
            wetted_m2: Some(0.85 * 2.0 * (f.width_m + f.height_m) * f.length_m),
        });
    }
    let mut gear = Vec::new();
    if let Some(g) = &l.gear {
        let attach_z = spec.master.wing_xyz_m[2];
        let leg = (attach_z - (g.ground_z_m + 0.5 * g.main_wheel_diameter_m)).max(0.02);
        gear.push(wheel(
            "main wheels",
            g.main_wheel_diameter_m,
            g.main_wheel_width_m,
            2,
        ));
        gear.push(wheel(
            "aux wheel",
            g.aux_wheel_diameter_m,
            g.aux_wheel_width_m,
            1,
        ));
        gear.push(
            Component::new(
                "main gear legs",
                Shape::Item {
                    cd: 1.4,
                    frontal_m2: LEG_WIDTH_M * leg,
                },
            )
            .with_count(2)
            .as_gear(),
        );
    }
    if matches!(spec.topology.gear, GearKind::Retract { .. }) {
        let (gone, kept): (Vec<_>, Vec<_>) = gear
            .into_iter()
            .partition(|c| c.name == "main wheels" || c.name == "main gear legs");
        hull.items = kept;
        hull.ground_only = gone;
    } else {
        hull.items = gear;
    }
    if l.motor_mount.is_some() {
        let wing =
            &AirframeGeometry::from_master(&spec.master, &spec.topology, &TailShape::default())?
                .wing;
        if !matches!(spec.topology.motors, MotorLayout::SinglePusher) {
            for m in motor_stations(spec)? {
                let st = station(wing, m.plane_m[1].abs())
                    .ok_or_else(|| StudioError::Invalid("motor outside the wing".into()))?;
                hull.motors.push(DragStation {
                    y_m: m.plane_m[1],
                    s_to_wing_m: (st.le_m[0] + 0.25 * st.chord_m - m.plane_m[0]).max(1e-3),
                });
            }
        }
        if let (MotorLayout::Twin { .. }, Some(d)) = (spec.topology.motors, motor_diameter_m) {
            hull.items.push(
                Component::new(
                    "motor nacelles",
                    Shape::Item {
                        cd: 0.8,
                        frontal_m2: std::f64::consts::PI / 4.0 * d * d,
                    },
                )
                .with_junction(Junction::NacelleNear)
                .with_count(2)
                .in_slipstream(),
            );
        }
    }
    Ok(hull)
}

/// The same design with a driven empennage turned into explicit surfaces
/// (`span_m` along the surface), so the M2 chain can use it. An explicit
/// empennage is returned unchanged.
///
/// # Errors
///
/// Non-physical geometry.
pub fn with_explicit_empennage(spec: &UavSpec) -> Result<UavSpec, StudioError> {
    if matches!(spec.master.empennage, EmpennageDriving::Explicit { .. }) {
        return Ok(spec.clone());
    }
    let g = AirframeGeometry::from_master(&spec.master, &spec.topology, &TailShape::default())?;
    let to_surface = |s: &crate::airframe::geometry::TailSurfaceGeom| {
        let p = &s.planform.panels[0];
        let vertical = s.role == TailRole::Vertical;
        let dih = s.v_dihedral_rad;
        TailSurface {
            span_m: if vertical {
                p.span_m
            } else {
                s.planform.span_m() / dih.cos()
            },
            root_chord_m: p.chord_root_m,
            taper_ratio: p.chord_tip_m / p.chord_root_m,
            sweep_le_deg: p.sweep_le_rad.to_degrees(),
            incidence_deg: s.incidence_rad.to_degrees(),
            dihedral_deg: dih.to_degrees(),
            // Section of the sized surface: ESTIMATE (symmetric 10 %), not a design choice.
            airfoil: "naca0010".into(),
        }
    };
    let mut out = spec.clone();
    out.master.empennage = EmpennageDriving::Explicit {
        horizontal: g.horizontal.as_ref().map(to_surface),
        vertical: g.vertical.as_ref().map(to_surface),
    };
    Ok(out)
}
