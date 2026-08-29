// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Retained solver-branch identity for result-scene data selection.
//!
//! The Results page does not expose a global solver switch: Model Comparison
//! presents each method's scope and status in its own tab instead.

/// Whole-aircraft aerodynamic source or view selected by a caller.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SolverResultView {
    /// The primary in-process ALAS aerodynamic report.
    #[default]
    Vlm,
    /// The retained AVL result from an independently optimized branch.
    Avl,
    /// The figure overlaying retained ALAS VLM and Athena AVL results.
    Comparison,
}
