// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Material-attributed FE shell checks for the submitted isotropic proxy.

use std::collections::{BTreeMap, BTreeSet};

use super::{error, FindingCode, PhysicalFinding};
use alas_struct::nastran::{StaticCaseIdentity, StaticResult, StaticShellStressCase};

pub(super) fn append(
    inputs: Option<&crate::structural::EvaluationInputs>,
    solver: &str,
    response: &StaticResult,
    findings: &mut Vec<PhysicalFinding>,
) {
    let Some(stresses) = &response.shell_stress else {
        return;
    };
    let complete = stresses.error.is_none()
        && stresses.case_identity == StaticCaseIdentity::ExplicitSubcaseIds
        && stresses.cases.len() == 3
        && ["pull-up", "push-down", "level"].iter().all(|name| {
            stresses
                .cases
                .iter()
                .filter(|case| case.name == *name)
                .count()
                == 1
        });
    let Some(inputs) = inputs.filter(|inputs| complete && !inputs.shell_identities().is_empty())
    else {
        findings.push(error(
            FindingCode::StructuralSolverFailed,
            format!("{solver} attributed shell stresses lack complete cases or submitted-deck identities"),
            None, None, "",
        ));
        return;
    };
    for case in &stresses.cases {
        match governing_utilization(inputs, case) {
            Some((actual, allowable)) if actual > allowable => findings.push(error(
                FindingCode::StructuralStrengthViolation,
                format!("{solver} {} shell stress exceeds its submitted material allowable; isotropic proxy review required", case.name),
                Some(actual), Some(allowable), "Pa",
            )),
            Some(_) => {}
            None => findings.push(error(
                FindingCode::StructuralSolverFailed,
                format!("{solver} {} attributed shell stress output is incomplete or invalid", case.name),
                None, None, "",
            )),
        }
    }
}

fn governing_utilization(
    inputs: &crate::structural::EvaluationInputs,
    case: &StaticShellStressCase,
) -> Option<(f64, f64)> {
    let expected: BTreeMap<_, _> = inputs
        .shell_identities()
        .iter()
        .map(|&(element, property, material)| (element, (property, material)))
        .collect();
    if case.subcase_id <= 0 || case.samples.len() != 2 * expected.len() {
        return None;
    }
    let mut fibers: BTreeMap<i64, BTreeSet<u64>> = BTreeMap::new();
    let mut governing = (0.0, 1.0);
    for sample in &case.samples {
        if expected.get(&sample.element_id) != Some(&(sample.property_id, sample.material_id))
            || [
                sample.fiber_distance_m,
                sample.normal_x_pa,
                sample.normal_y_pa,
                sample.shear_xy_pa,
                sample.von_mises_pa,
            ]
            .iter()
            .any(|value| !value.is_finite())
            || sample.von_mises_pa < 0.0
        {
            return None;
        }
        let allowable = inputs.material_allowable_pa(sample.material_id)?;
        let vm = (sample.normal_x_pa.powi(2) + sample.normal_y_pa.powi(2)
            - sample.normal_x_pa * sample.normal_y_pa
            + 3.0 * sample.shear_xy_pa.powi(2))
        .sqrt();
        if !vm.is_finite() {
            return None;
        }
        // The scalar allowable and von Mises measure are the same isotropic
        // proxy as the product MAT1 deck, not an orthotropic laminate failure
        // criterion. Shell checks do not supply the absent CBAR cap stresses.
        let actual = vm.max(sample.von_mises_pa);
        if actual / allowable > governing.0 / governing.1 {
            governing = (actual, allowable);
        }
        if !fibers
            .entry(sample.element_id)
            .or_default()
            .insert(sample.fiber_distance_m.to_bits())
        {
            return None;
        }
    }
    (fibers.len() == expected.len() && fibers.values().all(|values| values.len() == 2))
        .then_some(governing)
}

#[cfg(test)]
mod tests;
