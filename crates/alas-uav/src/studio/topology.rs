// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Topology library: complete, valid `UavSpec` skeletons from a few sizing
//! numbers. Each entry is data (a `Topology` plus master parameters), not a
//! code path of the physics.
//!
//! Layout rules shared by the entries (all ESTIMATES of proportion, to be
//! overwritten by the designer; none is a validated optimum):
//! wing root leading edge at 40 % of the pod length (payload bay centred near
//! the wing; the CG is NOT tuned to a margin); low wing (root chord line
//! a quarter of the pod height below the pod axis); tail aerodynamic centre one
//! `tail_arm_m` aft of the wing aerodynamic centre (driven empennage, Raymer
//! Table 6.4 volume coefficients); boom from the pod end to the tail mount.
//! Frame: x aft from the nose datum, y starboard, z up, m.

use std::collections::BTreeMap;

use crate::airframe::tail::{AircraftClass, VolumeCoefficients};
use crate::score::presets;
use crate::spec::layout::{BayDef, FuselagePod, GearGeometry, LayoutParams, MotorMount};
use crate::spec::master::{
    ControlKind, ControlSurface, EmpennageDriving, HostSurface, MasterParams, TailBoom, WingPanel,
};
use crate::spec::mission::{Decisions, MissionSpec, PresetRef};
use crate::spec::topology::{
    FuselageKind, GearKind, MotorLayout, Rotation, TailKind, Topology, WingKind,
};
use crate::spec::{UavSpec, SPEC_SCHEMA_VERSION};

use super::publish::refresh;
use super::StudioError;

/// A configuration of the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Preset {
    /// Single nose tractor, conventional tail on a boom, taildragger.
    ConventionalSingle,
    /// Twin wing tractors, V-tail on a boom, taildragger (ACC/XC class).
    VTailTwin,
    /// Single pusher, T-tail on a boom, tricycle gear.
    TTailPusher,
    /// Tailless swept wing with elevons, pusher, belly landing.
    FlyingWing,
}

/// Library entry description.
#[derive(Debug, Clone, Copy)]
pub struct PresetInfo {
    /// The entry.
    pub preset: Preset,
    /// Stable key.
    pub key: &'static str,
    /// One-line description.
    pub summary: &'static str,
}

/// The library.
pub const LIBRARY: [PresetInfo; 4] = [
    PresetInfo {
        preset: Preset::ConventionalSingle,
        key: "conventional-single",
        summary: "single tractor, conventional tail on a boom, taildragger",
    },
    PresetInfo {
        preset: Preset::VTailTwin,
        key: "vtail-twin",
        summary: "twin wing tractors, V-tail on a boom, taildragger",
    },
    PresetInfo {
        preset: Preset::TTailPusher,
        key: "ttail-pusher",
        summary: "single pusher, T-tail on a boom, tricycle gear",
    },
    PresetInfo {
        preset: Preset::FlyingWing,
        key: "flying-wing",
        summary: "tailless swept wing with elevons, pusher, belly landing",
    },
];

/// The numbers a designer types to start from a preset.
#[derive(Debug, Clone, PartialEq)]
pub struct Sizing {
    /// Design name.
    pub name: String,
    /// Wing span, m.
    pub span_m: f64,
    /// Wing root chord, m.
    pub root_chord_m: f64,
    /// Tip chord over root chord of the outer panel.
    pub taper: f64,
    /// Airfoil name for all wing panels.
    pub airfoil: String,
    /// Pod length, m.
    pub pod_length_m: f64,
    /// Pod width, m.
    pub pod_width_m: f64,
    /// Pod height, m.
    pub pod_height_m: f64,
    /// Wing-to-tail aerodynamic-centre arm, m.
    pub tail_arm_m: f64,
    /// Mission preset id (`score::presets`).
    pub mission_preset: String,
}

impl Default for Sizing {
    fn default() -> Self {
        Self {
            name: "studio-design".into(),
            span_m: 2.4,
            root_chord_m: 0.3,
            taper: 0.5,
            airfoil: "naca2412".into(),
            pod_length_m: 0.8,
            pod_width_m: 0.16,
            pod_height_m: 0.14,
            tail_arm_m: 0.9,
            mission_preset: "acc-2026".into(),
        }
    }
}

