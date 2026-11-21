// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The held clean-sheet engine against the thrust its own closed design
//! requires, for the product surfaces (pipeline finding, report, GUI) that
//! cannot read the optimizer's Diagnostic row `engine_oversize`.

use alas_config::{AlasConfig, DesignMode, DesignVector};

/// A held clean-sheet engine and its requirement, in one unit (kN or kW).
#[derive(Debug, Clone, PartialEq)]
pub struct HeldEngineOversize {
    /// Installed rating.
    pub rating: f64,
    /// Rating the closed design requires.
    pub required: f64,
    /// Largest rating that still matches the requirement,
    /// `required / (1 - window)`.
    pub largest_match: f64,
    /// The engine rating window, fraction.
    pub window: f64,
    /// `kN` or `kW`.
    pub unit: &'static str,
    /// The row's explanation (the governing requirement), present when the
    /// engine is above the window.
    pub detail: Option<String>,
}

impl HeldEngineOversize {
    /// The rating is above the largest rating that still matches.
    #[must_use]
    pub fn exceeds_window(&self) -> bool {
        self.rating > self.largest_match
    }

    /// Percent by which the rating exceeds the requirement.
    #[must_use]
    pub fn percent_above_requirement(&self) -> f64 {
        100.0 * (self.rating / self.required - 1.0)
    }
}

/// Assess `design` as the product does and read the `engine_oversize` row.
///
/// `Ok(None)` when the engine is not a held clean-sheet engine (registered and
/// reference modes, sized engines, or no thrust requirement could be
/// evaluated): there is nothing to compare. The requirement is the one the
/// sizing search bisects over the real residual rows, evaluated on the native
/// polar; a run that fixes the drag polar externally emits the same row in
/// the optimizer's own table.
///
/// # Errors
///
/// The product assessor's description when the candidate cannot be assessed.
pub fn held_engine_oversize(
    config: &AlasConfig,
    design: &DesignVector,
) -> Result<Option<HeldEngineOversize>, String> {
    let space = &config.optimizer.design_space;
    if space.mode != DesignMode::CleanSheet || space.sizes_engine() {
        return Ok(None);
    }
    let assessment = crate::assess_product_candidate(config, design)?;
    Ok(assessment
        .residuals
        .iter()
        .find(|row| row.id == "engine_oversize")
        .map(|row| HeldEngineOversize {
            rating: row.actual,
            required: row.limit * (1.0 - space.engine_rating_window),
            largest_match: row.limit,
            window: space.engine_rating_window,
            unit: row.unit,
            detail: row.detail.clone(),
        }))
}
