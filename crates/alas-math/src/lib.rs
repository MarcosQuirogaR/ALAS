// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Numerical primitives that several other crates need but that no single
//! Python module owns: pseudospectral operators, splines, small dense linear
//! algebra. Each one gets its own submodule rather than its own crate,
//! because they are all "a few hundred lines of numerics with no physics in
//! them" and splitting further would scatter that.
//!
//! This module holds the Chebyshev pseudospectral operator, the cubic spline
//! through a sequence of points, the interpolating cubic B-spline, the
//! interpolating bicubic surface over a rectangular grid, NumPy's
//! piecewise-linear [`interp`], and the two dense solves everything above is
//! built on: [`linalg::solve`], the square Gaussian elimination the splines'
//! collocation systems and a vortex-lattice AIC matrix both use, and
//! [`lstsq::least_squares`], the overdetermined Householder-QR fit that a
//! curve-fitting caller needs and that a square solve cannot answer.
//!
//! Two of those are both "a cubic through the same points" and are not
//! interchangeable. [`CubicSpline`] takes an explicit first- or
//! second-derivative condition at each end and is the object SciPy's
//! `CubicSpline` builds; [`CubicBSpline`] takes not-a-knot end conditions and
//! is the object CasADi builds, which is what every native aerodynamic model surrogate and
//! its default atmosphere are made of. They differ by a per-cent-scale amount
//! near the ends of the data, so a caller picks the one its upstream picked.

mod bicubic;
mod bspline;
mod chebyshev;
pub mod hybrd;
mod interp;
pub mod linalg;
pub mod lstsq;
mod spline;

pub use bicubic::{Axis, BicubicSpline, BicubicSplineError};
pub use bspline::{CubicBSpline, CubicBSplineError};
pub use chebyshev::{chebyshev_data, ChebyshevData, ChebyshevError};
pub use interp::interp;
pub use spline::{Boundary, CubicSpline, CubicSplineError};
