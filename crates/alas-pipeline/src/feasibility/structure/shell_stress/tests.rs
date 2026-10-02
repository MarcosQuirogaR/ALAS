// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Constructed fixture values are test assertions, so failed unwraps fail the test.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use alas_struct::nastran::{StaticShellStressResponse, StaticShellStressSample};

fn deck_inputs(
    additional_safety_factor: f64,
) -> (
    super::super::AlasConfig,
    crate::structural::EvaluationInputs,
) {
    let (mut config, _, report, result) =
        crate::feasibility::structure::tests::structural_fixture();
    // Distinct declared shell materials exercise attribution, independently
    // of the baseline fixture's single-material sizing law.
    config.structures.spar_web_material = "CFRP QI".to_owned();
    config.structures.additional_safety_factor = additional_safety_factor;
    let get = |name: &str| alas_config::materials::get(name).unwrap();
    let cfg = &config.structures;
    let (deck, _, _) = alas_struct::mesh::build_wing_mesh_bdf_product(
        result.wsg.as_ref().unwrap(),
        result.sizing.as_ref().unwrap(),
        cfg,
        &config.geometry.engine,
        &config.mass_model,
        &config.requirements,
        get(&cfg.skin_material),
        get(&cfg.spar_web_material),
        get(&cfg.spar_cap_material),
        get(&cfg.rib_material),
    )
    .unwrap();
    let inputs = crate::structural::EvaluationInputs::from_deck(&config, &report, &deck);
    (config, inputs)
}

fn samples(inputs: &crate::structural::EvaluationInputs) -> Vec<StaticShellStressSample> {
    inputs
        .shell_identities()
        .iter()
        .flat_map(|&(element_id, property_id, material_id)| {
            let stress = 0.5 * inputs.material_allowable_pa(material_id).unwrap();
            [-0.001, 0.001].map(|fiber_distance_m| StaticShellStressSample {
                element_id,
                property_id,
                material_id,
                fiber_distance_m,
                normal_x_pa: stress,
                normal_y_pa: 0.0,
                shear_xy_pa: 0.0,
                von_mises_pa: stress,
            })
        })
        .collect()
}

#[test]
fn attributed_stress_uses_its_material_not_the_strongest_declared_allowable() {
    let (_, inputs) = deck_inputs(1.0);
    let mut case = StaticShellStressCase {
        subcase_id: 1,
        name: "pull-up",
        samples: samples(&inputs),
    };
    let strongest = inputs
        .shell_identities()
        .iter()
        .filter_map(|&(_, _, material)| inputs.material_allowable_pa(material))
        .fold(0.0_f64, f64::max);
    let weakest = inputs
        .shell_identities()
        .iter()
        .filter_map(|&(_, _, material)| inputs.material_allowable_pa(material))
        .fold(f64::INFINITY, f64::min);
    assert!(
        strongest > weakest,
        "fixture has distinct cover/web/rib material allowables"
    );
    let sample = case
        .samples
        .iter_mut()
        .find(|sample| inputs.material_allowable_pa(sample.material_id) == Some(weakest))
        .unwrap();
    let stress = 0.5 * (weakest + strongest);
    sample.normal_x_pa = stress;
    sample.von_mises_pa = stress;
    let (actual, allowable) = governing_utilization(&inputs, &case).unwrap();
    assert!(actual > allowable);
    assert!(actual < strongest);
    assert_eq!(allowable, weakest);
}

#[test]
fn extra_safety_factor_scales_ultimate_load_once_and_keeps_the_material_allowable() {
    let (baseline, baseline_inputs) = deck_inputs(1.0);
    let (factored, factored_inputs) = deck_inputs(1.3);
    for &(_, _, mid) in baseline_inputs.shell_identities() {
        assert_eq!(
            baseline_inputs.material_allowable_pa(mid),
            factored_inputs.material_allowable_pa(mid)
        );
    }
    assert_eq!(
        super::super::strongest_declared_allowable(&baseline),
        super::super::strongest_declared_allowable(&factored)
    );
    let base_cases = alas_struct::loads::load_cases(&baseline.requirements, 1.0);
    let factored_cases = alas_struct::loads::load_cases(&factored.requirements, 1.3);
    for (base, factored) in base_cases.iter().zip(&factored_cases) {
        let expected = if base.name == "level" { 1.0 } else { 1.3 };
        assert!((factored.total_force_n / base.total_force_n - expected).abs() < 1.0e-12);
    }
}

#[test]
fn complete_safe_samples_pass_and_tensor_demand_cannot_hide_in_a_summary() {
    let (_, _, _, result) = crate::feasibility::structure::tests::structural_fixture();
    let inputs = result.evaluation_inputs.unwrap();
    let mut case = StaticShellStressCase {
        subcase_id: 1,
        name: "pull-up",
        samples: samples(&inputs),
    };
    let (actual, allowable) = governing_utilization(&inputs, &case).unwrap();
    assert!(actual < allowable);
    let sample = &mut case.samples[0];
    sample.normal_x_pa = 2.0 * inputs.material_allowable_pa(sample.material_id).unwrap();
    sample.von_mises_pa = 0.0;
    let (actual, allowable) = governing_utilization(&inputs, &case).unwrap();
    assert!(actual > allowable);
}

#[test]
fn missing_duplicated_nonfinite_or_foreign_samples_fail_closed() {
    let (_, _, _, result) = crate::feasibility::structure::tests::structural_fixture();
    let inputs = result.evaluation_inputs.unwrap();
    let original = StaticShellStressCase {
        subcase_id: 1,
        name: "pull-up",
        samples: samples(&inputs),
    };
    for change in 0..5 {
        let mut case = original.clone();
        match change {
            0 => {
                case.samples.pop();
            }
            1 => case.samples[1] = case.samples[0].clone(),
            2 => case.samples[0].shear_xy_pa = f64::NAN,
            3 => case.samples[0].material_id = i64::MAX,
            _ => case.samples[0].property_id = i64::MAX,
        }
        assert!(governing_utilization(&inputs, &case).is_none());
    }
}

#[test]
fn nominal_shell_success_cannot_hide_missing_explicit_cases() {
    let (_, _, _, result) = crate::feasibility::structure::tests::structural_fixture();
    let inputs = result.evaluation_inputs.unwrap();
    let response = StaticResult {
        shell_stress: Some(StaticShellStressResponse {
            cases: vec![StaticShellStressCase {
                subcase_id: 1,
                name: "pull-up",
                samples: samples(&inputs),
            }],
            error: None,
            case_identity: StaticCaseIdentity::ExplicitSubcaseIds,
        }),
        ..Default::default()
    };
    let mut findings = Vec::new();
    append(Some(&inputs), "test FE", &response, &mut findings);
    assert!(findings
        .iter()
        .any(|finding| finding.code == FindingCode::StructuralSolverFailed));
}
