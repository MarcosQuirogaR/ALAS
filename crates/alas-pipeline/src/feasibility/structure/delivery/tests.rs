// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Constructed fixture values are test assertions, so failed unwraps fail the test.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::feasibility::{FindingSeverity, PhysicalFinding};
use alas_struct::nastran::{
    NastranResults, StaticCaseIdentity, StaticSpanwiseCase, StaticSpanwiseResponse,
};

fn native_error() -> PhysicalFinding {
    error(
        FindingCode::StructuralResponseUnavailable,
        "native-only failure",
        None,
        None,
        "",
    )
}

fn fe_solution(semispan: f64) -> NastranResults {
    let mut result = NastranResults::default();
    result.static_solve.status = ResultStatus::Ok;
    let mut curves = Vec::new();
    for (subcase_id, name, load_factor) in [
        (1, "pull-up", 3.75),
        (2, "push-down", -1.5),
        (3, "level", 1.0),
    ] {
        result.static_solve.tip_deflection_m.push(name, 0.01);
        result.static_solve.root_von_mises_max_pa.push(name, 1.0);
        curves.push(StaticSpanwiseCase {
            subcase_id,
            name,
            load_factor,
            grid_ids: vec![1, 2],
            xyz_m: vec![[0.0; 3], [0.0, semispan, 0.0]],
            y_m: vec![0.0, semispan],
            translations_m: vec![[0.0; 3], [0.0, 0.0, 0.01]],
            rotations_rad: vec![[0.0; 3]; 2],
        });
    }
    result.static_solve.spanwise = Some(StaticSpanwiseResponse {
        cases: curves,
        error: None,
        case_identity: StaticCaseIdentity::ExplicitSubcaseIds,
    });
    result
}

#[test]
fn current_solved_fe_evidence_replaces_the_native_delivery_assessment() {
    let (config, design, report, mut result) =
        crate::feasibility::structure::tests::structural_fixture();
    result.nastran = Some(fe_solution(result.wsg.as_ref().unwrap().semi_span));
    let mut findings = vec![native_error()];
    append(&config, &design, &report, Some(&result), &mut findings);
    assert!(!findings
        .iter()
        .any(|finding| finding.message == "native-only failure"));
    let fe = result.wing_mass.unwrap().fe_primary_kg.unwrap();
    assert!(findings.iter().any(|finding| {
        finding.code == FindingCode::StructuralMassModelDifference
            && finding.message.starts_with("FE primary material")
            && finding.actual == Some(fe)
    }));
}

#[test]
fn deck_inventory_without_a_solve_cannot_erase_native_failures() {
    let (config, design, report, result) =
        crate::feasibility::structure::tests::structural_fixture();
    assert!(result.nastran.is_none() && result.nastran95.is_none());
    assert!(result.wing_mass.as_ref().unwrap().fe_primary_kg.is_some());
    let native_findings = vec![
        native_error(),
        error(
            FindingCode::StructuralStrengthViolation,
            "native strength failure",
            Some(2.0),
            Some(1.0),
            "ratio",
        ),
        error(
            FindingCode::StructuralLinearModelDomain,
            "native domain failure",
            Some(0.2),
            Some(0.05),
            "fraction",
        ),
    ];
    let mut findings = native_findings.clone();
    append(&config, &design, &report, Some(&result), &mut findings);
    for finding in &native_findings {
        assert!(
            findings.contains(finding),
            "deck-only result erased {finding:?}"
        );
    }
}

#[test]
fn stale_geometry_loads_or_materials_cannot_replace_current_native_evidence() {
    let (config, design, report, mut result) =
        crate::feasibility::structure::tests::structural_fixture();
    result.nastran = Some(fe_solution(result.wsg.as_ref().unwrap().semi_span));
    for change in 0..4 {
        let mut config = config.clone();
        let mut design = design;
        let mut report = report.clone();
        match change {
            0 => {
                design.span_m *= 1.1;
                report.design = design;
            }
            1 => config.requirements.mtow_kg *= 1.1,
            2 => config.structures.skin_material = "CFRP QI".to_owned(),
            _ => report.airplane.wings[0].xsecs[0].chord *= 1.1,
        }
        let mut findings = vec![native_error()];
        append(&config, &design, &report, Some(&result), &mut findings);
        assert!(findings
            .iter()
            .any(|finding| finding.message == "native-only failure"));
        assert!(findings
            .iter()
            .any(|finding| finding.severity == FindingSeverity::Warning
                && finding.message.contains("provenance")));
    }
}

#[test]
fn absent_or_unidentified_fe_results_preserve_native_fallback() {
    let (config, design, report, mut result) =
        crate::feasibility::structure::tests::structural_fixture();
    let original = vec![native_error()];
    let mut findings = original.clone();
    append(&config, &design, &report, None, &mut findings);
    assert_eq!(findings, original);
    result.evaluation_inputs = None;
    append(&config, &design, &report, Some(&result), &mut findings);
    assert_eq!(findings[0], original[0]);
}

