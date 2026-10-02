// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

#[test]
fn empirical_mass_discrepancies_are_quantitative_warnings_and_do_not_revoke_delivery() {
    let mut findings = Vec::new();
    append_mass_comparisons(Some(30000.0), Some(36000.0), Some(20000.0), &mut findings);
    assert_eq!(findings.len(), 2);
    assert!(findings
        .iter()
        .all(|f| f.code == FindingCode::StructuralMassModelDifference
            && f.severity == crate::feasibility::FindingSeverity::Warning));
    assert_eq!(findings[0].actual, Some(30000.0));
    assert_eq!(findings[1].actual, Some(36000.0));
    assert!(findings.iter().all(|f| f.limit == Some(20000.0)));
    let mut delivered = valid_optimization();
    let before = delivered.clone();
    assert!(revoke_delivery(&findings, Some(&mut delivered)).is_none());
    assert_eq!(delivered, before);
}

#[test]
fn invalid_material_inventory_or_ledger_still_fails() {
    for invalid in [
        None,
        Some(f64::NAN),
        Some(f64::INFINITY),
        Some(0.0),
        Some(-1.0),
    ] {
        let mut findings = Vec::new();
        append_mass_comparisons(invalid, Some(20.0), Some(10.0), &mut findings);
        assert!(findings
            .iter()
            .any(|f| f.code == FindingCode::StructuralResponseUnavailable
                && f.severity == crate::feasibility::FindingSeverity::Error));
        findings.clear();
        append_mass_comparisons(Some(20.0), invalid, Some(10.0), &mut findings);
        assert!(findings
            .iter()
            .any(|f| f.code == FindingCode::StructuralResponseUnavailable));
        findings.clear();
        append_mass_comparisons(Some(20.0), Some(30.0), invalid, &mut findings);
        assert!(findings
            .iter()
            .any(|f| f.code == FindingCode::MassLedgerUnavailable));
    }
}

fn valid_optimization() -> alas_opt::OptimizationResult {
    alas_opt::OptimizationResult {
        best_design: DesignVector::default(),
        best_cost: 1.0,
        best_valid: true,
        history: Default::default(),
        wall_time_s: 0.0,
        method: "differential_evolution".to_owned(),
        strategy: "lshade_eps_de".to_owned(),
        termination: "converged".to_owned(),
        pareto_front: Vec::new(),
        search_diagnostics: None,
        delivered_acceptance: None,
    }
}

#[test]
fn downstream_failure_revokes_a_previously_valid_search_result() {
    let mut result = valid_optimization();
    let findings = vec![error(
        FindingCode::StructuralSolverFailed,
        "static verification incomplete",
        None,
        None,
        "",
    )];
    let branch = |solver| crate::dual_solver::SolverOptimizationResult {
        solver,
        status: crate::dual_solver::SolverOptimizationStatus::Completed,
        design: Some(result.best_design),
        optimization: Some(result.clone()),
        report: None,
        avl_result: None,
        output_dir: None,
        error: None,
    };
    let mut branches = crate::dual_solver::SolverOptimizationSet {
        vlm: branch(crate::solver_mode::SolverKind::Vlm),
        avl: branch(crate::solver_mode::SolverKind::Avl),
    };
    let message = revoke_delivery(&findings, Some(&mut result)).unwrap();
    assert!(!result.best_valid);
    assert_ne!(result.termination, "converged");
    sync_selected_delivery(
        Some(&mut branches),
        crate::solver_mode::OptimizationSolverMode::Both,
        Some(&result),
    );
    assert_eq!(branches.vlm.optimization.as_ref(), Some(&result));
    // Only the selected VLM aircraft received this downstream analysis.
    assert!(branches.avl.optimization.as_ref().unwrap().best_valid);
    let acceptance = result.delivered_acceptance.unwrap();
    assert!(!acceptance.verified);
    assert_eq!(
        acceptance.delivered_rejected_by,
        acceptance.finalist_rejected_by
    );
    assert!(message.contains(FindingCode::StructuralSolverFailed.as_str()));
}

fn solved_static() -> StructuralAnalysisResult {
    let mut nastran = alas_struct::nastran::NastranResults::default();
    nastran.static_solve.status = ResultStatus::Ok;
    for case in ["pull-up", "push-down", "level"] {
        nastran.static_solve.tip_deflection_m.push(case, 0.01);
        nastran.static_solve.root_von_mises_max_pa.push(case, 1.0);
    }
    StructuralAnalysisResult {
        status: "ok".to_owned(),
        error: None,
        wsg: None,
        sizing: None,
        mesh_health: None,
        analysis: None,
        nastran: Some(nastran),
        nastran95: None,
        patran: None,
        torenbeek_wing_mass_kg: 0.0,
    }
}

#[test]
fn nominal_solver_success_cannot_hide_missing_cases_or_invalid_stress() {
    for duplicate in [false, true] {
        let mut result = solved_static();
        let response = &mut result.nastran.as_mut().unwrap().static_solve;
        if duplicate {
            response.tip_deflection_m = Default::default();
            for _ in 0..3 {
                response.tip_deflection_m.push("level", 0.01);
            }
        } else {
            response.root_von_mises_max_pa.push("pull-up", f64::NAN);
        }
        let mut findings = Vec::new();
        append_downstream(&AlasConfig::default(), Some(&result), &mut findings);
        assert!(findings
            .iter()
            .any(|f| f.code == FindingCode::StructuralSolverFailed));
    }
}

