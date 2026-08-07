// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Numerical primitives that several other crates need but that no single
//! Python module owns: pseudospectral operators, splines, small dense linear
//! algebra. Each one gets its own submodule rather than its own crate,
//! because they are all "a few hundred lines of numerics with no physics in
//! them" and splitting further would scatter that.
//!
//! This module holds the Chebyshev pseudospectral operator, the cubic spline
//! through a sequence of points, the interpolating cubic B-spline, and the
//! interpolating bicubic surface over a rectangular grid.
//!
//! Two of those are both "a cubic through the same points" and are not
//! interchangeable. [`CubicSpline`] takes an explicit first- or
//! second-derivative condition at each end and is the object SciPy's
//! `CubicSpline` builds; [`CubicBSpline`] takes not-a-knot end conditions and
//! is the object CasADi builds, which is what every AeroSandbox surrogate and
//! its default atmosphere are made of. They differ by a per-cent-scale amount
//! near the ends of the data, so a caller picks the one its upstream picked.

mod bicubic;
mod bspline;
mod chebyshev;
mod spline;

pub use bicubic::{Axis, BicubicSpline, BicubicSplineError};
pub use bspline::{CubicBSpline, CubicBSplineError};
pub use chebyshev::{chebyshev_data, ChebyshevData, ChebyshevError};
pub use spline::{Boundary, CubicSpline, CubicSplineError};
