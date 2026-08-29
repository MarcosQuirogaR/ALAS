// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native OpenVSP/VSPAERO mesh-refinement evidence.
//!
//! A VSPAERO coefficient is not a mesh-independent answer merely because the
//! native process completed. This module changes only OpenVSP's documented
//! wing tessellation controls (`SectTess_U` and `Tess_W`) and evaluates the
//! resulting comparable `CL` and `Cm` arrays. It deliberately contains no
//! drag comparison: the report's drag is a hybrid total-drag model whereas
//! this VSPAERO path is inviscid.

use alas_aero::vspaero::{VspaeroModel, VspaeroPolar};

/// One supported OpenVSP lifting-surface tessellation level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VspaeroMeshResolution {
    /// Stable label used in retained runtime evidence.
    pub label: &'static str,
    /// Spanwise tessellation count for every wing section.
    pub section_spanwise: usize,
    /// Chordwise tessellation count for every lifting surface.
    pub chordwise: usize,
}

/// Three successively finer, source-documented OpenVSP tessellation levels.
pub const VSPAERO_REFINEMENT_LEVELS: [VspaeroMeshResolution; 3] = [
    VspaeroMeshResolution {
        label: "coarse",
        section_spanwise: 4,
        chordwise: 9,
    },
    VspaeroMeshResolution {
        label: "medium",
        section_spanwise: 8,
        chordwise: 17,
    },
    VspaeroMeshResolution {
        label: "fine",
        section_spanwise: 16,
        chordwise: 33,
    },
];

/// Maximum absolute changes between two native mesh levels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VspaeroCoefficientChange {
    /// Largest pointwise `|CL_b - CL_a|` over the shared alpha schedule.
    pub max_abs_cl: f64,
    /// Largest pointwise `|Cm_b - Cm_a|` over the shared alpha schedule.
    pub max_abs_cm: f64,
}

/// Evidence-aware outcome of a three-level native refinement sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VspaeroRefinementVerdict {
    /// Both comparable coefficient changes shrink from medium to fine.
    ChangesShrink,
    /// At least one comparable coefficient does not show a shrinking change.
    ChangesDoNotShrink,
}

/// Comparable native VSPAERO changes and their refinement trend.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VspaeroRefinementAssessment {
    /// Coarse-to-medium change.
    pub coarse_to_medium: VspaeroCoefficientChange,
    /// Medium-to-fine change.
    pub medium_to_fine: VspaeroCoefficientChange,
    /// Trend only; it is not an absolute-error or validation claim.
    pub verdict: VspaeroRefinementVerdict,
}

/// Insert one resolution's documented OpenVSP controls into a generated script.
///
/// `wing_sections` contains the number of physical wing sections for each
/// `wing_{index}` in the generated script. The insertion is immediately before
/// `WriteVSPFile`, so it changes tessellation after all geometry and airfoil
/// definitions have been constructed and before `VSPAEROComputeGeometry` runs.
pub fn apply_vspaero_mesh_resolution(
    script: &str,
    wing_sections: &[usize],
    resolution: VspaeroMeshResolution,
) -> Result<String, String> {
    if resolution.section_spanwise < 2 || resolution.chordwise < 2 {
        return Err("OpenVSP tessellation counts must be at least two".to_owned());
    }
    if wing_sections.is_empty() || wing_sections.contains(&0) {
        return Err("at least one wing with one physical section is required".to_owned());
    }
    let marker = "    WriteVSPFile(";
    let Some(position) = script.find(marker) else {
        return Err("generated OpenVSP script has no WriteVSPFile call".to_owned());
    };

    let mut controls =
        String::from("    // Native VSPAERO refinement controls; geometry is unchanged.\n");
    for (wing_index, &sections) in wing_sections.iter().enumerate() {
        let wing = format!("wing_{wing_index}");
        if !script.contains(&format!("string {wing} = AddGeom( \"WING\", \"\" );")) {
            return Err(format!("generated OpenVSP script has no {wing}"));
        }
        for section in 1..=sections {
            controls.push_str(&format!(
                "    SetParmVal( {wing}, \"SectTess_U\", \"XSec_{section}\", {} );\n",
                resolution.section_spanwise
            ));
        }
        controls.push_str(&format!(
            "    SetParmVal( {wing}, \"Tess_W\", \"Shape\", {} );\n",
            resolution.chordwise
        ));
    }

    let mut refined = String::with_capacity(script.len() + controls.len());
    refined.push_str(&script[..position]);
    refined.push_str(&controls);
    refined.push_str(&script[position..]);
    Ok(refined)
}

/// Evaluate mesh changes for `CL` and `Cm` only after proving all three
/// polars still describe the same VSPAERO model, references, and alpha grid.
pub fn assess_vspaero_refinement(
    coarse: &VspaeroPolar,
    medium: &VspaeroPolar,
    fine: &VspaeroPolar,
) -> Result<VspaeroRefinementAssessment, String> {
    ensure_comparable_sequence(coarse, medium, "coarse", "medium")?;
    ensure_comparable_sequence(medium, fine, "medium", "fine")?;
    let coarse_to_medium = coefficient_change(coarse, medium);
    let medium_to_fine = coefficient_change(medium, fine);
    let verdict = if medium_to_fine.max_abs_cl < coarse_to_medium.max_abs_cl
        && medium_to_fine.max_abs_cm < coarse_to_medium.max_abs_cm
    {
        VspaeroRefinementVerdict::ChangesShrink
    } else {
        VspaeroRefinementVerdict::ChangesDoNotShrink
    };
    Ok(VspaeroRefinementAssessment {
        coarse_to_medium,
        medium_to_fine,
        verdict,
    })
}

