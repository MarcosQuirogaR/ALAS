// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py:figure_cg_envelope (L2354-2835)
// Reference: alas @ rust-port-baseline.

//! Model-derived CG loading-state check: `%MAC` vs weight.
//!
//! The implementation is split into calculation helpers, figure rendering,
//! and tests so the translated figure remains easy to audit.

mod figure;
mod helpers;
#[cfg(test)]
// These tests intentionally panic if their constructed fixture violates its precondition.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

pub use figure::figure_cg_envelope;
