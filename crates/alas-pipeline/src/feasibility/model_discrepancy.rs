//! A registered aircraft is feasible by certification.
//!
//! In a baseline mode, a registered preset is analysed with its own
//! geometry: `BaselineSandbox`, or `ReferenceAdaptation` while the design
//! vector is still the registered one. There a CG, tail-scrape, tip-back,
//! static-margin or gear gate that the model fails is a discrepancy between
//! the model and a certified aeroplane, not a physical infeasibility. Each
//! such finding is kept as a warning. It carries the model value
//! (`actual`), the model's own limit (`limit`), the published evidence and
//! its source. Optimisation, clean-sheet and adapted designs keep the hard
//! gate.

use alas_config::{presets, AlasConfig, DesignMode, DesignVector};

use super::{DesignProvenance, FindingCode, FindingSeverity, PhysicalFinding};

/// The certified gates covered: CG lines, tail scrape, tip-back, the
/// static-margin floor and the landing-gear checks.
const CERTIFIED_GATES: [FindingCode; 9] = [
    FindingCode::CgEnvelopeViolation,
    FindingCode::ModelCgForwardRangeViolation,
    FindingCode::PublicPlanningCgEnvelopeViolation,
    FindingCode::NoseGearStrengthViolation,
    FindingCode::MainGearStrengthViolation,
    FindingCode::MinimumNoseGearLoadViolation,
    FindingCode::TipBackViolation,
    FindingCode::TailScrapeViolation,
    FindingCode::InsufficientStaticMargin,
];

/// Whether `code` is one of the gates a certified aeroplane passes by
/// certification.
#[must_use]
pub(super) fn is_certified_gate(code: FindingCode) -> bool {
    CERTIFIED_GATES.contains(&code)
}

/// The published evidence for a registered preset's certified gates: the
/// planning CG envelope where one is published, with its forward and aft
/// extremes, else the type's revision-locked sources. `None` for a design
/// that is not the registered aeroplane in a baseline mode.
fn certified_evidence(config: &AlasConfig, design: &DesignVector) -> Option<String> {
    if !matches!(
        config.optimizer.design_space.mode,
        DesignMode::BaselineSandbox | DesignMode::ReferenceAdaptation
    ) {
        return None;
    }
    if !matches!(
        DesignProvenance::of(config, design),
        DesignProvenance::CalibratedReference { .. }
    ) {
        return None;
    }
    let preset = presets::get(&config.preset).ok()?;
    let reference = &preset.reference;
    // The type certificate is the certification evidence; another document
    // stands in only where none is registered.
    let sources = reference
        .sources
        .iter()
        .find(|source| source.starts_with("EASA.") || source.contains("TCDS"))
        .or_else(|| reference.sources.first())
        .map_or_else(|| "the type certificate".to_owned(), |source| (*source).to_owned());
    let planning = reference.planning_cg_envelope.as_ref().and_then(|envelope| {
        let vertices = envelope.vertices();
        let forward = vertices
            .iter()
            .map(|vertex| vertex.flight.forward_pct_mac.min(vertex.ground.forward_pct_mac))
            .fold(f64::INFINITY, f64::min);
        let aft = vertices
            .iter()
            .flat_map(|vertex| [vertex.flight.aft_pct_mac, vertex.ground.aft_pct_mac])
            .flatten()
            .fold(f64::NEG_INFINITY, f64::max);
        (forward.is_finite() && aft.is_finite()).then(|| {
            format!(
                "published CG {forward:.1} to {aft:.1} %MAC ({}, {}, {})",
                envelope.source.document, envelope.source.revision, envelope.source.location
            )
        })
    });
    Some(match planning {
        Some(planning) => format!(
            "the certified {} passes this gate ({planning}; type source: {sources})",
            preset.name
        ),
        None => format!(
            "the certified {} passes this gate (type source: {sources})",
            preset.name
        ),
    })
}

