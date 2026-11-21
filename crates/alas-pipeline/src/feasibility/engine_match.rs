// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Warning when the closed take-off mass leaves the band around the mass the
//! fixed engine rating was matched to. Warning only: the result is
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

/// Append the held-engine oversize warning, if any: a held clean-sheet engine more than the rating window
/// above the thrust its own closed design requires. The numbers are those of
/// the optimizer's Diagnostic row `engine_oversize`; the design stays
/// feasible. Registered, reference and sized-engine runs have nothing to
/// compare and add nothing.
pub(super) fn append_oversize(
    config: &AlasConfig,
    design: &alas_config::DesignVector,
    findings: &mut Vec<PhysicalFinding>,
) {
    let held = match alas_opt::mdo::engine::oversize::held_engine_oversize(config, design) {
        Ok(Some(held)) => held,
        Ok(None) => return,
        Err(reason) => {
            findings.push(warning(
                FindingCode::EngineOversize,
                format!("the held-engine oversize check could not be evaluated: {reason}"),
                None,
                None,
                "",
            ));
            return;
        }
    };
    if !held.exceeds_window() {
        return;
    }
    findings.push(warning(
        FindingCode::EngineOversize,
        format!(
            "the held engine rating {:.1} {unit} is {:.1} % above the {:.1} {unit} its own design requires, beyond the {:.0} % rating window (largest matching rating {:.1} {unit}){}; warning only, the design stays feasible",
            held.rating,
            held.percent_above_requirement(),
            held.required,
            100.0 * held.window,
            held.largest_match,
            held.detail
                .as_deref()
                .map_or_else(String::new, |detail| format!(": {detail}")),
            unit = held.unit,
        ),
        Some(held.rating),
        Some(held.largest_match),
        held.unit,
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
            .unwrap_or_else(|error| panic!("E195-E2 report: {error}"));
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

#[cfg(test)]
mod oversize_tests {
    use super::*;
    use crate::feasibility::{assess_physical_feasibility, format_feasibility, FindingSeverity};

    /// The E195-E2 held on its catalogue engine (the A320's brief needs
    /// 1.028 of the CFM56 rating, so its held engine is not oversize).
    fn held_e195(window: f64) -> (AlasConfig, alas_config::DesignVector) {
        let mut config = AlasConfig::from_value(&serde_json::json!({"preset": "E195-E2"}))
            .unwrap_or_else(|error| panic!("E195-E2 config: {error}"));
        config.optimizer.design_space.mode = alas_config::DesignMode::CleanSheet;
        config.optimizer.design_space.hold_as_drawn();
        config.optimizer.design_space.engine_rating_window = window;
        let preset = alas_config::presets::get("E195-E2")
            .unwrap_or_else(|error| panic!("E195-E2 preset: {error}"));
        let design = alas_opt::canonicalize_design(&config, preset.design_vector)
            .unwrap_or_else(|error| panic!("design: {error}"));
        (config, design)
    }

    /// End to end on the E195-E2 held at a 2 percent window (its brief needs
    /// about 0.85 of the PW1900G rating): the pipeline feasibility carries a
    /// Warning finding with the numbers, the text report prints it, and the
    /// verdict is the one the default window gives (warning only).
    #[test]
    fn a_held_engine_beyond_the_window_is_a_warning_finding_with_the_numbers() {
        let (config, design) = held_e195(0.02);
        let report = crate::full_analysis::FullAnalysis::new(config.clone())
            .run(&design, true)
            .unwrap_or_else(|error| panic!("E195-E2 report: {error}"));
        let feasibility = assess_physical_feasibility(&config, &design, &report, None);
        let finding = feasibility
            .findings
            .iter()
            .find(|f| f.code == FindingCode::EngineOversize)
            .unwrap_or_else(|| panic!("the oversize warning is a finding"));
        assert_eq!(finding.severity, FindingSeverity::Warning);
        let (rating, limit) = (
            finding.actual.unwrap_or(f64::NAN),
            finding.limit.unwrap_or(f64::NAN),
        );
        assert!(rating > limit && limit > 0.0, "{finding:?}");
        assert_eq!(finding.unit, "kN");
        assert!(
            finding.message.contains("rating window"),
            "{}",
            finding.message
        );
        let text = format_feasibility(&feasibility);
        assert!(text.contains("WARNING: the held engine rating"), "{text}");

        // Warning only: inside the default window there is no finding, and the
        // verdict (errors) is identical.
        let (config20, design20) = held_e195(0.20);
        let report20 = crate::full_analysis::FullAnalysis::new(config20.clone())
            .run(&design20, true)
            .unwrap_or_else(|error| panic!("E195-E2 report: {error}"));
        let inside = assess_physical_feasibility(&config20, &design20, &report20, None);
        assert!(inside
            .findings
            .iter()
            .all(|f| f.code != FindingCode::EngineOversize));
        let errors = |r: &crate::feasibility::FeasibilityReport| {
            r.findings
                .iter()
                .filter(|f| f.severity == FindingSeverity::Error)
                .count()
        };
        assert_eq!(errors(&feasibility), errors(&inside));
    }

    /// An assessment that cannot run is reported, never skipped.
    #[test]
    fn an_oversize_check_that_cannot_be_evaluated_is_a_warning() {
        let (mut config, design) = held_e195(0.02);
        config.mass_model.mass_architecture =
            alas_config::MassArchitecture::LegacyReferenceCompatibleComparison;
        let mut findings = Vec::new();
        append_oversize(&config, &design, &mut findings);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, FindingCode::EngineOversize);
        assert_eq!(findings[0].severity, FindingSeverity::Warning);
        assert!(findings[0].message.contains("could not be evaluated"));
    }

    #[test]
    fn a_registered_aircraft_has_nothing_to_compare() {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"}))
            .unwrap_or_else(|error| panic!("A320 config: {error}"));
        let design = alas_config::presets::get("A320-200")
            .unwrap_or_else(|error| panic!("A320 preset: {error}"))
            .design_vector;
        let mut findings = Vec::new();
        append_oversize(&config, &design, &mut findings);
        assert!(findings.is_empty());
    }
}
