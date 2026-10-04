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
        match sampled_domain_indicator(case) {
            Some(value) if value <= config.structures.max_linear_curvature_relative_error => {}
            Some(value) => findings.push(super::curvature_finding(
                case.name,
                FindingCode::StructuralFemModelDomain,
                format!("{solver} {} sampled front-spar bending rotation or translation gradient exceeds the linear curvature error budget", case.name),
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

fn sampled_domain_indicator(case: &StaticSpanwiseCase) -> Option<f64> {
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
        let chord = subtract(case.xyz_m[i], case.xyz_m[i - 1]);
        let length = norm(chord);
        if !length.is_finite() || length <= 0.0 {
            return None;
        }
        let tangent = chord.map(|component| component / length);
        let gradient = subtract(case.translations_m[i], case.translations_m[i - 1])
            .map(|component| component / length);
        let longitudinal_stretch = 1.0 + dot(gradient, tangent);
        if !longitudinal_stretch.is_finite() || longitudinal_stretch <= 0.0 {
            return None;
        }
        // All response vectors and GRID positions are basic-frame quantities.
        // Resolve both transverse translation components against the actual
        // swept/dihedral spar tangent, not just basic Z over projected Y.
        let slope = norm(cross(gradient, tangent)) / longitudinal_stretch;
        maximum = maximum.max(curvature_indicator(slope)?);
        for rotation in &case.rotations_rad[i - 1..=i] {
            // Euler-Bernoulli small-rotation kinematics: r cross tangent is
            // the linearized change of section direction. Tangent-parallel
            // rotation is torsion and does not enter this bending indicator.
            // Finite rotations need a nonlinear assessment; these samples
            // are necessary checks and cannot qualify domain replacement.
            maximum = maximum.max(curvature_indicator(norm(cross(*rotation, tangent)))?);
        }
    }
    Some(maximum)
}

fn curvature_indicator(slope: f64) -> Option<f64> {
    let value = (1.0 + slope * slope).powf(1.5) - 1.0;
    value.is_finite().then_some(value)
}

fn subtract(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}

fn norm(vector: [f64; 3]) -> f64 {
    vector[0].hypot(vector[1]).hypot(vector[2])
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub(super) fn has_complete_response(response: &StaticResult) -> bool {
    response.spanwise.as_ref().is_some_and(|curves| {
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
            && curves
                .cases
                .iter()
                .all(|case| sampled_domain_indicator(case).is_some())
    })
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
        assert!(sampled_domain_indicator(&case).unwrap() > 0.05);
    }

    #[test]
    fn incomplete_nonfinite_or_duplicate_stations_fail_closed() {
        let mut case = curve();
        case.y_m[1] = 0.0;
        assert!(sampled_domain_indicator(&case).is_none());
        let mut case = curve();
        case.rotations_rad[0][1] = f64::NAN;
        assert!(sampled_domain_indicator(&case).is_none());
        let mut case = curve();
        case.translations_m.pop();
        assert!(sampled_domain_indicator(&case).is_none());
    }

    #[test]
    fn small_secants_cannot_hide_large_bending_rotations() {
        let mut case = curve();
        case.translations_m.fill([0.0; 3]);
        case.rotations_rad[2] = [0.5, 0.0, 0.0];
        assert!(sampled_domain_indicator(&case).unwrap() > 0.05);
    }

    #[test]
    fn swept_dihedral_tangent_separates_bending_from_torsion() {
        let mut case = curve();
        case.xyz_m = vec![[0.0; 3], [9.0, 9.0, 9.0], [10.0, 10.0, 10.0]];
        case.translations_m.fill([0.0; 3]);
        let component = 0.5 / 3.0_f64.sqrt();
        case.rotations_rad.fill([component; 3]);
        assert!(sampled_domain_indicator(&case).unwrap().abs() < 1.0e-12);
        case.rotations_rad[2] = [0.5 / 2.0_f64.sqrt(), -0.5 / 2.0_f64.sqrt(), 0.0];
        assert!(sampled_domain_indicator(&case).unwrap() > 0.05);
    }

    #[test]
    fn horizontal_translation_gradient_is_a_bending_domain_indicator() {
        let mut case = curve();
        case.translations_m[2] = [1.0, 0.0, 0.0];
        assert!(sampled_domain_indicator(&case).unwrap() > 0.05);
        // Stretch parallel to the tangent is not transverse bending.
        case.translations_m[2] = [0.0, 1.0, 0.0];
        assert!(sampled_domain_indicator(&case).unwrap().abs() < 1.0e-12);
    }
}
