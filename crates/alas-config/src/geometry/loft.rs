// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The serialised description of a lofted body: the data-file schema (v1), its
//! validation, and the registry of embedded data files.
//!
//! A [`LoftSpec`] is a fuselage-class body described by five clamped B-spline
//! curves of the normalised axial coordinate `u = x / reference_length_m` in
//! `[0, 1]`, and a closed family of cross-sections evaluated from them
//! (`alas-geom::loft` owns the evaluation; this module is data and rules only):
//!
//! | curve | symbol | meaning | unit |
//! |-------|--------|---------|------|
//! | `A`   | `a^2`  | squared plan half-width | m^2 |
//! | `E`   | `e^2`  | squared half-height | m^2 |
//! | `zc`  | `zc`   | section centre height | m |
//! | `n`   | `n`    | superellipse exponent, `2 <= n <= 6` | - |
//! | `k`   | `k`    | shoulder offset, `abs(k) < 0.5` | - |
//!
//! The section at `x` is `y = a * eta`, `z = zc + e * (zeta + k * (1 - zeta^2))`
//! with `|eta|^n + |zeta|^n = 1`, in the fuselage frame (x aft of the nose tip,
//! y right, z up, metres; the z datum is the preset fuselage frame). Squares are
//! stored for `A` and `E` so that a body closing to a point has a smooth
//! (paraboloid) tip of radius `tip_radius_m` instead of a square-root edge.
//!
//! Optional minor extension (schema 1, additive): a [`ResidualSpec`] scales the
//! half-width of the upper arc only, `y = a * eta * (1 + R(zeta))`, with `R` a
//! degree-6 Bernstein polynomial (first three coefficients zero: G2 at the widest
//! point) whose four coefficients vary along the body as clamped B-splines. It
//! leaves `z`, the crown, keel, plan and side silhouettes unchanged; files
//! without it are unchanged. It exists for the Boeing 747 upper-deck hump.
//!
//! A [`LoftRef`] on [`super::FuselageConfig`] names an embedded data file under
//! `crates/alas-config/data/lofts/<id>.json` (see [`registry`]); `None` keeps
//! the parametric fuselage laws unchanged.

#[cfg(test)]
mod data_tests;
mod registry;
mod residual;
mod spec;
mod validate;

pub use registry::{embedded_loft, embedded_loft_ids, parse_loft_spec, preset_loft_id, LoftRef};
pub use residual::{
    residual_basis, residual_basis_dt, residual_slope, residual_value, ResidualSpec,
    MAX_RESIDUAL_TERMS, MIN_WIDTH_FACTOR, RESIDUAL_COEFF_LIMIT,
};
pub use spec::{
    BodyCurves, CabinStations, Closure, Confidence, DrawingRef, FairnessBudget, FitRecord,
    InflectionBudget, LoftSpec, ProvenanceKind, SourceRef, SplineSpec, LOFT_SCHEMA_VERSION,
};
pub use validate::{
    LoftSpecError, SplineProblem, MAX_COEFFS_PER_CURVE, MAX_EXPONENT, MAX_TIP_RADIUS_RATIO,
    MIN_EXPONENT, SHOULDER_LIMIT,
};
