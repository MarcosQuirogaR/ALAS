// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Layout parameters: fuselage pod, landing gear and motor mounting.
//!
//! These complete the global skeleton so that the dependent geometry (bays,
//! gear and motor stations) is derived from parameters and never typed as
//! absolute coordinates. Frame: origin at the nose datum, `x` aft, `y`
//! starboard, `z` up (the `alas-geom` convention); lengths in metres.
//!
//! All fields are optional in the schema (`#[serde(default)]`): a design that
//! carries no layout still validates and still flies the M2 chain; the
//! packaging and 3D functions of `studio` report the missing block instead of
//! guessing it.

use serde::{Deserialize, Serialize};

/// A named bay of the pod, as a fraction of the pod length measured from the nose.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BayDef {
    /// Bay name; the publication is `FS_Bay_<name>`.
    pub name: String,
    /// Forward end, fraction of the pod length, [0, 1).
    pub x0_frac: f64,
    /// Aft end, fraction of the pod length, (x0_frac, 1].
    pub x1_frac: f64,
}

/// A rectangular-section pod. The nose is at `MasterParams::fuselage_xyz_m`,
/// which is also the centre of the section in `y` and `z`.
///
/// The bays are the inscribed rectangular boxes (wall thickness removed); a
/// real superellipse or rounded section holds less than its bounding box, so
/// the box is an upper bound of the usable volume.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FuselagePod {
    /// Length from the nose, m.
    pub length_m: f64,
    /// Outer width (y), m.
    pub width_m: f64,
    /// Outer height (z), m.
    pub height_m: f64,
    /// Skin and frame wall thickness, m.
    pub wall_m: f64,
    /// Bays, forward to aft; they may not overlap.
    pub bays: Vec<BayDef>,
}

/// Landing gear stations and wheel sizes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GearGeometry {
    /// Main axle x, m.
    pub main_x_m: f64,
    /// Main wheel lateral position (each side), m.
    pub main_half_track_m: f64,
    /// Main wheel diameter, m.
    pub main_wheel_diameter_m: f64,
    /// Main tyre width, m.
    pub main_wheel_width_m: f64,
    /// Tail or nose wheel x, m.
    pub aux_x_m: f64,
    /// Auxiliary wheel diameter, m.
    pub aux_wheel_diameter_m: f64,
    /// Auxiliary tyre width, m.
    pub aux_wheel_width_m: f64,
    /// Ground plane z (negative below the datum), m.
    pub ground_z_m: f64,
}

/// Motor mounting derived from the motor layout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MotorMount {
    /// Distance from the wing leading edge at the motor station forward to the
    /// propeller plane (twin layout), m. For a single motor it is the distance
    /// of the propeller plane ahead of (tractor) or behind (pusher) the pod
    /// end it is mounted on.
    pub prop_plane_offset_m: f64,
    /// Motor height above (positive) the wing leading-edge chord line, m.
    pub z_offset_m: f64,
}

/// The layout block.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LayoutParams {
    /// Fuselage pod; `None` for a flying wing without a fuselage.
    #[serde(default)]
    pub fuselage: Option<FuselagePod>,
    /// Landing gear; `None` for belly landing.
    #[serde(default)]
    pub gear: Option<GearGeometry>,
    /// Motor mounting.
    #[serde(default)]
    pub motor_mount: Option<MotorMount>,
}
