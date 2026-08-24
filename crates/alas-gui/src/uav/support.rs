// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Small typed constructors and catalogue-evidence helpers for UAV state.

use alas_uav::catalog::{ComponentRecord, Dimensions};
use alas_uav::optimizer::VariableBounds;

use super::ComponentRole;

pub(super) fn dimensions(length_m: f64, width_m: f64, height_m: f64) -> Dimensions {
    Dimensions {
        length_m,
        width_m,
        height_m,
    }
}

pub(super) fn missing_field<T>(
    gaps: &mut Vec<String>,
    role: ComponentRole,
    record: &ComponentRecord,
    value: Option<T>,
    field: &str,
) {
    if value.is_none() {
        gaps.push(format!("{} {}: {field}", role.label(), record.id));
    }
}

pub(super) fn bounds(minimum: f64, maximum: f64) -> VariableBounds {
    VariableBounds { minimum, maximum }
}
