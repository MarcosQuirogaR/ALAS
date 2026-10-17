// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Airframe mass, centre of gravity, inertia, stability and control (M2 lane D).
//!
//! UAV-PLAN sections 2.2, 3.4 and 3.7.  Everything here works from the
//! [`crate::spec::UavSpec`] component tree and the `alas-uav-db` rows, and is
//! independent of the aerodynamic solver: the neutral point comes from a
//! [`stability::NeutralPointMethod`] whose default is a closed-form
//! lifting-surface estimate, to be replaced by the VLM implementation.
//!
//! # Conventions
//!
//! * SI throughout (m, kg, s, N, N*m, rad inside the physics; the spec keeps
//!   degrees and they are converted at the boundary).
//! * Body (structural) axes of the `alas-geom` convention: origin at the nose
//!   datum, `x` aft, `y` to starboard, `z` up.  Stability is therefore
//!   `x_np > x_cg` (neutral point aft of the CG) and
//!   `SM = (x_np - x_cg) / c_ref` is positive when stable.
//! * Pitching moment is positive nose-up.  Elevator, elevon and ruddervator
//!   deflection is positive trailing-edge down, which produces a positive
//!   (upward) lift increment on the surface.
//! * The inertia tensor is `I = integral (|r|^2 E - r r^T) dm` about the CG,
//!   so `I_xy = -integral x y dm` (off-diagonals carry the sign of the tensor,
//!   not of the product of inertia).  The 180 degree rotation about `y` that
//!   maps these axes onto the usual x-forward, z-down body axes leaves
//!   `I_xx`, `I_yy`, `I_zz` and `I_xz` unchanged.
//! * Mass values are tagged [`mass::MassBasis`]: `Database` (a vendor figure
//!   from `alas-uav-db`), `Weighed` (a measured override), `UserInput`, or
//!   `Estimate` (computed from areal densities, statistics or allowances).
//!   Estimates are flagged in every result; they are NOT calibrated.
//!
//! # Validity
//!
//! Subsonic, attached flow, linear lift; rigid airframe; fixed tail incidence
//! unless stated; propulsion forces are not in the neutral point (power-off
//! stability, Raymer section 16.3.1).  The mass model is for conceptual and
//! optimisation use: an uncalibrated total carries the plan's +/-8 % band
//! ([`mass::UNCALIBRATED_MASS_BAND`]).

pub mod analysis;
pub mod beam;
pub mod clt;
pub mod control;
pub mod geometry;
pub mod inertia;
mod layout;
pub mod liftcurve;
pub mod mass;
pub mod materials;
pub mod spar;
pub mod stability;
pub mod structure;
pub mod tail;
mod tree_mass;
pub mod trim;
pub mod vlm_np;
pub mod wing_buildup;

pub use control::{HingeCase, HingeMoment, ServoCheck, ServoLoad};
pub use geometry::{AirframeGeometry, Planform, TailRole, TailSurfaceGeom};
pub use inertia::{Inertia, Vec3};
pub use mass::{LoadCase, MassBasis, MassGroup, MassItem, MassStatement};
pub use stability::{
    FuselageContribution, LongitudinalOptions, NeutralPoint, NeutralPointMethod,
    SimpleLiftingSurface, StabilityPoint,
};
pub use structure::{ArealDensity, FuselageShell, StructureInputs};
pub use tail::{TailAreas, VTailMixer, VolumeCoefficients};
pub use tree_mass::{build_mass_statement, wire_mass_per_m, MassInputs, WireRun};

/// A failure of an airframe model.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum AirframeError {
    /// A required value is absent (never defaulted silently).
    #[error("{what}: required field `{field}` is missing")]
    Missing {
        /// Component, node or surface concerned.
        what: String,
        /// The absent field.
        field: &'static str,
    },
    /// An input is outside the model's validity domain.
    #[error("invalid input: {0}")]
    Invalid(String),
    /// A component id is not in the database.
    #[error("component `{0}` is not in the database")]
    UnknownComponent(String),
    /// A publication or node name does not resolve.
    #[error("unknown publication or node `{0}`")]
    UnknownName(String),
    /// The requested condition has no solution (e.g. ballast would be negative).
    #[error("no solution: {0}")]
    NoSolution(String),
}
