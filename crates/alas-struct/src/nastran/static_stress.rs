// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Element-attributed shell stresses. These are isotropic plane-stress
//! measures, not laminate failure indices or complete cap-strength checks.

use super::StaticCaseIdentity;
use crate::mesh::{CoverPanel, ShellStressAxes};

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

/// One rib bay of one wing-box cover: the panel's mean stress along the
/// stringers, `E_skin N / sum(E A)` over the bay's skin and spar caps
/// ([`crate::mesh::CoverPanel::stringer_stress_pa`]). This is the stress the
/// stiffened-panel allowable is defined on (M. C. Y. Niu, *Airframe
/// Structural Design*, 1988, Sec. 5.6, p. 148; Torenbeek, *Advanced Aircraft
/// Design*, 2013, Eq. 11.71, p. 351).
#[derive(Debug, Clone, PartialEq)]
pub struct StaticCoverPanelStress {
    /// Whether the panel is in the upper cover.
    pub upper: bool,
    /// Mean span station of the bay's shells, m.
    pub y_mid_m: f64,
    /// Material identifier of the skin.
    pub skin_material_id: i64,
    /// Mean stress along the stringers, Pa (compression negative).
    pub stress_pa: f64,
    /// The bay's skin shell identifiers.
    pub shell_ids: Vec<i64>,
}

/// Every cover panel's stress in one explicit subcase.
#[derive(Debug, Clone, PartialEq)]
pub struct StaticCoverPanelCase {
    /// Explicit output subcase identifier.
    pub subcase_id: i64,
    /// Requested load case name.
    pub name: &'static str,
    /// One entry per rib bay and cover.
    pub panels: Vec<StaticCoverPanelStress>,
}

/// Complete cover-panel stresses or an explicit extraction failure.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StaticCoverPanelStressResponse {
    /// In requested load-case order; empty if extraction fails.
    pub cases: Vec<StaticCoverPanelCase>,
    /// Missing shell or cap stress diagnostic.
    pub error: Option<String>,
}

/// Assess every panel of one subcase from the solver's shell mid-surface
/// tensors (element axes) and cap-bar axial stresses, both SI.
pub(crate) fn cover_panel_case(
    panels: &[CoverPanel],
    axes: ShellStressAxes,
    subcase_id: i64,
    name: &'static str,
    membrane: impl Fn(i64) -> Option<[f64; 3]>,
    axial: impl Fn(i64) -> Option<f64>,
) -> Result<StaticCoverPanelCase, String> {
    let panels = panels
        .iter()
        .map(|panel| {
            let stress_pa = panel
                .stringer_stress_pa(axes, &membrane, &axial)
                .ok_or_else(|| {
                    format!(
                        "cover panel at y {:.3} m lacks a shell or cap stress in subcase {subcase_id}",
                        panel.y_mid_m
                    )
                })?;
            Ok(StaticCoverPanelStress {
                upper: panel.upper,
                y_mid_m: panel.y_mid_m,
                skin_material_id: panel.skin_mid,
                stress_pa,
                shell_ids: panel.shells.iter().map(|s| s.eid).collect(),
            })
        })
        .collect::<Result<_, String>>()?;
    Ok(StaticCoverPanelCase {
        subcase_id,
        name,
        panels,
    })
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
