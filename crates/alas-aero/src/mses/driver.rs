// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from native aerodynamic model/aerodynamics/aero_2D/mses.py (MSES.run) and
// alas/physics/mses_analysis.py (run_mses_polar, run_mses_pressure_distribution),
// scoped to the polar-sweep and single-point pressure paths this program drives.
// Upstream: native aerodynamic model 4.2.8, MIT.

//! The [`Mses`] driver: mesh once, solve each angle in turn, read the result.
//!
//! This reproduces native aerodynamic model's `MSES.run` loop: generate a mesh with
//! `mset` at the first angle, then for each angle write the `mses.case` deck,
//! run `mses`, and if it converged dump the summary with `mplot` and parse it.
//! A non-converged angle reinitializes the mesh at the next angle and moves on,
//! upstream's `behavior_after_unconverged_run="reinitialize"` default. The
//! solve continues from the previous converged state within one sweep (the
//! working directory keeps the flow solution between angles), which is why a
//! swept angle and the same angle solved alone can differ, and why the driver
//! must keep one working directory for the whole sweep.
//!
//! native aerodynamic model's `run` ends by popping `"Ma"` out of its accumulated results,
//! which raises if nothing converged; this driver has no such step, so an
//! all-non-converged sweep surfaces as an empty accumulator instead, which the
//! entry points turn into the same "did not converge" outcome upstream produces
//! from that raise.

use super::exec::{self, RunError, WorkDir};
use super::{
    deck, parse, MsesConvergedCheckpoint, MsesOsmapStatus, MsesPolarPointDiagnostic,
    MsesPolarPointStatus, MsesPolarResult, MsesPressureResult, MsesSolverAttempt, MsesStatus,
};
use alas_config::MsesConfig;
use alas_geom::aircraft::airfoil::Airfoil;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

mod osmap;
use osmap::*;
mod case;
mod session;
pub use session::*;
mod polar_io;
use polar_io::*;
// Test fixtures use `expect`/`expect_err` so malformed cases fail at the
// assertion site; this allowance is intentionally scoped to the test module.
#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests;

#[path = "driver/sweep.rs"]
mod sweep;
