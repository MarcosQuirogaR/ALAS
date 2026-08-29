// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py: control-surface helpers and
// figure_control_surfaces (L1664-1991).
// Reference: alas @ rust-port-baseline.

//! Control-surface layout and tail-volume sizing figure.
//!
//! Geometry helpers, rendering, and tests live in submodules so the public
//! module path and figure API remain unchanged.

mod figure;
mod geometry;
#[cfg(test)]
mod tests;

pub use figure::figure_control_surfaces;