fn ensure_comparable_sequence(
    first: &VspaeroPolar,
    second: &VspaeroPolar,
    first_label: &str,
    second_label: &str,
) -> Result<(), String> {
    if first.model != VspaeroModel::ALAS_VLM || second.model != VspaeroModel::ALAS_VLM {
        return Err("mesh refinement requires ALAS's lifting-surface VSPAERO model".to_owned());
    }
    if first.reference != second.reference {
        return Err(format!(
            "{first_label} and {second_label} VSPAERO references differ"
        ));
    }
    if first.points.len() != second.points.len() {
        return Err(format!(
            "{first_label} and {second_label} alpha schedules have different lengths"
        ));
    }
    for (index, (left, right)) in first.points.iter().zip(&second.points).enumerate() {
        if left.alpha_deg != right.alpha_deg
            || left.mach != right.mach
            || left.beta_deg != right.beta_deg
        {
            return Err(format!(
                "{first_label} and {second_label} operating points differ at row {index}"
            ));
        }
    }
    Ok(())
}

fn coefficient_change(first: &VspaeroPolar, second: &VspaeroPolar) -> VspaeroCoefficientChange {
    let (max_abs_cl, max_abs_cm) = first.points.iter().zip(&second.points).fold(
        (0.0_f64, 0.0_f64),
        |(cl, cm), (left, right)| {
            (
                cl.max((right.lift_coefficient - left.lift_coefficient).abs()),
                cm.max(
                    (right.pitching_moment_coefficient - left.pitching_moment_coefficient).abs(),
                ),
            )
        },
    );
    VspaeroCoefficientChange {
        max_abs_cl,
        max_abs_cm,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_aero::vspaero::{
        VspaeroCoefficientFrames, VspaeroGeometryScope, VspaeroMethod, VspaeroPolarPoint,
        VspaeroReference,
    };

    fn polar(cl_scale: f64, cm_scale: f64) -> VspaeroPolar {
        VspaeroPolar {
            reference: VspaeroReference {
                area_m2: 100.0,
                chord_m: 5.0,
                span_m: 30.0,
                moment_reference_m: [10.0, 0.0, 0.0],
            },
            model: VspaeroModel {
                method: VspaeroMethod::VortexLattice,
                geometry_scope: VspaeroGeometryScope::LiftingSurfacesOnly,
                frames: VspaeroCoefficientFrames {
                    forces_are_wind_axes: true,
                    moments_are_body_axes: true,
                },
            },
            points: [-1.0, 1.0]
                .into_iter()
                .map(|alpha_deg| VspaeroPolarPoint {
                    beta_deg: 0.0,
                    mach: 0.8,
                    alpha_deg,
                    reynolds: 1.0e7,
                    lift_coefficient: alpha_deg * cl_scale,
                    induced_drag_coefficient: 0.0,
                    total_drag_coefficient: 0.0,
                    side_force_coefficient: 0.0,
                    lift_to_drag: 0.0,
                    span_efficiency: None,
                    rolling_moment_coefficient: 0.0,
                    pitching_moment_coefficient: alpha_deg * cm_scale,
                    yawing_moment_coefficient: 0.0,
                })
                .collect(),
        }
    }

    #[test]
    fn resolution_controls_every_lifting_section_before_geometry_analysis() {
        let script = "int main()\n{\n    string wing_0 = AddGeom( \"WING\", \"\" );\n    string wing_1 = AddGeom( \"WING\", \"\" );\n    WriteVSPFile( \"case.vsp3\", SET_ALL );\n}\n";
        let refined = apply_vspaero_mesh_resolution(
            script,
            &[2, 1],
            VspaeroMeshResolution {
                label: "test",
                section_spanwise: 8,
                chordwise: 17,
            },
        )
        .unwrap_or_else(|error| panic!("apply OpenVSP mesh controls: {error}"));
        assert!(refined.contains("wing_0, \"SectTess_U\", \"XSec_1\", 8"));
        assert!(refined.contains("wing_0, \"SectTess_U\", \"XSec_2\", 8"));
        assert!(refined.contains("wing_1, \"SectTess_U\", \"XSec_1\", 8"));
        assert!(refined.contains("wing_1, \"Tess_W\", \"Shape\", 17"));
        assert!(matches!(
            (refined.find("SectTess_U"), refined.find("WriteVSPFile")),
            (Some(controls), Some(write)) if controls < write
        ));
    }

    #[test]
    fn shrinking_cl_and_cm_changes_are_reported_without_drag() {
        let coarse = polar(1.0, 1.0);
        let medium = polar(1.1, 1.1);
        let fine = polar(1.14, 1.14);
        let assessment = assess_vspaero_refinement(&coarse, &medium, &fine)
            .unwrap_or_else(|error| panic!("assess comparable refinement: {error}"));
        assert_eq!(assessment.verdict, VspaeroRefinementVerdict::ChangesShrink);
        assert!((assessment.coarse_to_medium.max_abs_cl - 0.1).abs() < 1.0e-12);
        assert!((assessment.medium_to_fine.max_abs_cm - 0.04).abs() < 1.0e-12);
    }

    #[test]
    fn a_schedule_change_is_not_a_mesh_sensitivity_result() {
        let coarse = polar(1.0, 1.0);
        let mut medium = polar(1.1, 1.1);
        medium.points[1].alpha_deg = 2.0;
        assert!(assess_vspaero_refinement(&coarse, &medium, &medium).is_err());
    }
}
