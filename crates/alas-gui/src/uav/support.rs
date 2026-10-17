// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Small typed constructors for UAV state.

use alas_uav::catalog::Dimensions;
use alas_uav::optimizer::VariableBounds;

pub(super) fn dimensions(length_m: f64, width_m: f64, height_m: f64) -> Dimensions {
    Dimensions {
        length_m,
        width_m,
        height_m,
    }
}

pub(super) fn bounds(minimum: f64, maximum: f64) -> VariableBounds {
    VariableBounds { minimum, maximum }
}
