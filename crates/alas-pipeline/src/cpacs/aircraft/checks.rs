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
