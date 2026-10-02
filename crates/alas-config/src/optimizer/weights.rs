// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/optimizer_config.py (`ObjectiveWeights`)

//! The transport-planform thresholds and failure cost the optimizer reads.
//!
//! The mission-sized objective ranks feasibility first, so most of what lives
//! here is a limit on the planform or tail geometry rather than a weight; the
//! one scalar cost it reads is the flat [`ObjectiveWeights::failure_cost`] of a
//! candidate that cannot be evaluated at all.
//!
//! Fields whose names end in a weight suffix are offered as sliders rather
//! than as numbers. That rule catches the failure cost, which is not a
//! weight, and is reproduced rather than corrected.

mod defaults;
mod fields;
#[cfg(test)]
mod tests;

pub use fields::ObjectiveWeights;
