// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Screening runner orchestrating Stage 1 (2-D), Stage 2 (3-D), and Stage 3 (MSES).

mod blend;
mod entry;
mod filter;
mod run;
mod stage2;
#[cfg(test)]
mod tests;
mod workers;

pub use blend::blend_scores;
pub use entry::{
    run_airfoil_screening, run_airfoil_screening_product,
    run_airfoil_screening_reference_compatibility,
};
pub use filter::filter_names;
