// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Element-attributed shell stresses. These are isotropic plane-stress
//! measures, not laminate failure indices or complete cap-strength checks.

use super::StaticCaseIdentity;

/// One printed shell-centroid fiber stress, SI, in element stress axes.
#[derive(Debug, Clone, PartialEq)]
pub struct StaticShellStressSample {
    /// Shell element identifier.
    pub element_id: i64,
    /// PSHELL identifier, resolved against the submitted deck.
    pub property_id: i64,
    /// PSHELL membrane material identifier, resolved against the deck.
    pub material_id: i64,
    /// Signed distance from shell reference plane, metres.
    pub fiber_distance_m: f64,
    /// Printed local normal stress, Pa.
    pub normal_x_pa: f64,
    /// Printed local normal stress, Pa.
    pub normal_y_pa: f64,
    /// Printed local engineering shear stress, Pa.
    pub shear_xy_pa: f64,
    /// Plane-stress von Mises derived from the three printed components, Pa.
    pub von_mises_pa: f64,
}

/// Complete centroid top/bottom shell stresses for one explicit subcase.
#[derive(Debug, Clone, PartialEq)]
pub struct StaticShellStressCase {
    /// Explicit output subcase identifier.
    pub subcase_id: i64,
    /// Requested load case name.
    pub name: &'static str,
    /// Two fiber samples per submitted CQUAD4/CTRIA3 element.
    pub samples: Vec<StaticShellStressSample>,
}

/// Complete shell-centroid stress output or an explicit extraction failure.
#[derive(Debug, Clone, PartialEq)]
pub struct StaticShellStressResponse {
    /// In requested load-case order; empty if extraction fails.
    pub cases: Vec<StaticShellStressCase>,
    /// Missing, duplicate, unsupported or nonfinite output diagnostic.
    pub error: Option<String>,
    /// Actual explicit output case identity, never inferred from position.
    pub case_identity: StaticCaseIdentity,
}