fn panel(span: f64, taper: f64, sweep: f64, dihedral: f64, foil: &str) -> WingPanel {
    WingPanel {
        span_m: span,
        taper_ratio: taper,
        sweep_le_deg: sweep,
        dihedral_deg: dihedral,
        twist_tip_deg: 0.0,
        airfoil_root: foil.into(),
        airfoil_tip: foil.into(),
    }
}

fn control(kind: ControlKind, host: HostSurface, a: f64, b: f64, cf: f64) -> ControlSurface {
    ControlSurface {
        kind,
        host,
        span_start: a,
        span_end: b,
        chord_fraction: cf,
        max_down_deg: 25.0,
        max_up_deg: 25.0,
    }
}

fn bays() -> Vec<BayDef> {
    [
        ("Nose", 0.0, 0.2),
        ("Battery", 0.2, 0.36),
        ("Payload", 0.36, 0.70),
        ("Aft", 0.70, 1.0),
    ]
    .into_iter()
    .map(|(n, a, b)| BayDef {
        name: n.into(),
        x0_frac: a,
        x1_frac: b,
    })
    .collect()
}

/// Build a preset.
///
/// # Errors
///
/// [`StudioError::Invalid`] for a non-physical sizing or an unknown mission
/// preset; the produced spec is validated.
pub fn build(preset: Preset, s: &Sizing) -> Result<UavSpec, StudioError> {
    let bad = |m: &str| StudioError::Invalid(m.to_owned());
    if !(s.span_m > 0.3 && s.root_chord_m > 0.03 && s.taper > 0.05 && s.tail_arm_m > 0.1) {
        return Err(bad(
            "sizing: span, chord, taper and tail arm must be positive",
        ));
    }
    let flying = preset == Preset::FlyingWing;
    let half = 0.5 * s.span_m;
    let (l, w, h) = (s.pod_length_m, s.pod_width_m, s.pod_height_m);
    let foil = s.airfoil.as_str();
    let wing_x = if flying { 0.0 } else { 0.40 * l };
    let wing_z = if flying { 0.0 } else { -0.25 * h };
    let panels = if flying {
        vec![
            panel(0.35 * half, 1.0, 20.0, 0.0, foil),
            panel(0.65 * half, s.taper, 20.0, 2.0, foil),
        ]
    } else {
        vec![
            panel(0.3 * half, 1.0, 0.0, 0.0, foil),
            panel(0.7 * half, s.taper, 0.0, 2.0, foil),
        ]
    };
    let (kind_tail, motors, gear_kind, fuselage_kind) = match preset {
        Preset::ConventionalSingle => (
            TailKind::Conventional,
            MotorLayout::SingleTractor,
            GearKind::Taildragger,
            FuselageKind::Boom,
        ),
        Preset::VTailTwin => (
            TailKind::V { dihedral_deg: 40.0 },
            MotorLayout::Twin {
                span_fraction: 0.2,
                rotation: Rotation::Counter,
            },
            GearKind::Taildragger,
            FuselageKind::Boom,
        ),
        Preset::TTailPusher => (
            TailKind::T,
            MotorLayout::SinglePusher,
            GearKind::Tricycle,
            FuselageKind::Boom,
        ),
        Preset::FlyingWing => (
            TailKind::None,
            MotorLayout::SinglePusher,
            GearKind::Belly,
            FuselageKind::None,
        ),
    };
    let class = if flying {
        AircraftClass::Sailplane
    } else {
        AircraftClass::Homebuilt
    };
    let vc = VolumeCoefficients::raymer(class).for_topology(kind_tail);
    let wing_ac = wing_x + 0.25 * s.root_chord_m;
    let tail_x = wing_ac + s.tail_arm_m - 0.05;
    let tail_z = if kind_tail == TailKind::T { 0.2 } else { 0.0 };
    let boom = (fuselage_kind == FuselageKind::Boom).then(|| TailBoom {
        length_m: (tail_x - l).max(0.1),
        outer_diameter_m: 0.02,
        wall_m: 0.0015,
    });
    let mut controls = vec![
        control(ControlKind::Aileron, HostSurface::Wing, 0.55, 0.95, 0.25),
        control(ControlKind::Flap, HostSurface::Wing, 0.1, 0.5, 0.25),
    ];
    if flying {
        controls = vec![control(
            ControlKind::Elevon,
            HostSurface::Wing,
            0.3,
            0.95,
            0.25,
        )];
    } else if kind_tail == TailKind::Conventional || kind_tail == TailKind::T {
        controls.push(control(
            ControlKind::Elevator,
            HostSurface::HorizontalTail,
            0.0,
            1.0,
            0.35,
        ));
        controls.push(control(
            ControlKind::Rudder,
            HostSurface::VerticalTail,
            0.0,
            1.0,
            0.35,
        ));
    } else if matches!(kind_tail, TailKind::V { .. }) {
        controls.push(control(
            ControlKind::Ruddervator,
            HostSurface::HorizontalTail,
            0.0,
            1.0,
            0.35,
        ));
    }
    // Leg length: room for a 0.28 m propeller below a low wing (ESTIMATE).
    let ground_z = wing_z.min(-0.5 * h) - 0.2;
    let layout = LayoutParams {
        fuselage: (!flying).then(|| FuselagePod {
            length_m: l,
            width_m: w,
            height_m: h,
            wall_m: 0.002,
            bays: bays(),
        }),
        gear: (gear_kind != GearKind::Belly).then(|| GearGeometry {
            main_x_m: wing_x + 0.3 * s.root_chord_m,
            main_half_track_m: 0.25 * half.min(0.8),
            main_wheel_diameter_m: 0.08,
            main_wheel_width_m: 0.025,
            aux_x_m: if gear_kind == GearKind::Tricycle {
                0.1 * l
            } else {
                tail_x + 0.05
            },
            aux_wheel_diameter_m: 0.03,
            aux_wheel_width_m: 0.01,
            ground_z_m: ground_z,
        }),
        motor_mount: Some(MotorMount {
            prop_plane_offset_m: match motors {
                MotorLayout::Twin { .. } => 0.12,
                _ => 0.04,
            },
            z_offset_m: 0.0,
        }),
    };
    let wing_kind = if flying {
        WingKind::Flying {
            sweep_deg: 20.0,
            washout_deg: 2.0,
            reflex_airfoil: false,
        }
    } else {
        WingKind::Straight
    };
    let mission = presets::load(&s.mission_preset)
        .map_err(|e| StudioError::Invalid(format!("mission preset: {e}")))?;
    let mut spec = UavSpec {
        schema_version: SPEC_SCHEMA_VERSION,
        name: s.name.clone(),
        master: MasterParams {
            root_chord_m: s.root_chord_m,
            panels,
            winglet: None,
            wing_xyz_m: [wing_x, 0.0, wing_z],
            fuselage_xyz_m: [0.0; 3],
            tail_xyz_m: if flying {
                [0.0; 3]
            } else {
                [tail_x, 0.0, tail_z]
            },
            tail_boom: boom,
            controls,
            empennage: if flying {
                EmpennageDriving::Explicit {
                    horizontal: None,
                    vertical: None,
                }
            } else {
                EmpennageDriving::Driven {
                    horizontal_volume: vc.horizontal,
                    vertical_volume: vc.vertical,
                    arm_m: s.tail_arm_m,
                }
            },
            layout,
        },
        topology: Topology {
            wing: wing_kind,
            tail: kind_tail,
            motors,
            gear: gear_kind,
            fuselage: fuselage_kind,
        },
        subskeletons: BTreeMap::new(),
        tolerances_m: BTreeMap::new(),
        tree: Vec::new(),
        mission: MissionSpec {
            preset: PresetRef {
                id: s.mission_preset.clone(),
                version: mission.version,
                variant: None,
            },
            parameters: BTreeMap::new(),
            decisions: Decisions {
                announced_takeoff_m: None,
                payload_count: None,
                declared_payload_count: None,
                takeoff_confidence: None,
            },
            segments: mission.segments,
            rounds: 3,
        },
        origins: BTreeMap::new(),
    };
    refresh(&mut spec)?;
    spec.validate()
        .map_err(|p| StudioError::Invalid(p.join("; ")))?;
    Ok(spec)
}