#[test]
fn complete_sampled_fe_shape_keeps_native_domain_and_unsolved_cap_strength() {
    let (config, design, report, mut result) =
        crate::feasibility::structure::tests::structural_fixture();
    let semispan = result.wsg.as_ref().unwrap().semi_span;
    let response = result.analysis.as_mut().unwrap();
    let level = response
        .load_cases
        .iter_mut()
        .find(|case| case.name == "level")
        .unwrap();
    level.tip_deflection_m = semispan;
    level.deflection_m.clone_from(&level.y);
    let mut findings = Vec::new();
    append(&config, &design, &report, Some(&result), &mut findings);
    assert!(findings.iter().any(|finding| finding.code
        == FindingCode::StructuralLinearModelDomain
        && finding.severity == FindingSeverity::Error));
    result.nastran = Some(fe_solution(semispan));
    findings.clear();
    append(&config, &design, &report, Some(&result), &mut findings);
    assert!(findings.iter().any(|finding| finding.code
        == FindingCode::StructuralLinearModelDomain
        && finding.severity == FindingSeverity::Error));
    result.sizing.as_mut().unwrap().spars[0].margin_of_safety[0] = -0.1;
    findings.clear();
    append(&config, &design, &report, Some(&result), &mut findings);
    assert!(findings
        .iter()
        .any(|finding| finding.code == FindingCode::StructuralStrengthViolation));
}

#[test]
fn small_fe_secants_and_large_rotations_cannot_erase_native_domain_error() {
    let (config, design, report, mut result) =
        crate::feasibility::structure::tests::structural_fixture();
    let mut solution = fe_solution(result.wsg.as_ref().unwrap().semi_span);
    let level = solution
        .static_solve
        .spanwise
        .as_mut()
        .unwrap()
        .cases
        .iter_mut()
        .find(|case| case.name == "level")
        .unwrap();
    level.rotations_rad[1] = [0.5, 0.0, 0.0];
    result.nastran = Some(solution);
    let native = error(
        FindingCode::StructuralLinearModelDomain,
        "native domain failure",
        Some(0.2),
        Some(0.05),
        "fraction",
    );
    let mut findings = vec![native.clone()];
    append(&config, &design, &report, Some(&result), &mut findings);
    assert!(findings.contains(&native));
    assert!(findings.iter().any(|finding| {
        finding.code == FindingCode::StructuralFemModelDomain
            && finding.severity == FindingSeverity::Error
            && finding.actual.is_some_and(|value| value > 0.05)
            && finding.message.contains("bending rotation")
    }));
}

#[test]
fn incomplete_fe_shape_cannot_suppress_the_beam_domain_gate() {
    let (config, design, report, mut result) =
        crate::feasibility::structure::tests::structural_fixture();
    let response = result.analysis.as_mut().unwrap();
    let level = response
        .load_cases
        .iter_mut()
        .find(|case| case.name == "level")
        .unwrap();
    level.tip_deflection_m = *level.y.last().unwrap();
    level.deflection_m.clone_from(&level.y);
    let mut solution = fe_solution(design.span_m / 2.0);
    solution.static_solve.spanwise = None;
    result.nastran = Some(solution);
    let mut findings = Vec::new();
    append(&config, &design, &report, Some(&result), &mut findings);
    assert!(findings.iter().any(|finding| finding.code
        == FindingCode::StructuralLinearModelDomain
        && finding.severity == FindingSeverity::Error));
    assert!(findings
        .iter()
        .any(|finding| finding.code == FindingCode::StructuralSolverFailed));
}

#[test]
fn incomplete_solved_stress_cannot_erase_native_failures() {
    let (config, design, report, mut result) =
        crate::feasibility::structure::tests::structural_fixture();
    let mut solution = fe_solution(design.span_m / 2.0);
    solution.static_solve.root_von_mises_max_pa = Default::default();
    result.nastran = Some(solution);
    let original = native_error();
    let mut findings = vec![original.clone()];
    append(&config, &design, &report, Some(&result), &mut findings);
    assert!(findings.contains(&original));
    assert!(findings
        .iter()
        .any(|finding| finding.code == FindingCode::StructuralSolverFailed));
}

#[test]
fn invalid_current_fe_inventory_fails_closed() {
    let (config, design, report, mut result) =
        crate::feasibility::structure::tests::structural_fixture();
    result.wing_mass.as_mut().unwrap().fe_primary_kg = Some(f64::NAN);
    let mut findings = Vec::new();
    append(&config, &design, &report, Some(&result), &mut findings);
    assert!(findings.iter().any(|finding| finding.code
        == FindingCode::StructuralResponseUnavailable
        && finding.severity == FindingSeverity::Error));
}
