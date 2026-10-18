// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Warning when the closed take-off mass leaves the band around the mass the
//! fixed engine rating was matched to (SIZ-01). Warning only: the result is
//! never rejected on this finding.

use alas_config::AlasConfig;
use alas_opt::mdo::engine_mass_match;

use super::{warning, FindingCode, PhysicalFinding};
use crate::full_analysis::AnalysisReport;

/// Append the engine/mass mismatch warning, if any.
///
/// The take-off mass is the report's mission-sized closure; an unsized report
/// has no closed mass and no verdict. The design mass is the registered
/// aircraft's declared MTOW (see [`alas_opt::mdo::engine_mass_match`]).
pub(super) fn append(
    config: &AlasConfig,
    report: &AnalysisReport,
    findings: &mut Vec<PhysicalFinding>,
) {
    let Some(closed_kg) = report.sized_takeoff_mass_kg() else {
        return;
    };
    let Some(matched) = engine_mass_match(config, closed_kg) else {
        return;
    };
    if matched.within_band() {
        return;
    }
    findings.push(warning(
        FindingCode::EngineMassMismatch,
        format!(
            "closed take-off mass {:.0} kg is {:.1} % of the {:.0} kg the fixed engine rating was matched to, outside the +/-{:.0} % band; the thrust no longer belongs to the sized aircraft",
            closed_kg,
            100.0 * matched.ratio,
            matched.engine_design_takeoff_mass_kg,
            100.0 * matched.tolerance
        ),
        Some(matched.ratio),
        Some(1.0),
        "ratio",
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feasibility::FindingSeverity;

    fn report_with_closed_mass(config: &AlasConfig, closed_kg: Option<f64>) -> AnalysisReport {
        let design = alas_config::presets::get("A320-200")
            .unwrap_or_else(|error| panic!("A320 preset: {error}"))
            .design_vector;
        let mut report = crate::full_analysis::FullAnalysis::new(config.clone())
            .run(&design, true)
            .unwrap_or_else(|error| panic!("A320 report: {error}"));
        if let Some(mass_kg) = closed_kg {
            report
                .geometry_summary
                .insert("analysis_mass_basis_is_sized".to_owned(), 1.0);
            report
                .geometry_summary
                .insert("analysis_mass_basis_kg".to_owned(), mass_kg);
        }
        report
    }

    #[test]
    fn a_closed_mass_outside_the_band_warns_and_inside_does_not() {
        let config = AlasConfig::from_value(&serde_json::json!({
            "preset": "A320-200",
            "optimizer": {"design_space": {"mode": "reference_adaptation"}}
        }))
        .unwrap_or_else(|error| panic!("A320 config: {error}"));
        let design_kg = config.requirements.mtow_kg;
        for (factor, warns) in [
            (1.0, false),
            (0.86, false),
            (1.14, false),
            (1.2, true),
            (0.8, true),
        ] {
            let report = report_with_closed_mass(&config, Some(factor * design_kg));
            let mut findings = Vec::new();
            append(&config, &report, &mut findings);
            assert_eq!(findings.len(), usize::from(warns), "factor {factor}");
            if warns {
                assert_eq!(findings[0].severity, FindingSeverity::Warning);
                assert_eq!(findings[0].code, FindingCode::EngineMassMismatch);
                assert!((findings[0].actual.unwrap_or(f64::NAN) - factor).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn an_unsized_report_has_no_closed_mass_and_no_verdict() {
        let config = AlasConfig::from_value(&serde_json::json!({
            "preset": "A320-200",
            "optimizer": {"design_space": {"mode": "reference_adaptation"}}
        }))
        .unwrap_or_else(|error| panic!("A320 config: {error}"));
        let report = report_with_closed_mass(&config, None);
        let mut findings = Vec::new();
        append(&config, &report, &mut findings);
        assert!(findings.is_empty());
    }
}
