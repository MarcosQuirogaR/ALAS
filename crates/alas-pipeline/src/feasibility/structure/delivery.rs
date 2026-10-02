// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Final structural authority: current deck inventory and solved FE response.

use super::{
    append_assessment, append_downstream, design_config, error, warning, AlasConfig,
    AnalysisReport, DesignVector, FindingCode, PhysicalFinding, ResultStatus,
    StructuralAnalysisResult,
};
use alas_struct::feasibility::{assess, LinearModelLimits};

pub(crate) fn append(
    config: &AlasConfig,
    design: &DesignVector,
    report: &AnalysisReport,
    result: Option<&StructuralAnalysisResult>,
    findings: &mut Vec<PhysicalFinding>,
) {
    let Some(result) = result else {
        return;
    };
    let evaluated_config = design_config(config, report);
    if !result
        .evaluation_inputs
        .as_ref()
        .is_some_and(|inputs| inputs.matches(&evaluated_config, design, report))
    {
        findings.push(warning(
            FindingCode::StructuralResponseUnavailable,
            "downstream structural result has no matching evaluated-design provenance; native assessment remains authoritative",
            None, None, "",
        ));
        return;
    }
    if let Some(masses) = &result.wing_mass {
        if masses.fe_primary_kg.is_some() {
            replace_native(config, result, masses, findings);
        }
    }
    append_downstream(&evaluated_config, Some(result), findings);
}

fn replace_native(
    config: &AlasConfig,
    result: &StructuralAnalysisResult,
    masses: &crate::structural::WingMassComparison,
    findings: &mut Vec<PhysicalFinding>,
) {
    if !masses
        .fe_primary_kg
        .is_some_and(|mass| mass.is_finite() && mass > 0.0)
    {
        findings.push(error(
            FindingCode::StructuralResponseUnavailable,
            "current FE primary material mass is unavailable or invalid",
            masses.fe_primary_kg,
            None,
            "kg",
        ));
        return;
    }
    let (Some(sizing), Some(response)) = (&result.sizing, &result.analysis) else {
        findings.push(error(
            FindingCode::StructuralResponseUnavailable,
            "FE primary inventory lacks the sizing and response of its submitted wingbox",
            masses.fe_primary_kg,
            None,
            "kg",
        ));
        return;
    };
    let mut assessment = assess(
        sizing,
        response,
        LinearModelLimits {
            max_curvature_relative_error: config.structures.max_linear_curvature_relative_error,
        },
    );
    assessment.mesh_primary_mass_kg = masses.fe_primary_kg;
    if !assessment.input_valid {
        findings.push(error(
            FindingCode::StructuralResponseUnavailable,
            "FE primary inventory is associated with invalid sizing or structural response",
            None,
            None,
            "",
        ));
        return;
    }
    if result.status == "ok"
        && [result.nastran.as_ref(), result.nastran95.as_ref()]
            .into_iter()
            .flatten()
            .any(|solution| complete_fe_response(&solution.static_solve))
    {
        // A deck inventory is not a solved structural response. Only complete
        // current-design FE evidence can replace preceding native findings.
        // Other physical families and the ledger retain their authority.
        findings.retain(|finding| {
            !matches!(
                finding.code,
                FindingCode::StructuralStrengthViolation
                    | FindingCode::StructuralResponseUnavailable
                    | FindingCode::StructuralMassModelDifference
            )
        });
        // A complete sampled front-spar curve establishes response identity,
        // not a bound on unsampled bending rotations, gradients or nonlinear
        // curvature. Keep the native domain veto and its ultimate diagnostic
        // until a qualified whole-model deformation assessment is available.
    }
    // FE primary mass comes from shell/bar material actually submitted, not
    // engine/fuel CONM2 or an empirical complete-wing estimate. Beam sizing
    // retains cap strength/packaging and rib checks absent from FE summaries.
    append_assessment(&assessment, masses, findings);
}

fn complete_fe_response(response: &alas_struct::nastran::StaticResult) -> bool {
    response.status == ResultStatus::Ok
        && response.error.is_none()
        && response.tip_deflection_m.len() == 3
        && response.root_von_mises_max_pa.len() == 3
        && ["pull-up", "push-down", "level"].iter().all(|name| {
            response
                .tip_deflection_m
                .iter()
                .filter(|(case, tip)| case == name && tip.is_finite())
                .count()
                == 1
                && response
                    .root_von_mises_max_pa
                    .iter()
                    .filter(|(case, stress)| case == name && stress.is_finite() && *stress >= 0.0)
                    .count()
                    == 1
        })
        && super::spanwise::has_complete_response(response)
}

#[cfg(test)]
mod tests;
