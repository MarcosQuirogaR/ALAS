// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Re-check the cruise-attitude window on the mesh the run actually reports.
//!
//! The optimizer constrains the trimmed body attitude to
//! `[geometric_body_alpha_min_deg, geometric_body_alpha_max_deg]`, and it does
//! so on `AnalysisConfig::spanwise_resolution`/`chordwise_resolution`: the
//! in-loop mesh. The published analysis then re-solves the winner on the
//! finer `fine_*` mesh and prints that number instead. The two do not agree,
//! and the difference is not small: the chordwise convergence is first-order
//! in panel count, so a supercritical wing reported at sixteen chordwise
//! panels still sits about half a degree from the extrapolated limit, and the
//! search mesh sits further still (`.agent/reports/
//! 2026-09-11-vlm-resolution-sensitivity.html`).
//!
//! Before this check existed nothing closed that loop: `feasibility` tested
//! the trimmed point for finiteness only, so a candidate selected because its
//! in-loop attitude fell inside the window could be published with a reported
//! attitude outside it, and the report would not say so. The constraint is
//! the optimizer's, but the number a reader sees is this one, so this is
//! where the two have to be reconciled.
//!
//! Scope matches the optimizer's own: clean-sheet passenger searches with the
//! transport planform constraints active. A registered aircraft keeps its
//! measured attitude for audit, applying a generic design target to a real
//! airframe would rewrite the reference rather than test it, and so does a
//! run whose constraints are switched off.

use alas_config::{AlasConfig, DesignMode};

use super::types::{FindingCode, FindingSeverity, PhysicalFinding};
use crate::full_analysis::AnalysisReport;

/// Whether the cruise-attitude window applies to this run at all.
///
/// The three conditions are the optimizer's, read from the same fields, so
/// the report cannot start policing a window the search never enforced.
fn window_is_enforced(config: &AlasConfig) -> bool {
    config.optimizer.design_space.mode == DesignMode::CleanSheet
        && config.requirements.aircraft_type == "passenger"
        && config
            .optimizer
            .weights
            .transport_planform_constraints_enabled
}

/// One finding when the reported cruise attitude leaves the window the
/// candidate was selected inside, and none otherwise.
pub(crate) fn assess(config: &AlasConfig, report: &AnalysisReport) -> Vec<PhysicalFinding> {
    if !window_is_enforced(config) {
        return Vec::new();
    }
    let Some(trim) = report.trimmed_design_point.as_ref() else {
        return Vec::new();
    };
    let alpha = trim.geometric_body_alpha_deg;
    if !alpha.is_finite() {
        // A non-finite trim is already reported as `TrimUnavailable`; saying
        // it twice in different words helps nobody.
        return Vec::new();
    }

    let weights = &config.optimizer.weights;
    let (min, max) = (
        weights.geometric_body_alpha_min_deg,
        weights.geometric_body_alpha_max_deg,
    );
    if (min..=max).contains(&alpha) {
        return Vec::new();
    }

    let limit = if alpha < min { min } else { max };
    let side = if alpha < min { "below" } else { "above" };
    vec![PhysicalFinding {
        code: FindingCode::ReportedCruiseAttitudeOutsideWindow,
        // A *design-preference* window, not a statement that the aircraft
        // cannot fly. `[2.0, 4.0]` deg is a configurable convention for a
        // transport's cruise floor attitude, and six of the eight registered
        // aircraft miss it at their own nominal design with no optimizer
        // running: a check that rejects six real aeroplanes is measuring the
        // convention, not the aeroplanes. An aircraft trimmed at 1.7 deg body
        // attitude is trimmed. So this is reported, prominently and with its
        // numbers, as a warning rather than as physical infeasibility.
        //
        // This is not a relaxation and nothing about the search changes: the
        // `geometric_body_alpha` residual stays in the optimizer's Geometry
        // family under that family's configured policy, hard by default, so a
        // candidate outside the window is still rejected by the search.
        severity: FindingSeverity::Warning,
        message: format!(
            "the reported cruise body attitude ({alpha:.2} deg) lies {side} the \
             design window [{min:.2}, {max:.2}] deg this configuration declares. \
             That window is a design preference for a transport's cruise \
             attitude, not a flight limit. When a search runs it evaluates the \
             window on the in-loop panel mesh while this analysis re-solves it \
             on the finer reported mesh, so a candidate selected inside the \
             window can be reported outside it by roughly the coarser mesh's \
             own error; raise analysis.chordwise_resolution toward \
             analysis.fine_chordwise_resolution to close that gap. Without a \
             search this is simply the analysed design's own reported attitude."
        ),
        actual: Some(alpha),
        limit: Some(limit),
        unit: "deg",
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(config: &mut AlasConfig) {
        config.optimizer.design_space.mode = DesignMode::CleanSheet;
        config.requirements.aircraft_type = "passenger".to_owned();
        config
            .optimizer
            .weights
            .transport_planform_constraints_enabled = true;
        config.optimizer.weights.geometric_body_alpha_min_deg = 2.0;
        config.optimizer.weights.geometric_body_alpha_max_deg = 4.0;
    }

    #[test]
    fn a_run_outside_the_optimizer_scope_is_not_policed() {
        let mut config = AlasConfig::default();
        window(&mut config);
        config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
        assert!(!window_is_enforced(&config));

        let mut config = AlasConfig::default();
        window(&mut config);
        config.requirements.aircraft_type = "freighter".to_owned();
        assert!(!window_is_enforced(&config));

        let mut config = AlasConfig::default();
        window(&mut config);
        config
            .optimizer
            .weights
            .transport_planform_constraints_enabled = false;
        assert!(!window_is_enforced(&config));
    }

    #[test]
    fn the_clean_sheet_passenger_search_is_policed() {
        let mut config = AlasConfig::default();
        window(&mut config);
        assert!(window_is_enforced(&config));
    }
}
