// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! UAV studio core (M3): skeleton publications, topology library, packaging,
//! markers and live feasibility. Pure data and physics; the GUI only draws it.
//!
//! # Conventions
//!
//! Body axes of `alas-geom`: origin at the nose datum, `x` aft, `y` starboard,
//! `z` up; SI (m, kg, N); spec angles in degrees, converted at the boundary.
//! Static margin is `(x_np - x_cg) / c_ref`, positive when stable.
//!
//! | module | content |
//! |---|---|
//! | [`geom`] | stations on planforms (leading edge, chord, hinge points) |
//! | [`publish`] | publications derived from the master parameters (never stored by hand) |
//! | [`topology`] | library of configurations built from a few sizing numbers |
//! | [`pack`], [`check`] | placement of database parts into bays; volume, collision and propeller-disc checks |
//! | [`markers`], [`scene`] | CG and neutral-point markers, wing fitting for a margin, component-box scene data |
//! | [`hull`] | fuselage, gear and nacelle drag items derived from the layout |
//! | [`feasibility`] | L0 (closed form) and L1 (coupled chain) traffic lights |
//! | [`reference`] | the Xtra2UPV XTRA28 reference aircraft from a spec |

pub mod check;
pub mod feasibility;
pub mod geom;
pub mod hull;
pub mod lights;
pub mod markers;
pub mod pack;
pub mod publish;
pub mod reference;
pub mod scene;
pub mod topology;

use crate::airframe::AirframeError;

/// A failure of a studio function.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum StudioError {
    /// A block the function needs is absent from the spec (never guessed).
    #[error("the spec has no `{0}`")]
    Missing(&'static str),
    /// A parameter combination is not valid.
    #[error("invalid: {0}")]
    Invalid(String),
    /// A name does not resolve.
    #[error("unknown name `{0}`")]
    UnknownName(String),
    /// Airframe model failure.
    #[error(transparent)]
    Airframe(#[from] AirframeError),
    /// Integrated chain failure.
    #[error(transparent)]
    Coupled(#[from] crate::coupled::CoupledError),
    /// Powertrain failure.
    #[error(transparent)]
    Power(#[from] crate::power::PowerError),
    /// Mission failure.
    #[error(transparent)]
    Perf(#[from] crate::perf::PerfError),
}
