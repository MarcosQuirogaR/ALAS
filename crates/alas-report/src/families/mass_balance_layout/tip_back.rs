// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The tip-back basis of the landing-gear figure.

use alas_config::AlasConfig;
use alas_pipeline::full_analysis::AnalysisReport;

/// The tip-back basis of the figure from the CG envelope assessment already
/// computed for the figure's limits: the most aft reachable ground CG, the
/// analysed takeoff state's ground-referenced CG height (the envelope's own
/// basis) and the scrape angle.
pub(super) fn basis(
    report: &AnalysisReport,
    config: &AlasConfig,
    assessment: Option<&alas_opt::ModelCgEnvelopeAssessment>,
) -> alas_perf::landing_gear::geometry::TipBackBasis {
    let Some(assessment) = assessment else {
        return alas_perf::landing_gear::geometry::TipBackBasis::default();
    };
    let envelope =
        alas_pipeline::feasibility::assess_operational_envelope(config, report, assessment);
    alas_pipeline::feasibility::tip_back_basis(report, assessment, envelope.as_ref())
        .map_or_else(Default::default, |(basis, _)| basis)
}

/// The minimum static nose share the gear is sized at: the class minimum.
pub(super) fn min_nose_fraction(config: &AlasConfig) -> f64 {
    config.mass_model.pct_load_nlg_min
}
