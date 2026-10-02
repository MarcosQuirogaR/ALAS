// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Shared native structural feasibility and final finite-element vetoes.

use super::{error, warning, FindingCode, PhysicalFinding};
use crate::{full_analysis::AnalysisReport, structural::StructuralAnalysisResult};
use alas_config::{AlasConfig, DesignVector};
use alas_struct::nastran::ResultStatus;

mod spanwise;

/// Keep the GUI's branch-specific optimizer record consistent with delivery.
pub(crate) fn sync_selected_delivery(
    solutions: Option<&mut crate::dual_solver::SolverOptimizationSet>,
    mode: crate::solver_mode::OptimizationSolverMode,
    delivered: Option<&alas_opt::OptimizationResult>,
) {
    let (Some(solutions), Some(delivered)) = (solutions, delivered) else {
        return;
    };
    let Ok(selected) = solutions.selected(mode) else {
        return;
    };
    let branch = match selected.solver {
        crate::solver_mode::SolverKind::Vlm => &mut solutions.vlm,
        crate::solver_mode::SolverKind::Avl => &mut solutions.avl,
    };
    branch.optimization = Some(delivered.clone());
    if branch.status.has_design() {
        branch.status = crate::dual_solver::SolverOptimizationStatus::for_delivered(delivered);
    }
}

/// Structural design authority shared by reporting and its final native check.
///
/// The structural loads read the mass the report was evaluated at
/// ([`AnalysisReport::analysis_takeoff_mass_kg`]: the mission-sized takeoff mass of a
/// sized finalist, otherwise the declared requirement), never the declared
/// MTOW limit of a lighter sized design. `AlasConfig::at_closure_mass` keeps a
/// registered aircraft's declared design gross and landing masses, so only a
/// coupled (clean-sheet) basis follows the closure mass; this is the same
/// binding the sized report itself is built with.
pub(crate) fn design_config(config: &AlasConfig, report: &AnalysisReport) -> AlasConfig {
    super::design_mass_config(config, report)
}

