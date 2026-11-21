// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Transform, reference and section validation for the CPACS conversion.

use super::*;

pub(super) fn transform_translation(
    transformation: Option<&CpacsTransformation>,
    parent_origin: [f64; 3],
) -> [f64; 3] {
    let translation = transformation
        .and_then(|value| value.translation)
        .unwrap_or([0.0, 0.0, 0.0]);
    if transformation.and_then(|value| value.translation_reference.as_deref()) == Some("absGlobal")
    {
        translation
    } else {
        add(parent_origin, translation)
    }
}

pub(super) fn validate_rigid_parent_transform(
    uid: &str,
    transformation: Option<&CpacsTransformation>,
    kind: &'static str,
) -> Result<(), CpacsAircraftError> {
    let scaling = transformation
        .and_then(|value| value.scaling)
        .unwrap_or([1.0, 1.0, 1.0]);
    let rotation = transformation
        .and_then(|value| value.rotation)
        .unwrap_or([0.0, 0.0, 0.0]);
    if scaling != [1.0, 1.0, 1.0] || rotation != [0.0, 0.0, 0.0] {
        return Err(CpacsAircraftError::UnsupportedTransformation {
            path: format!("{kind}/{uid}/transformation"),
            reason: "parent scaling and rotation are not representable".to_owned(),
        });
    }
    Ok(())
}

pub(super) fn is_unit_ellipse(points: &[[f64; 3]]) -> bool {
    if points.len() < 3 {
        return false;
    }
    let y_max = points
        .iter()
        .map(|point| point[1].abs())
        .fold(0.0, f64::max);
    let z_max = points
        .iter()
        .map(|point| point[2].abs())
        .fold(0.0, f64::max);
    points
        .iter()
        .all(|point| nearly(point[0], 0.0) && nearly(point[1].hypot(point[2]), 1.0))
        && nearly(y_max, 1.0)
        && nearly(z_max, 1.0)
}

pub(super) fn validate_wing_section_transform(
    transformation: Option<&CpacsTransformation>,
    uid: &str,
) -> Result<(), CpacsAircraftError> {
    let scaling = transformation
        .and_then(|value| value.scaling)
        .unwrap_or([1.0, 1.0, 1.0]);
    let rotation = transformation
        .and_then(|value| value.rotation)
        .unwrap_or([0.0, 0.0, 0.0]);
    if !scaling.into_iter().all(|value| nearly(value, 1.0))
        || !nearly(rotation[1], 0.0)
        || !nearly(rotation[2], 0.0)
    {
        return Err(CpacsAircraftError::UnsupportedTransformation {
            path: format!("wing/section/{uid}/transformation"),
            reason: "section scaling and non-dihedral rotation are not representable".to_owned(),
        });
    }
    Ok(())
}

pub(super) fn validate_fuselage_section_transform(
    transformation: Option<&CpacsTransformation>,
    uid: &str,
) -> Result<(), CpacsAircraftError> {
    let scaling = transformation
        .and_then(|value| value.scaling)
        .unwrap_or([1.0, 1.0, 1.0]);
    let rotation = transformation
        .and_then(|value| value.rotation)
        .unwrap_or([0.0, 0.0, 0.0]);
    if !scaling.into_iter().all(|value| nearly(value, 1.0)) || !is_zero_vector(rotation) {
        return Err(CpacsAircraftError::UnsupportedTransformation {
            path: format!("fuselage/section/{uid}/transformation"),
            reason: "fuselage section scaling and rotation are not representable".to_owned(),
        });
    }
    Ok(())
}

pub(super) fn validate_reference(name: &'static str, value: f64) -> Result<(), CpacsAircraftError> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(CpacsAircraftError::InvalidReference { name, value })
    }
}

pub(super) fn require_engine_value(
    field: &'static str,
    value: f64,
    predicate: impl FnOnce(f64) -> bool,
) -> Result<(), CpacsAircraftError> {
    if value.is_finite() && predicate(value) {
        Ok(())
    } else {
        Err(CpacsAircraftError::InvalidEngineValue { field, value })
    }
}

pub(super) fn nearly(left: f64, right: f64) -> bool {
    (left - right).abs() <= 1.0e-10 * left.abs().max(right.abs()).max(1.0)
}

pub(super) fn is_zero_vector(value: [f64; 3]) -> bool {
    value.into_iter().all(|component| nearly(component, 0.0))
}

fn add(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] + right[0], left[1] + right[1], left[2] + right[2]]
}

/// Give each imported wing section whose CPACS rotation about x differs from
/// the dihedral its leading-edge line implies the plane that rotation states.
///
/// The writer stores every section's frame as that rotation: the bisector of
/// the adjacent leading-edge segments for an ordinary section, and the kept
/// plane of one with a [`WingXSec::span_axis`] (a wingtip device's root and
/// path stations). Reading the rotation back only where it differs leaves every
/// ordinary section on the derived frame it had, and restores the kept planes.
pub(super) fn keep_section_planes(sections: &mut [WingXSec], rotations_deg: &[f64]) {
    /// Rotation differences below this (degrees) are rounding, not a stated plane.
    const PLANE_TOLERANCE_DEG: f64 = 1.0e-6;
    let derived: Vec<Option<f64>> = (0..sections.len())
        .map(|index| derived_section_dihedral_deg(sections, index))
        .collect();
    for (index, section) in sections.iter_mut().enumerate() {
        let (Some(derived), Some(&given)) = (derived[index], rotations_deg.get(index)) else {
            continue;
        };
        if (given - derived).abs() > PLANE_TOLERANCE_DEG {
            let angle = given.to_radians();
            section.span_axis = Some([0.0, angle.cos(), angle.sin()]);
        }
    }
}

/// The dihedral (degrees) of the frame a section with no kept plane takes from
/// its adjacent leading-edge segments, projected on the YZ plane, or `None`
/// for a single section or a segment with no YZ extent.
fn derived_section_dihedral_deg(sections: &[WingXSec], index: usize) -> Option<f64> {
    let yz = |from: &WingXSec, to: &WingXSec| {
        let (dy, dz) = (to.xyz_le[1] - from.xyz_le[1], to.xyz_le[2] - from.xyz_le[2]);
        let length = dy.hypot(dz);
        (length > 1.0e-12).then(|| [dy / length, dz / length])
    };
    let last = sections.len().checked_sub(1)?;
    if last == 0 {
        return None;
    }
    let direction = if index == 0 {
        yz(&sections[0], &sections[1])?
    } else if index == last {
        yz(&sections[last - 1], &sections[last])?
    } else {
        let (before, after) = (
            yz(&sections[index - 1], &sections[index])?,
            yz(&sections[index], &sections[index + 1])?,
        );
        [before[0] + after[0], before[1] + after[1]]
    };
    Some(direction[1].atan2(direction[0]).to_degrees())
}
