// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Longitudinal stability and balance.
//!
//! [`trim`] is `alas/physics/stability.py`: the two-point vortex-lattice
//! static-margin measurement, the geometry-anchored neutral point (wing+tail
//! VLM slope, tail dynamic-pressure efficiency and the fuselage Munk/Multhopp
//! contribution), the cruise-condition longitudinal trim solve, the
//! `autobalance` CG shift, and the closed-form tail-volume and Munk
//! apparent-mass correlations the two stability estimates build on.
//!
//! Everything here that reports a stability derivative runs one or more
//! vortex-lattice solves on a built `alas-geom` airplane; the
//! correlations underneath are closed-form `f64` arithmetic over the same
//! geometry and a published table.
//!
//! [`modes`] provides closed-form small-perturbation eigenmode approximations
//! (phugoid,
//! short-period, roll subsidence, dutch roll, spiral) from a
//! stability-derivative set. It runs no solve of its own -- the derivatives it
//! consumes come from a vortex-lattice sweep, but it takes them as data -- so
//! it is checked at the `closed` tier against fixed derivative sets.
//!
//! [`dynamics`] is `alas/physics/dynamics.py`: the gross-geometry inertia
//! estimate and the end-to-end path that runs a vortex-lattice
//! stability-derivative sweep on the built aircraft and feeds it to
//! [`modes::get_modes`]. Its eigenmode path carries the VLM solve, so -- like
//! [`trim`] -- the whole row is checked at a single `linalg`, even though the
//! inertia estimate alone would bear `closed`.
//!
//! [`static_stability`] provides the static branch: DATCOM lift-curve slopes,
//! downwash gradients, tube-and-wing pitching- and yaw-moment slopes, and the
//! resulting static margin and neutral point. Unlike the three rows above it
//! runs no vortex-lattice solve -- it is a closed-form correlation over the
//! mission geometry and flight condition -- so it is checked at `closed`. It
//! takes a narrow, flat input rather than a whole vehicle. See its module doc
//! for why every centre of gravity it reads is the origin.

pub mod dynamics;
pub mod modes;
pub mod static_stability;
pub mod trim;
