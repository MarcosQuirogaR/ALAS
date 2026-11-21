// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Text rendering for physical-feasibility evidence.

use alas_config::CgEnvelopeEvidence;
use alas_opt::{AftCgLimitGovernance, ModelCgEnvelopeAssessment};

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
    lines.push(format_model_cg_assessment(
        report.model_cg.as_ref(),
        report.design_model_cg.as_ref(),
    ));
    if let Some(line) = format_forward_limit(report.model_cg.as_ref()) {
        lines.push(line);
    }
    if let Some(line) = format_aft_limit_governance(report.model_cg.as_ref()) {
        lines.push(line);
    }
    lines.push(format_cg_assessment(&report.cg_envelope));
    lines.push(cruise_equilibrium::format(
        report.cruise_equilibrium.as_ref(),
    ));
    lines.join("\n")
}

/// The status line counts exactly what the feasibility verdict gates on: the
/// hard constraints of the flown-loading assessment and, when it is a
/// separate assessment, of the maximum-fuel design loading. The minimum
/// usable CG range is a diagnostic in both the search and the verdict
/// (`ModelCgConstraint::is_diagnostic`), so it is counted on its own and
/// never reported as a hard failure.
fn format_model_cg_assessment(
    assessment: Option<&ModelCgEnvelopeAssessment>,
    design: Option<&ModelCgEnvelopeAssessment>,
) -> String {
    let Some(assessment) = assessment else {
        return "Model CG status     : NOT EVALUATED".to_owned();
    };
    let gated = std::iter::once(assessment)
        .chain(design.filter(|design| *design != assessment))
        .flat_map(|assessment| &assessment.loading_states)
        .flat_map(|state| &state.constraints)
        .filter(|constraint| constraint.violated);
    let (diagnostic, hard): (Vec<&alas_opt::ModelCgConstraintAssessment>, Vec<_>) =
        gated.partition(|constraint| constraint.is_diagnostic());
    let mut status = if hard.is_empty() {
        "HARD CONSTRAINTS PASS".to_owned()
    } else {
        format!("HARD CONSTRAINTS FAIL ({})", hard.len())
    };
    if !diagnostic.is_empty() {
        status.push_str(&format!(
            " ({} diagnostic limit(s) not met)",
            diagnostic.len()
        ));
    }
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

/// The forward CG boundary with its landing law and, beside it, Torenbeek's
/// design rule (eq. 6.2 at `C_L,max,land`, `C_L,h = -0.80`), which is
/// reported and never a limit. `None` without a finite forward boundary.
fn format_forward_limit(assessment: Option<&ModelCgEnvelopeAssessment>) -> Option<String> {
    let assessment = assessment?;
    if !assessment.configured_forward_limit_pct_mac.is_finite() {
        return None;
    }
    let value = |pct: f64| {
        if pct.is_finite() {
            format!("{pct:.1}% MAC")
        } else {
            "n/a".to_owned()
        }
    };
    Some(format!(
        "Forward CG boundary : {}; landing trim {} (approach at 1.23 V_SR0, stabiliser          stop), maximum nose load {}; Torenbeek eq. 6.2 design rule {} (reported, not a limit)",
        value(assessment.configured_forward_limit_pct_mac),
        value(assessment.landing_trim_fwd_limit_pct_mac),
        value(assessment.max_nose_load_fwd_limit_pct_mac),
        value(assessment.scissor_plot_fwd_limit_pct_mac),
    ))
}

/// Name the governing aft boundary when it is not the one the envelope
/// reports.
///
/// Printed only when the two disagree, because on a layout where the
/// aerodynamic boundary governs there is nothing to say and a line saying so
/// would dilute the one case that matters. The wording states which boundary
/// is which and the band between them; it moves no limit.
fn format_aft_limit_governance(assessment: Option<&ModelCgEnvelopeAssessment>) -> Option<String> {
    let assessment = assessment?;
    let AftCgLimitGovernance::GroundMinimumNoseLoad { margin_pct_mac } =
        assessment.aft_limit_governance
    else {
        return None;
    };
    let consequence = if assessment.any_ground_reaction_inadmissible() {
        "an analyzed loading state already sits back on its tail"
    } else {
        "no analyzed loading state has left the two-point compression domain"
    };
    Some(format!(
        "Aft CG boundary     : NOT GOVERNING; aerodynamic {:.1}% MAC against ground \
         minimum-nose-load {:.1}% MAC ({:.1}% MAC further forward), effective main gear {:.1}% \
         MAC; {consequence}",
        assessment.aerodynamic_aft_limit_pct_mac,
        assessment.ground_aft_limit_pct_mac,
        margin_pct_mac,
        assessment.main_gear_station_pct_mac,
    ))
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
