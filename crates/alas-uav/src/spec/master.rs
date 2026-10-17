// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Master parameters: the global skeleton of the airframe (SI, aircraft frame).
//!
//! Frame: origin at the fuselage nose reference point, `x` aft, `y` towards
//! the right wing tip, `z` up (the `alas-geom` convention). Lengths in metres,
//! areas in square metres, angles in degrees (the unit a CAD parameter table
//! uses), fractions dimensionless. Parts never store absolute coordinates;
//! they reference publications (see `skeleton`), so changing `root_chord_m`
//! regenerates ribs, bays and the centre of gravity.

use serde::{Deserialize, Serialize};

use super::layout::LayoutParams;

/// One straight wing panel, root to tip, half-span (the other half is mirrored).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WingPanel {
    /// Panel span measured along `y`, m.
    pub span_m: f64,
    /// Tip chord over root chord of this panel, (0, 1.5].
    pub taper_ratio: f64,
    /// Leading-edge sweep, deg (positive aft).
    pub sweep_le_deg: f64,
    /// Dihedral, deg (positive tip up).
    pub dihedral_deg: f64,
    /// Geometric twist of the tip section relative to the root, deg (positive nose-up).
    pub twist_tip_deg: f64,
    /// Airfoil at the panel root (name resolved by the airfoil library or a polar import).
    pub airfoil_root: String,
    /// Airfoil at the panel tip.
    pub airfoil_tip: String,
}

/// A winglet attached to the last panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Winglet {
    /// Cant from vertical, deg (0 = vertical, 90 = continuation of the wing).
    pub cant_deg: f64,
    /// Winglet length along its own axis, m.
    pub length_m: f64,
    /// Root chord over the tip chord of the last panel, (0, 1].
    pub root_chord_ratio: f64,
    /// Tip chord over its own root chord, (0, 1].
    pub taper_ratio: f64,
    /// Toe-in (positive nose-in), deg.
    pub toe_deg: f64,
}

/// A tail boom (carbon tube) connecting wing/fuselage to the empennage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TailBoom {
    /// Length from the boom root station to the empennage mount, m.
    pub length_m: f64,
    /// Outer diameter, m.
    pub outer_diameter_m: f64,
    /// Wall thickness, m.
    pub wall_m: f64,
}

/// Which surface a control surface belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostSurface {
    /// The main wing.
    Wing,
    /// Horizontal tail (or the V-tail pair as one lifting surface).
    HorizontalTail,
    /// Vertical tail.
    VerticalTail,
}

/// Control-surface function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlKind {
    /// Roll control.
    Aileron,
    /// High-lift flap.
    Flap,
    /// Combined flap and aileron.
    Flaperon,
    /// Pitch control.
    Elevator,
    /// Yaw control.
    Rudder,
    /// V-tail surface mixing pitch and yaw.
    Ruddervator,
    /// Flying-wing surface mixing pitch and roll.
    Elevon,
    /// Airbrake or spoiler.
    Spoiler,
}

/// A hinged control surface on one half of its host (instantiated both sides
/// unless the function is on the centre line).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControlSurface {
    /// Function.
    pub kind: ControlKind,
    /// Host surface.
    pub host: HostSurface,
    /// Inboard end as a fraction of the host half-span, [0, 1].
    pub span_start: f64,
    /// Outboard end as a fraction of the host half-span, (span_start, 1].
    pub span_end: f64,
    /// Hinged chord fraction, (0, 0.5].
    pub chord_fraction: f64,
    /// Maximum deflection trailing-edge down, deg.
    pub max_down_deg: f64,
    /// Maximum deflection trailing-edge up, deg.
    pub max_up_deg: f64,
}

/// A tail lifting surface described explicitly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TailSurface {
    /// Total span (both halves for a horizontal or V pair) measured ALONG the
    /// surface, m: the true span, so the true planform area is
    /// `span_m * mean chord` as team reports quote it. For a dihedral pair the
    /// lateral projection is `span_m cos(dihedral)`; a fin runs along `z`.
    pub span_m: f64,
    /// Root chord, m.
    pub root_chord_m: f64,
    /// Tip chord over root chord, (0, 1.5].
    pub taper_ratio: f64,
    /// Leading-edge sweep, deg.
    pub sweep_le_deg: f64,
    /// Incidence relative to the fuselage reference, deg (positive nose-up).
    pub incidence_deg: f64,
    /// Dihedral (V-tail: half-angle from horizontal), deg.
    pub dihedral_deg: f64,
    /// Airfoil name.
    pub airfoil: String,
}

/// How the empennage is dimensioned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum EmpennageDriving {
    /// Tail volume coefficients and arm choose the areas (optimiser mode).
    Driven {
        /// Horizontal tail volume coefficient S_h l_h / (S_w c_ref), 1.
        horizontal_volume: f64,
        /// Vertical tail volume coefficient S_v l_v / (S_w b), 1.
        vertical_volume: f64,
        /// Arm from the wing aerodynamic centre to the tail aerodynamic centre, m.
        arm_m: f64,
    },
    /// Surfaces typed in as in a team's CAD model.
    Explicit {
        /// Horizontal (or V) surface; `None` for a flying wing.
        horizontal: Option<TailSurface>,
        /// Vertical surface; `None` for V-tail and flying wing.
        vertical: Option<TailSurface>,
    },
}

/// The global skeleton parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MasterParams {
    /// Root chord of the first panel at the aircraft centre line, m.
    pub root_chord_m: f64,
    /// Wing panels (1 to 4), root outwards.
    pub panels: Vec<WingPanel>,
    /// Optional winglet on the last panel.
    pub winglet: Option<Winglet>,
    /// Wing root leading-edge position, m.
    pub wing_xyz_m: [f64; 3],
    /// Fuselage reference point position, m.
    pub fuselage_xyz_m: [f64; 3],
    /// Empennage mount position, m.
    pub tail_xyz_m: [f64; 3],
    /// Tail boom, if the fuselage is a pod and boom.
    pub tail_boom: Option<TailBoom>,
    /// Control surfaces.
    pub controls: Vec<ControlSurface>,
    /// Empennage dimensioning.
    pub empennage: EmpennageDriving,
    /// Fuselage pod, gear and motor mounting (optional blocks).
    #[serde(default)]
    pub layout: LayoutParams,
}

impl MasterParams {
    /// Full span including both halves, m.
    pub fn span_m(&self) -> f64 {
        2.0 * self.panels.iter().map(|p| p.span_m).sum::<f64>()
    }
}