/// Demote the certified gates in `findings` to warnings for a registered
/// preset analysed in a baseline mode, recording the model value, the
/// published evidence and its source in the message. Other runs and other
/// findings are left unchanged.
pub(super) fn demote_certified_gates(
    config: &AlasConfig,
    design: &DesignVector,
    findings: &mut [PhysicalFinding],
) {
    if !findings
        .iter()
        .any(|finding| finding.severity == FindingSeverity::Error && is_certified_gate(finding.code))
    {
        return;
    }
    let Some(evidence) = certified_evidence(config, design) else {
        return;
    };
    for finding in findings
        .iter_mut()
        .filter(|finding| finding.severity == FindingSeverity::Error && is_certified_gate(finding.code))
    {
        let model = match (finding.actual, finding.limit) {
            (Some(actual), Some(limit)) => {
                format!("model {actual:.3} against model limit {limit:.3} {}", finding.unit)
            }
            (Some(actual), None) => format!("model {actual:.3} {}", finding.unit),
            _ => "model value not quantified".to_owned(),
        };
        finding.severity = FindingSeverity::Warning;
        finding.message = format!(
            "model discrepancy, not infeasibility: {}; {model}; {evidence}",
            finding.message
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(preset: &str, mode: DesignMode) -> AlasConfig {
        let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
            .unwrap_or_else(|error| panic!("{error}"));
        config.optimizer.design_space.mode = mode;
        config
    }

    fn registered_design(preset: &str) -> DesignVector {
        presets::get(preset)
            .unwrap_or_else(|error| panic!("{error}"))
            .design_vector
    }

    fn finding(code: FindingCode) -> PhysicalFinding {
        PhysicalFinding {
            code,
            severity: FindingSeverity::Error,
            message: "gate failed".to_owned(),
            actual: Some(12.0),
            limit: Some(4.5),
            unit: "deg",
        }
    }

    fn assessed(config: &AlasConfig, design: &DesignVector) -> Vec<PhysicalFinding> {
        let mut findings = vec![
            finding(FindingCode::TailScrapeViolation),
            finding(FindingCode::ModelCgForwardRangeViolation),
            finding(FindingCode::MissionFuelShortfall),
        ];
        demote_certified_gates(config, design, &mut findings);
        findings
    }

    /// A registered aeroplane in either baseline mode keeps every certified
    /// gate as a warning that carries the model value and the published
    /// source; a finding outside these gates stays an error.
    #[test]
    fn a_registered_baseline_reports_failed_certified_gates_as_model_discrepancies() {
        for (preset, planning) in [("A320-200", false), ("A220-300", true)] {
            for mode in [DesignMode::BaselineSandbox, DesignMode::ReferenceAdaptation] {
                let config = config(preset, mode);
                let findings = assessed(&config, &registered_design(preset));
                for gate in &findings[..2] {
                    assert_eq!(gate.severity, FindingSeverity::Warning, "{mode:?}");
                    let message = &gate.message;
                    assert!(message.starts_with("model discrepancy"), "{message}");
                    assert!(message.contains("model 12.000 against model limit 4.500 deg"));
                    assert!(message.contains(&format!("certified {preset}")), "{message}");
                    assert!(message.contains("type source: "), "{message}");
                    assert_eq!(message.contains("published CG"), planning, "{message}");
                    assert_eq!((gate.actual, gate.limit), (Some(12.0), Some(4.5)));
                }
                assert_eq!(findings[2].severity, FindingSeverity::Error);
            }
        }
    }

    /// Optimisation and clean-sheet runs, and a reference adaptation that
    /// moved the design vector, keep the hard gate.
    #[test]
    fn designs_and_clean_sheet_runs_keep_the_hard_gates() {
        let registered = registered_design("A320-200");
        let clean = config("A320-200", DesignMode::CleanSheet);
        assert!(assessed(&clean, &registered)
            .iter()
            .all(|finding| finding.severity == FindingSeverity::Error));
        let mut moved = registered;
        moved.span_m += 1.0;
        for mode in [DesignMode::BaselineSandbox, DesignMode::ReferenceAdaptation] {
            assert!(assessed(&config("A320-200", mode), &moved)
                .iter()
                .all(|finding| finding.severity == FindingSeverity::Error));
        }
    }

    /// AVE is registered as a design requirement, not a certified aeroplane,
    /// so its gates stay hard in a baseline mode too.
    #[test]
    fn the_notional_reference_keeps_the_hard_gates() {
        let config = config("AVE", DesignMode::BaselineSandbox);
        assert!(assessed(&config, &registered_design("AVE"))
            .iter()
            .all(|finding| finding.severity == FindingSeverity::Error));
    }
}
