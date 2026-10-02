// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Necessary sampled-curve domain checks, not a full 3D FE qualification.

use super::{error, AlasConfig, FindingCode, PhysicalFinding};
use alas_struct::nastran::{StaticCaseIdentity, StaticResult, StaticSpanwiseCase};

pub(super) fn append(
    config: &AlasConfig,
    solver: &str,
    response: &StaticResult,
    findings: &mut Vec<PhysicalFinding>,
) {
    let curves = response.spanwise.as_ref().filter(|curves| {
        curves.error.is_none()
            && curves.case_identity == StaticCaseIdentity::ExplicitSubcaseIds
            && curves.cases.len() == 3
            && ["pull-up", "push-down", "level"].iter().all(|name| {
                curves
                    .cases
                    .iter()
                    .filter(|case| case.name == *name)
                    .count()
                    == 1
            })
    });
    let Some(curves) = curves else {
        findings.push(error(
            FindingCode::StructuralSolverFailed,
            format!("{solver} lacks complete, explicitly identified front-spar static curves"),
            None,
            None,
            "",
        ));
        return;
    };
    for case in &curves.cases {
        match curvature_lower_bound(case) {
            Some(value) if value <= config.structures.max_linear_curvature_relative_error => {}
            Some(value) => findings.push(super::curvature_finding(
                case.name,
                FindingCode::StructuralFemModelDomain,
                format!("{solver} {} sampled front-spar slope exceeds the linear curvature error budget", case.name),
                value, config.structures.max_linear_curvature_relative_error,
            )),
            None => findings.push(error(
                FindingCode::StructuralSolverFailed,
                format!("{solver} {} front-spar curve has invalid or incomplete stations/response", case.name),
                None, None, "",
            )),
        }
    }
}

fn curvature_lower_bound(case: &StaticSpanwiseCase) -> Option<f64> {
    let n = case.y_m.len();
    if n < 2
        || case.grid_ids.len() != n
        || case.xyz_m.len() != n
        || case.translations_m.len() != n
        || case.rotations_rad.len() != n
        || !case.load_factor.is_finite()
        || case.subcase_id <= 0
        || case.y_m.iter().any(|v| !v.is_finite())
        || case
            .xyz_m
            .iter()
            .chain(&case.translations_m)
            .chain(&case.rotations_rad)
            .flatten()
            .any(|v| !v.is_finite())
        || case
            .grid_ids
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != n
    {
        return None;
    }
    // Product extraction verifies every expected root-to-tip front-spar GRID.
    // A swept perpendicular tip rib ends inboard of nominal wing semispan;
    // use actual GRID stations, not a fictitious nominal-span endpoint.
    let tolerance = 1.0e-6 * case.y_m[n - 1].abs().max(1.0);
    if case
        .xyz_m
        .iter()
        .zip(&case.y_m)
        .any(|(xyz, y)| (xyz[1] - y).abs() > tolerance)
    {
        return None;
    }
    let mut maximum = 0.0_f64;
    for i in 1..n {
        let dy = case.y_m[i] - case.y_m[i - 1];
        if dy <= 0.0 {
            return None;
        }
        let slope = (case.translations_m[i][2] - case.translations_m[i - 1][2]) / dy;
        let value = (1.0 + slope * slope).powf(1.5) - 1.0;
        if !value.is_finite() {
            return None;
        }
        maximum = maximum.max(value);
    }
    Some(maximum)
}

// A test asserts on values it constructed here directly, so a failed unwrap is
// the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn curve() -> StaticSpanwiseCase {
        StaticSpanwiseCase {
            subcase_id: 1,
            name: "pull-up",
            load_factor: 3.75,
            grid_ids: vec![1, 2, 3],
            y_m: vec![0.0, 9.0, 10.0],
            xyz_m: vec![[0.0, 0.0, 0.0], [0.0, 9.0, 0.0], [0.0, 10.0, 0.0]],
            translations_m: vec![[0.0; 3], [0.0; 3], [0.0, 0.0, 1.0]],
            rotations_rad: vec![[0.0; 3]; 3],
        }
    }

    #[test]
    fn local_slope_cannot_hide_behind_a_small_tip_to_span_ratio() {
        let case = curve();
        assert!(super::super::fem_curvature_lower_bound(1.0, Some(10.0)).unwrap() < 0.05);
        assert!(curvature_lower_bound(&case).unwrap() > 0.05);
    }

    #[test]
    fn incomplete_nonfinite_or_duplicate_stations_fail_closed() {
        let mut case = curve();
        case.y_m[1] = 0.0;
        assert!(curvature_lower_bound(&case).is_none());
        let mut case = curve();
        case.rotations_rad[0][1] = f64::NAN;
        assert!(curvature_lower_bound(&case).is_none());
        let mut case = curve();
        case.translations_m.pop();
        assert!(curvature_lower_bound(&case).is_none());
    }
}
