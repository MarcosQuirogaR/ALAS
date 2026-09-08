// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Geometry-resolving Prandtl lifting-line analysis.
//!
//! This is an independent finite-wing model, not a panel or vortex-lattice
//! method. Each symmetric lifting surface is reduced to a spanwise lifting
//! line and its circulation is expanded in odd Fourier harmonics. The dense
//! collocation system is the one derived in MIT 16.100 Lectures 17-18:
//!
//! `alpha - alpha_l0 = sum(A_n sin(n theta) [4 b / (a0 c) + n / sin(theta)])`.
//!
//! Section zero-lift angles come from the actual airfoil mean-camber lines by
//! thin-airfoil theory. Surface lift and Trefftz-plane induced drag are then
//! normalized to the aircraft reference area before isolated-surface results
//! are summed. No VLM output, fitted aircraft polar, or expected result enters
//! the calculation.
//!
//! The method assumes steady, attached, incompressible flow; straight lifting
//! lines; small angles; and no aerodynamic interference between surfaces. It
//! does not predict profile or wave drag, pitching moment, stall, or fuselage
//! lift. Sweep and dihedral remain visible in the source geometry but are not
//! corrections in the governing equation.
//!
//! References:
//! - D. Darmofal, MIT 16.100, Lectures 17-18, "Prandtl's Lifting Line" and
//!   "Force Calculations for Lifting Line," 2005.
//! - L. Prandtl, NACA-TN-182, "Induced Drag of Multiplanes," 1924.

include!("fourier_lifting_line_parts/part_01.rs");
include!("fourier_lifting_line_parts/part_02.rs");
