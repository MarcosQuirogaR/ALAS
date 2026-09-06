// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Gradient-based design optimization over the converged sizing loop.
//!
//! The population methods in `search_methods` treat every candidate as a
//! scalar cost. A gradient-based driver needs more: the objective and each
//! inequality constraint as separate smooth quantities, so that it can
//! linearise them, solve for a step that trades objective against active
//! constraints, and measure convergence through the Karush-Kuhn-Tucker
//! conditions rather than through a population's spread.
//!
//! [`sqp`] is that driver, in the multidisciplinary-feasible architecture:
//! every evaluation it sees is a fully converged aircraft (geometry, mass,
//! trim, mission fuel and takeoff mass closed by `mdo::mda`), and only the
//! design variables are its unknowns. [`qp`] is the dense quadratic
//! subproblem solver it relies on.

mod differences;
pub mod qp;
pub mod sqp;
mod step;

pub use qp::{solve_qp, QpSolution};
pub use sqp::{run_sqp, ConstrainedEvaluator, ConstrainedPoint, SqpOutcome, SqpSettings};