#[test]
fn failed_mesh_is_an_explicit_final_delivery_failure() {
    let mut result = solved_static();
    result.status = "error".to_owned();
    result.error = Some("mesh generation failed".to_owned());
    result.nastran = None;
    let mut findings = Vec::new();
    append_downstream(&AlasConfig::default(), Some(&result), &mut findings);
    assert!(findings
        .iter()
        .any(|f| f.code == FindingCode::StructuralResponseUnavailable));
}

#[test]
fn solved_displacements_do_not_replace_missing_stress_verification() {
    let mut result = solved_static();
    result
        .nastran
        .as_mut()
        .unwrap()
        .static_solve
        .root_von_mises_max_pa = Default::default();
    let mut findings = Vec::new();
    append_downstream(&AlasConfig::default(), Some(&result), &mut findings);
    assert!(findings
        .iter()
        .any(|f| f.code == FindingCode::StructuralSolverFailed));
}

#[test]
fn finite_extreme_stress_is_not_accepted_as_available_evidence() {
    let mut result = solved_static();
    let response = &mut result.nastran.as_mut().unwrap().static_solve;
    response.root_von_mises_max_pa = Default::default();
    for case in ["pull-up", "push-down", "level"] {
        response.root_von_mises_max_pa.push(case, 1.0e12);
    }
    let mut findings = Vec::new();
    append_downstream(&AlasConfig::default(), Some(&result), &mut findings);
    assert!(findings
        .iter()
        .any(|f| f.code == FindingCode::StructuralStrengthViolation));
}

#[test]
fn extreme_tip_response_cannot_pass_as_a_linear_solution() {
    assert!(fem_curvature_lower_bound(20.0, Some(33.0)).unwrap() > 0.05);
    assert_eq!(
        fem_curvature_lower_bound(20.0, Some(33.0)),
        fem_curvature_lower_bound(-20.0, Some(33.0))
    );
    assert!(fem_curvature_lower_bound(0.1, Some(33.0)).unwrap() < 0.05);
    assert!(fem_curvature_lower_bound(f64::NAN, Some(33.0)).is_none());
    assert!(fem_curvature_lower_bound(1.0, Some(0.0)).is_none());
}

/// A clean-sheet design closed below its declared MTOW limit: the structural
/// stage must load the wing at the closure mass the report was evaluated at,
/// exactly as the sized report binds it, and not at the configured limit.
#[test]
fn structural_loads_use_the_sized_takeoff_mass_not_the_configured_limit() {
    let config = AlasConfig::default();
    let design = DesignVector::default();
    let limit_kg = config.requirements.mtow_kg;
    let sized_kg = 0.8 * limit_kg;
    let report = crate::FullAnalysis::new(config.clone())
        .run_at_sized_takeoff_mass(&design, false, sized_kg)
        .expect("the default design closes at a lighter takeoff mass");

    assert_eq!(
        report.analysis_takeoff_mass_kg(config.requirements.mtow_kg),
        sized_kg
    );
    let structural = design_config(&config, &report);
    assert_eq!(structural.requirements.mtow_kg, sized_kg);
    assert_eq!(
        alas_opt::mdo::structural_feasibility::structural_design_mass_kg(&structural),
        sized_kg,
        "a clean-sheet basis couples its design gross to the closure mass"
    );
    assert!(sized_kg < limit_kg);

    // The consequence: the wing box sized for the lighter design is lighter.
    let at_sized = alas_opt::mdo::structural_feasibility::assess_candidate(
        &structural,
        &design,
        &report.airplane,
    )
    .expect("sized-mass assessment");
    let at_limit =
        alas_opt::mdo::structural_feasibility::assess_candidate(&config, &design, &report.airplane)
            .expect("limit-mass assessment");
    assert!(
        at_sized.primary_mass_kg < at_limit.primary_mass_kg,
        "primary structure {} kg at {sized_kg} kg against {} kg at {limit_kg} kg",
        at_sized.primary_mass_kg,
        at_limit.primary_mass_kg
    );
}

/// A registered aircraft keeps its declared design gross weight while its
/// closure mass moves, so a light dispatch never resizes its box.
#[test]
fn a_registered_aircraft_keeps_its_declared_design_weight_under_a_sized_report() {
    let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" })).unwrap();
    config.optimizer.design_space.mode = alas_config::DesignMode::BaselineSandbox;
    let design = alas_config::presets::get("A320-200").unwrap().design_vector;
    let declared_kg = config.requirements.mtow_kg;
    let report = crate::FullAnalysis::new(config.clone())
        .run_at_sized_takeoff_mass(&design, false, 0.85 * declared_kg)
        .expect("the preset closes at a lighter takeoff mass");
    let structural = design_config(&config, &report);
    assert_eq!(structural.requirements.mtow_kg, 0.85 * declared_kg);
    assert_eq!(
        alas_opt::mdo::structural_feasibility::structural_design_mass_kg(&structural),
        declared_kg
    );
    // The V-n envelope is drawn at the same declared design weight the loads
    // use, not at the lighter closure mass.
    assert_eq!(
        super::super::design_vn_mass_kg(&config, &report),
        declared_kg
    );
}
