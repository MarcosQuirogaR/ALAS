// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Plausibility flags comparing the delivered wing with the baseline's.
//! They are labels only: no constraint reads them.

use crate::AnalysisReport;

/// The model omits aeroelastic weight and stiffness penalties, so a gain
/// bought with span or aspect ratio is an upper bound.
pub const AEROELASTIC_CAVEAT_TEXT: &str = "Span and aspect-ratio gains exclude aeroelastic penalties (flutter, aileron reversal); treat aspect-ratio increases above the reference aircraft as optimistic";

/// The buffet margin's basis: a drag-divergence estimate, not a measured
/// buffet boundary, so it only holds a design to its registered aircraft.
pub const BUFFET_BASIS_TEXT: &str = "Korn-inverse estimate, not a buffet boundary; validated error against Fokker 100 flight test about 0.3-0.65 CL; it only holds the design to the registered aircraft's own margin";

/// Label of the aspect-ratio plausibility flag; its value is
/// `result / reference`.
pub const ASPECT_RATIO_FLAG_LABEL: &str = "Aspect ratio above reference";

/// Label of the sweep plausibility flag; its value is `result / reference`.
pub const SWEEP_FLAG_LABEL: &str = "Quarter-chord sweep below reference";

/// Relative aspect-ratio increase over the baseline above which the gain is
/// flagged. Engineering choice, the same for every aircraft: the model has
/// no aeroelastic penalty, so a gain beyond a tenth of the reference is the
/// regime where that omission is most likely to flatter the result.
pub const ASPECT_RATIO_FLAG_FRACTION: f64 = 0.10;

/// Quarter-chord sweep reduction from the baseline, degrees, beyond which
/// the change is flagged. Engineering choice, the same for every aircraft:
/// less sweep raises the section Mach seen at the cruise point, so the wave
/// drag model is used further from its reference.
pub const SWEEP_FLAG_DEG: f64 = 3.0;

/// Aspect ratio and quarter-chord sweep of the delivered and baseline
/// designs, from the geometry the reports were built on. Aspect ratio is
/// the reports' `geometry_summary["aspect_ratio"]` (dimensionless); sweep is
/// `AeroAnalysis::quarter_chord_sweep_deg` of the built aircraft, degrees,
/// positive aft.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeometryComparison {
    /// Baseline wing aspect ratio.
    pub baseline_aspect_ratio: f64,
    /// Delivered wing aspect ratio.
    pub winner_aspect_ratio: f64,
    /// Baseline quarter-chord sweep, degrees.
    pub baseline_sweep_deg: f64,
    /// Delivered quarter-chord sweep, degrees.
    pub winner_sweep_deg: f64,
}

impl GeometryComparison {
    /// The comparison of two reports; `None` when either lacks a finite
    /// positive aspect ratio.
    #[must_use]
    pub fn of_reports(winner: &AnalysisReport, baseline: &AnalysisReport) -> Option<Self> {
        Some(Self {
            baseline_aspect_ratio: report_aspect_ratio(baseline)?,
            winner_aspect_ratio: report_aspect_ratio(winner)?,
            baseline_sweep_deg: report_quarter_chord_sweep_deg(baseline),
            winner_sweep_deg: report_quarter_chord_sweep_deg(winner),
        })
    }

    /// The delivered aspect ratio exceeds the baseline's by more than
    /// [`ASPECT_RATIO_FLAG_FRACTION`]. False for non-finite or non-positive
    /// values.
    #[must_use]
    pub fn aspect_ratio_above_reference(&self) -> bool {
        self.baseline_aspect_ratio.is_finite()
            && self.baseline_aspect_ratio > 0.0
            && self.winner_aspect_ratio.is_finite()
            && self.winner_aspect_ratio
                > self.baseline_aspect_ratio * (1.0 + ASPECT_RATIO_FLAG_FRACTION)
    }

    /// The delivered quarter-chord sweep is more than [`SWEEP_FLAG_DEG`]
    /// below the baseline's. False for non-finite values.
    #[must_use]
    pub fn sweep_below_reference(&self) -> bool {
        self.baseline_sweep_deg.is_finite()
            && self.winner_sweep_deg.is_finite()
            && self.winner_sweep_deg < self.baseline_sweep_deg - SWEEP_FLAG_DEG
    }

    /// `(label, "result / reference")` for each flag that fires; empty when
    /// neither does.
    #[must_use]
    pub fn flag_lines(&self) -> Vec<(&'static str, String)> {
        let mut lines = Vec::new();
        if self.aspect_ratio_above_reference() {
            lines.push((
                ASPECT_RATIO_FLAG_LABEL,
                format!(
                    "{:.2} / {:.2}",
                    self.winner_aspect_ratio, self.baseline_aspect_ratio
                ),
            ));
        }
        if self.sweep_below_reference() {
            lines.push((
                SWEEP_FLAG_LABEL,
                format!(
                    "{:.1} deg / {:.1} deg",
                    self.winner_sweep_deg, self.baseline_sweep_deg
                ),
            ));
        }
        lines
    }
}

fn report_aspect_ratio(report: &AnalysisReport) -> Option<f64> {
    report
        .geometry_summary
        .get("aspect_ratio")
        .copied()
        .filter(|value| value.is_finite() && *value > 0.0)
}

fn report_quarter_chord_sweep_deg(report: &AnalysisReport) -> f64 {
    alas_aero::analysis::AeroAnalysis::quarter_chord_sweep_deg(
        &report.airplane,
        report.design.sweep_deg,
    )
}
