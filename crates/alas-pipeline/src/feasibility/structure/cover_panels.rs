// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! FE wing-box cover check: each rib bay of each cover as one stiffened
//! panel, its mean stress along the stringers against the panel allowable.
//!
//! The stiffened-panel allowable (380 MPa for 7075-T6, the crippling cut-off
//! of the stringer section) is defined on the end load per unit width over
//! the smeared skin-stringer thickness, `N / T` with `T = (t b + A_s) / b`
//! (M. C. Y. Niu, *Airframe Structural Design*, Conmilit Press, 1988,
//! Sec. 5.6, p. 148 and Sec. 5.3, p. 136; Torenbeek, *Advanced Aircraft
//! Design*, 2013, Eq. 11.71, p. 351). The skin sheet's own von Mises is
//! checked separately against the material yield (`shell_stress`).

use super::{error, warning, FindingCode, PhysicalFinding};
use alas_struct::nastran::{StaticCoverPanelCase, StaticResult};

pub(super) fn append(
    inputs: Option<&crate::structural::EvaluationInputs>,
    solver: &str,
    response: &StaticResult,
    findings: &mut Vec<PhysicalFinding>,
) {
    let Some(panels) = &response.cover_panel_stress else {
        return;
    };
    let complete = panels.error.is_none()
        && panels.cases.len() == 3
        && ["pull-up", "push-down", "level"].iter().all(|name| {
            panels
                .cases
                .iter()
                .filter(|case| case.name == *name)
                .count()
                == 1
        });
    let Some(inputs) = inputs.filter(|_| complete) else {
        findings.push(error(
            FindingCode::StructuralSolverFailed,
            format!(
                "{solver} cover panel stresses are incomplete: {}",
                panels
                    .error
                    .as_deref()
                    .unwrap_or("missing load cases or submitted-deck identities")
            ),
            None,
            None,
            "",
        ));
        return;
    };
    for case in &panels.cases {
        let Some(scan) = scan(inputs, case) else {
            findings.push(error(
                FindingCode::StructuralSolverFailed,
                format!(
                    "{solver} {} cover panel stress has no finite panel allowable",
                    case.name
                ),
                None,
                None,
                "",
            ));
            continue;
        };
        if let Some((actual, allowable)) = scan.assessed {
            if actual > allowable {
                findings.push(error(
                    FindingCode::StructuralStrengthViolation,
                    format!("{solver} {} wing-box cover panel stress along the stringers exceeds its stiffened-panel allowable", case.name),
                    Some(actual),
                    Some(allowable),
                    "Pa",
                ));
            }
        }
        if let Some((actual, allowable)) = scan.load_introduction {
            if actual > allowable {
                findings.push(warning(
                    FindingCode::StructuralStrengthViolation,
                    format!("{solver} {} cover panel stress exceeds its stiffened-panel allowable only in rib bays touching a load-introduction or constraint zone (root clamp, RBE3 spiders, engine mass), which are not assessed; local detail review required", case.name),
                    Some(actual),
                    Some(allowable),
                    "Pa",
                ));
            }
        }
    }
}

/// Governing `(|stress|, allowable)` of the assessed panels and of the panels
/// with a shell in a load-introduction zone.
struct Scan {
    assessed: Option<(f64, f64)>,
    load_introduction: Option<(f64, f64)>,
}

fn scan(inputs: &crate::structural::EvaluationInputs, case: &StaticCoverPanelCase) -> Option<Scan> {
    let mut result = Scan {
        assessed: None,
        load_introduction: None,
    };
    for panel in &case.panels {
        let allowable = inputs
            .panel_allowable_pa(panel.skin_material_id)
            .filter(|value| value.is_finite() && *value > 0.0)?;
        if !panel.stress_pa.is_finite() {
            return None;
        }
        let actual = panel.stress_pa.abs();
        let zone = panel
            .shell_ids
            .iter()
            .any(|&element| inputs.in_load_introduction(element));
        let slot = if zone {
            &mut result.load_introduction
        } else {
            &mut result.assessed
        };
        if slot.is_none_or(|(a, b)| actual / allowable > a / b) {
            *slot = Some((actual, allowable));
        }
    }
    Some(result)
}

// A failed unwrap here is the assertion failing.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_struct::nastran::{StaticCoverPanelStress, StaticCoverPanelStressResponse};

    fn response(stress_pa: f64) -> StaticResult {
        let cases = ["pull-up", "push-down", "level"]
            .iter()
            .enumerate()
            .map(|(i, name)| StaticCoverPanelCase {
                subcase_id: i as i64 + 1,
                name,
                panels: vec![StaticCoverPanelStress {
                    upper: true,
                    y_mid_m: 5.0,
                    skin_material_id: 0,
                    stress_pa,
                    shell_ids: vec![-1],
                }],
            })
            .collect();
        StaticResult {
            cover_panel_stress: Some(StaticCoverPanelStressResponse { cases, error: None }),
            ..Default::default()
        }
    }

    #[test]
    fn missing_panel_inputs_or_incomplete_output_is_an_error_not_a_pass() {
        let mut findings = Vec::new();
        append(None, "MSC", &response(-1.0e8), &mut findings);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, FindingCode::StructuralSolverFailed);
        let mut broken = response(-1.0e8);
        broken.cover_panel_stress.as_mut().unwrap().cases.pop();
        let mut findings = Vec::new();
        append(None, "MSC", &broken, &mut findings);
        assert_eq!(findings[0].code, FindingCode::StructuralSolverFailed);
    }

    #[test]
    fn panel_stress_is_held_to_the_panel_allowable_of_its_skin_material() {
        let (_, _, _, result) = crate::feasibility::structure::tests::structural_fixture();
        let inputs = result.evaluation_inputs.unwrap();
        let (element, _, mid) = *inputs
            .shell_identities()
            .iter()
            .find(|(element, _, _)| !inputs.in_load_introduction(*element))
            .expect("an assessed shell");
        let allowable = inputs.panel_allowable_pa(mid).expect("panel allowable");
        assert!(allowable < inputs.material_allowable_pa(mid).unwrap() + 1.0);
        for (factor, violations) in [(0.9, 0), (-1.1, 1)] {
            let mut response = response(factor * allowable);
            for case in &mut response.cover_panel_stress.as_mut().unwrap().cases {
                case.panels[0].skin_material_id = mid;
                case.panels[0].shell_ids = vec![element];
            }
            let mut findings = Vec::new();
            append(Some(&inputs), "MSC", &response, &mut findings);
            assert_eq!(findings.len(), 3 * violations, "{findings:?}");
            assert!(findings
                .iter()
                .all(|f| f.code == FindingCode::StructuralStrengthViolation));
        }
    }

    #[test]
    fn a_deck_without_cover_panels_adds_no_finding() {
        let mut findings = Vec::new();
        append(None, "MSC", &StaticResult::default(), &mut findings);
        assert!(findings.is_empty());
    }
}