/// Revoke a delivered candidate after downstream checks; return its diagnostic.
pub(crate) fn revoke_delivery(
    findings: &[PhysicalFinding],
    optimization: Option<&mut alas_opt::OptimizationResult>,
) -> Option<String> {
    let errors: Vec<_> = findings
        .iter()
        .filter(|finding| finding.severity == super::FindingSeverity::Error)
        .collect();
    if errors.is_empty() {
        return None;
    }
    let optimization = optimization?;
    let mut acceptance =
        optimization
            .delivered_acceptance
            .clone()
            .unwrap_or(alas_opt::DeliveredAcceptance {
                verified: false,
                finalist_rejected_by: Vec::new(),
                delivered_rejected_by: Vec::new(),
                rejection_messages: Vec::new(),
                candidates_evaluated: 1,
                delivered_is_search_finalist: true,
                wall_time_s: 0.0,
                analyses: 0,
                baseline: None,
            });
    acceptance.verified = false;
    acceptance.delivered_rejected_by = errors.iter().map(|f| f.code.as_str().to_owned()).collect();
    if acceptance.delivered_is_search_finalist {
        acceptance.finalist_rejected_by = acceptance.delivered_rejected_by.clone();
    }
    acceptance.rejection_messages = errors.iter().map(|f| f.message.clone()).collect();
    optimization.record_delivered_acceptance(acceptance);
    Some(format!(
        "Delivered candidate rejected after downstream analysis: {}",
        errors
            .iter()
            .map(|f| f.code.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

pub(super) fn append_native(
    config: &AlasConfig,
    design: &DesignVector,
    report: &AnalysisReport,
    findings: &mut Vec<PhysicalFinding>,
) {
    let structural_config = design_config(config, report);
    let assessment = alas_opt::mdo::structural_feasibility::assess_candidate(
        &structural_config,
        design,
        &report.airplane,
    );
    let assessment = match assessment {
        Ok(value) if value.input_valid => value,
        result => {
            findings.push(error(
                FindingCode::StructuralResponseUnavailable,
                format!(
                    "mandatory structural assessment unavailable: {}",
                    result
                        .err()
                        .unwrap_or("invalid response arrays or model inputs")
                ),
                None,
                None,
                "",
            ));
            return;
        }
    };
    let strength_limit = 1.0 + alas_struct::sizing::MARGIN_NUMERICAL_ZERO;
    let wing_mass_kg = report
        .component_masses
        .get(alas_mass::breakdown::WING)
        .copied();
    append_mass_comparisons(
        Some(assessment.primary_mass_kg),
        assessment.mesh_primary_mass_kg,
        wing_mass_kg,
        findings,
    );
    for (actual, limit, label) in [
        (
            assessment.max_strength_utilization,
            strength_limit,
            "strength utilization",
        ),
        (assessment.rib_spacing_ratio, 1.0, "rib spacing ratio"),
        (
            assessment.cap_packaging_ratio,
            strength_limit,
            "spar cap packaging ratio",
        ),
    ] {
        if actual > limit {
            findings.push(error(
                FindingCode::StructuralStrengthViolation,
                format!("wing {label} exceeds the sized load requirement"),
                Some(actual),
                Some(limit),
                "ratio",
            ));
        }
    }
    if assessment.max_linear_curvature_relative_error
        > assessment.limits.max_curvature_relative_error
    {
        findings.push(error(FindingCode::StructuralLinearModelDomain,
            format!("{} (n={:.3}) exceeds the linear beam curvature error budget; a nonlinear structural assessment is required", assessment.governing_load_case, assessment.governing_load_factor),
            Some(assessment.max_linear_curvature_relative_error), Some(assessment.limits.max_curvature_relative_error), "fraction"));
    }
    if assessment.manoeuvre_curvature_relative_error
        > assessment.limits.max_curvature_relative_error
    {
        findings.push(warning(FindingCode::StructuralLinearModelDomain,
            "ultimate-load wing deflection exceeds the linear beam curvature error budget; the reported ultimate deflection is a linear estimate, the ultimate stresses are unaffected",
            Some(assessment.manoeuvre_curvature_relative_error), Some(assessment.limits.max_curvature_relative_error), "fraction"));
    }
}

/// Whether an FE case outside the linear curvature budget vetoes the result.
///
/// Only the 1 g flight shape does, for the reason
/// `alas_struct::feasibility` states: an ultimate case outside the budget has
/// a linear deflection estimate but unaffected stresses, and is a warning.
fn curvature_finding(
    case: &str,
    code: FindingCode,
    message: String,
    actual: f64,
    limit: f64,
) -> PhysicalFinding {
    if case == alas_struct::feasibility::FLIGHT_SHAPE_CASE {
        error(code, message, Some(actual), Some(limit), "fraction")
    } else {
        warning(code, message, Some(actual), Some(limit), "fraction")
    }
}

fn append_mass_comparisons(
    native_kg: Option<f64>,
    mesh_kg: Option<f64>,
    empirical_kg: Option<f64>,
    findings: &mut Vec<PhysicalFinding>,
) {
    let empirical_valid = empirical_kg.is_some_and(|mass| mass.is_finite() && mass > 0.0);
    if !empirical_valid {
        findings.push(error(
            FindingCode::MassLedgerUnavailable,
            "empirical wing mass ledger value is unavailable or invalid",
            empirical_kg,
            None,
            "kg",
        ));
    }
    for (label, mass) in [
        ("native primary structure", native_kg),
        ("FE primary material", mesh_kg),
    ] {
        match mass.filter(|v| v.is_finite() && *v > 0.0) {
            None => findings.push(error(
                FindingCode::StructuralResponseUnavailable,
                format!("{label} mass is unavailable or invalid"),
                mass,
                None,
                "kg",
            )),
            Some(actual) if empirical_valid => {
                let empirical = empirical_kg.unwrap_or_default();
                if actual != empirical {
                    findings.push(warning(FindingCode::StructuralMassModelDifference,
                        format!("{label} {actual:.3} kg differs from the empirical FLOPS complete-wing estimate {empirical:.3} kg by {:+.3} kg. These inventories have different model scopes; the discrepancy is diagnostic only, not a physical limit or objective penalty.",actual-empirical),
                        Some(actual),Some(empirical),"kg"));
                }
            }
            _ => {}
        }
    }
}

/// A completed downstream solve can veto an earlier native-model acceptance.
/// Sampled displacement secants bound maximum spanwise slope from below;
/// passing this necessary check does not validate the full FE rotation field.
pub(crate) fn append_downstream(
    config: &AlasConfig,
    result: Option<&StructuralAnalysisResult>,
    findings: &mut Vec<PhysicalFinding>,
) {
    let Some(result) = result else {
        return;
    };
    if result.status == "error" {
        findings.push(error(
            FindingCode::StructuralResponseUnavailable,
            result
                .error
                .clone()
                .unwrap_or_else(|| "downstream structural analysis failed".to_owned()),
            None,
            None,
            "",
        ));
    }
    let semispan = result.wsg.as_ref().map(|geometry| geometry.semi_span);
    for (solver, solution) in [
        ("MSC Nastran", result.nastran.as_ref()),
        ("NASTRAN-95", result.nastran95.as_ref()),
    ] {
        let Some(solution) = solution else {
            continue;
        };
        let response = &solution.static_solve;
        if response.status == ResultStatus::NotRun {
            continue;
        }
        // An optional solver that was never configured produced no finite
        // element result to veto with; the native structural assessment
        // remains the authority and this is reported, not failed. A
        // configured solver that is missing or fails still vetoes below.
        if response.status == ResultStatus::Error
            && response
                .error
                .as_deref()
                .is_some_and(alas_struct::nastran::is_solver_not_configured)
        {
            findings.push(warning(
                FindingCode::StructuralSolverFailed,
                format!(
                    "{solver} static verification was not run: {}",
                    response.error.as_deref().unwrap_or_default()
                ),
                None,
                None,
                "",
            ));
            continue;
        }
        let complete_cases = response.tip_deflection_m.len() == 3
            && ["pull-up", "push-down", "level"].iter().all(|expected| {
                response
                    .tip_deflection_m
                    .iter()
                    .filter(|(case, _)| case == expected)
                    .count()
                    == 1
            });
        let complete_stress = response.root_von_mises_max_pa.len() == 3
            && ["pull-up", "push-down", "level"].iter().all(|expected| {
                response
                    .root_von_mises_max_pa
                    .iter()
                    .filter(|(case, _)| case == expected)
                    .count()
                    == 1
            });
        let invalid_stress = response
            .root_von_mises_max_pa
            .iter()
            .any(|(_, stress)| !stress.is_finite() || stress < 0.0);
        if response.status == ResultStatus::Error
            || !complete_cases
            || !complete_stress
            || invalid_stress
        {
            findings.push(error(
                FindingCode::StructuralSolverFailed,
                format!(
                    "{solver} static verification failed or is incomplete: {}",
                    response
                        .error
                        .as_deref()
                        .unwrap_or("missing displacement/stress load cases")
                ),
                None,
                None,
                "",
            ));
            continue;
        }
        // A mixed-material root maximum has no element/material attribution.
        // Exceeding even the strongest declared allowable is a necessary veto;
        // passing it does not establish material-specific FE strength margins.
        match strongest_declared_allowable(config) {
            Some(allowable) => {
                for (case, stress) in response.root_von_mises_max_pa.iter() {
                    if stress > allowable {
                        findings.push(error(FindingCode::StructuralStrengthViolation,
                            format!("{solver} {case} root stress exceeds every declared material allowable; element-level stress and mesh review required"),
                            Some(stress), Some(allowable), "Pa"));
                    }
                }
            }
            None => findings.push(error(
                FindingCode::StructuralResponseUnavailable,
                "FE stress comparison has no finite declared material allowable",
                None,
                None,
                "Pa",
            )),
        }
        spanwise::append(config, solver, response, findings);
        for (case, tip) in response.tip_deflection_m.iter() {
            match fem_curvature_lower_bound(tip, semispan) {
                Some(error_bound) if error_bound <= config.structures.max_linear_curvature_relative_error => {},
                Some(error_bound) => findings.push(curvature_finding(case, FindingCode::StructuralFemModelDomain,
                    format!("{solver} {case} tip displacement proves the linear model exceeds its curvature error budget"),
                    error_bound, config.structures.max_linear_curvature_relative_error)),
                None => findings.push(error(FindingCode::StructuralSolverFailed,
                    format!("{solver} {case} returned an invalid displacement or reference semispan"), Some(tip), semispan, "m")),
            }
        }
    }
}

fn strongest_declared_allowable(config: &AlasConfig) -> Option<f64> {
    let cfg = &config.structures;
    if !cfg.additional_safety_factor.is_finite() || cfg.additional_safety_factor < 1.0 {
        return None;
    }
    let mut strongest = 0.0_f64;
    for name in [
        &cfg.skin_material,
        &cfg.spar_web_material,
        &cfg.spar_cap_material,
        &cfg.rib_material,
    ] {
        let allowable =
            alas_config::materials::get(name).ok()?.f_allow_pa / cfg.additional_safety_factor;
        if !allowable.is_finite() || allowable <= 0.0 {
            return None;
        }
        strongest = strongest.max(allowable);
    }
    Some(strongest)
}

fn fem_curvature_lower_bound(tip_m: f64, semispan_m: Option<f64>) -> Option<f64> {
    let span = semispan_m.filter(|span| span.is_finite() && *span > 0.0)?;
    if !tip_m.is_finite() {
        return None;
    }
    let value = (1.0 + (tip_m / span).powi(2)).powf(1.5) - 1.0;
    value.is_finite().then_some(value)
}

// Tests assert on values they construct directly, so a failed unwrap is the
// assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests;
