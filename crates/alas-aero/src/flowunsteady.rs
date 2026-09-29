// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Strict interchange for an optional FLOWUnsteady adapter.
//!
//! FLOWUnsteady is a Julia package, not code linked into ALAS.  Its public
//! API is intentionally not guessed here: a user-supplied adapter receives a
//! retained SI request and writes this versioned result file.  The explicit
//! reference/frame declarations prevent an unlabelled time history from being
//! drawn as an ALAS aircraft polar.

use std::fmt::Write as FmtWrite;

mod types;
pub use types::*;
mod wire;
pub use wire::*;
#[cfg(test)]
mod tests;
