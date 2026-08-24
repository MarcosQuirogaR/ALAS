// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Text rendering for physical-feasibility evidence.

use alas_config::CgEnvelopeEvidence;
use alas_opt::ModelCgEnvelopeAssessment;

use super::{
    cruise_equilibrium, CgEnvelopeAssessment, FeasibilityReport, FindingSeverity, PlanningCgStatus,
};

/// Format a feasibility report for terminal, text-report and PDF-summary use.
pub fn format_feasibility(report: &FeasibilityReport) -> String {
    let mut lines = if report.is_feasible() {
        vec!["Physical status     : FEASIBLE under implemented checks".to_owned()]
    } else {
        let error_count = report
            .findings
            .iter()
            .filter(|finding| finding.severity == FindingSeverity::Error)
            .count();
        vec![format!(
            "Physical status     : INFEASIBLE ({} finding(s))",
            error_count
        )]
    };
    for finding in &report.findings {
        let detail = match (finding.actual, finding.limit) {
            (Some(actual), Some(limit)) if !finding.unit.is_empty() => format!(
                "{} (actual {:.3} {}, limit {:.3} {})",
                finding.message, actual, finding.unit, limit, finding.unit
            ),
            _ => finding.message.clone(),
        };
        let severity = match finding.severity {
            FindingSeverity::Error => "ERROR",
            FindingSeverity::Warning => "WARNING",
        };
        lines.push(format!("  - {severity}: {detail}"));
    }
    lines.push(format_model_cg_assessment(report.model_cg.as_ref()));
    lines.push(format_cg_assessment(&report.cg_envelope));
    lines.push(cruise_equilibrium::format(
        report.cruise_equilibrium.as_ref(),
    ));
    lines.join("\n")
}

fn format_model_cg_assessment(assessment: Option<&ModelCgEnvelopeAssessment>) -> String {
    let Some(assessment) = assessment else {
        return "Model CG status     : NOT EVALUATED".to_owned();
    };
    let failed_constraints = assessment
        .loading_states
        .iter()
        .flat_map(|state| &state.constraints)
        .filter(|constraint| constraint.violated)
        .count();
    let status = if failed_constraints == 0 {
        "HARD CONSTRAINTS PASS".to_owned()
    } else {
        format!("HARD CONSTRAINTS FAIL ({failed_constraints})")
    };
    let target_status = if assessment.target_static_margin.met_or_exceeded() {
        "at/above preference"
    } else {
        "below preference"
    };
    format!(
        "Model CG status     : {status}; analyzed-TOW SM {:.1}%, hard floor {:.1}%, target preference {:.1}% ({target_status})",
        100.0 * assessment.target_static_margin.actual,
        100.0 * assessment.minimum_physical_static_margin,
        100.0 * assessment.target_static_margin.target,
    )
}

fn format_cg_assessment(assessment: &CgEnvelopeAssessment) -> String {
    let prefix = "CG reference status :";
    match assessment.planning_status {
        PlanningCgStatus::WithinPublishedLimits => format!(
            "{prefix} WITHIN PUBLIC PLANNING LIMITS; {} controls",
            assessment
                .controlling_document
                .unwrap_or("actual aircraft WBM")
        ),
        PlanningCgStatus::ForwardLimitViolation => format!(
            "{prefix} FORWARD OF PUBLIC PLANNING LIMIT; {} controls",
            assessment
                .controlling_document
                .unwrap_or("actual aircraft WBM")
        ),
        PlanningCgStatus::AftLimitViolation => format!(
            "{prefix} AFT OF PUBLIC PLANNING LIMIT; {} controls",
            assessment
                .controlling_document
                .unwrap_or("actual aircraft WBM")
        ),
        PlanningCgStatus::AftLimitNotPublished => format!(
            "{prefix} PUBLIC PLANNING AFT LIMIT NOT PUBLISHED; {} controls",
            assessment
                .controlling_document
                .unwrap_or("actual aircraft WBM")
        ),
        PlanningCgStatus::NotEvaluated => match assessment.evidence {
            CgEnvelopeEvidence::AfmRequired => {
                format!("{prefix} NOT EVALUATED - AFM/WBM limits required")
            }
            CgEnvelopeEvidence::DesignRequirement => {
                format!("{prefix} NOTIONAL DESIGN REQUIREMENT")
            }
            CgEnvelopeEvidence::PublicPlanning => {
                format!("{prefix} PUBLIC PLANNING LIMITS NOT EVALUATED")
            }
            CgEnvelopeEvidence::Unknown => format!("{prefix} NO REGISTERED SOURCE"),
        },
    }
}
