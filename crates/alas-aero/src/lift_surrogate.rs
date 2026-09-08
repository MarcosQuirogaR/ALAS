// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission analysis model/Analyses/Aerodynamics/Vortex_Lattice.py's
// sample_training, build_surrogate and evaluate_surrogate, and the
// fuselage_correction/aircraft_total tail of Fidelity_Zero's lift chain.
// Upstream: mission analysis model 2.5.2, LGPL-2.1.
// Reference: alas @ rust-port-baseline.

//! The lift and induced-drag surrogate the mission actually flies on.
//!
//! `mission analysis model.Analyses.Aerodynamics.Vortex_Lattice` does not run a vortex lattice
//! per flight condition. It runs [`crate::vorlax`] **once**, on a fixed grid
//! of ten angles of attack against eight Mach numbers, fits a bicubic spline
//! through the result, and every lift and induced-drag number a mission
//! segment then sees is an evaluation of that spline. So this is not an
//! optimization of the solver: it is the model. A knot placed differently is
//! a different aeroplane, which is why [`alas_math::BicubicSpline`] exists as
//! its own green row rather than as "a bicubic through the same points".
//!
//! This row is also what closes `alas-aero::drag_buildup`'s open input. That
//! module takes each wing's lift coefficient and inviscid induced drag
//! coefficient as data, because with `span_efficiency` at its `None` default
//! the inviscid induced drag *is* the vortex lattice's and there is no
//! closed-form fallback on the path this program takes. [`LiftSolution`] is
//! what supplies them.
//!
//! # Scope
//!
//! `Vortex_Lattice` carries three regimes -- subsonic, supersonic, and a
//! transonic interpolant blended between them by a `Cubic_Spline_Blender`
//! over two Mach bands. Only the first is reached.
//! `Vortex_Lattice.__defaults__` trains on sixteen Mach numbers of which
//! eight are supersonic, and `Fidelity_Zero.__defaults__` then **overrides
//! that grid** with `[0.0, 0.1, 0.2, 0.3, 0.5, 0.75, 0.85, 0.9]`. With the
//! supersonic training block empty, `build_surrogate` builds neither the
//! supersonic nor the transonic surface, and `evaluate_surrogate` takes its
//! `CL_surrogate_sup == None` branch -- one spline evaluation, no blending.
//! `gen_aero_lift_surrogate.py` refuses to write a fixture in which either
//! absent surrogate has become a surface.
//!
//! Above Mach 0.9 the answer is therefore the value at Mach 0.9, because
//! FITPACK clamps its argument to the boundary knot rather than continuing
//! the edge polynomial. That is not an approximation this port introduces; it
//! is what the reference computes, and the difference between a clamp and a
//! cubic extrapolation grows without bound. The fixture evaluates outside the
//! training rectangle on all four sides for exactly that reason.
//!
//! `evaluate_no_surrogate`, which runs the vortex lattice per condition and
//! reports sectional loads and pressures, is not translated:
//! `Fidelity_Zero.__defaults__` sets `use_surrogate = True` and the mission
//! runner overrides nothing, so `initialize` never binds it.

include!("lift_surrogate_parts/part_01.rs");
include!("lift_surrogate_parts/part_02.rs");
include!("lift_surrogate_parts/part_03.rs");
