// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Directional SI <-> FLOPS-unit conversions shared by the transport-category
//! equations. Every factor is the exact `alas-units` constant.

use alas_units::{FOOT, POUND_FORCE, POUND_MASS};

/// Mass in pounds from kilograms.
pub(super) fn lb(kilograms: f64) -> f64 {
    kilograms / POUND_MASS
}

/// Mass in kilograms from pounds.
pub(super) fn kg(pounds: f64) -> f64 {
    pounds * POUND_MASS
}

/// Force in pounds-force from newtons.
pub(super) fn lbf(newtons: f64) -> f64 {
    newtons / POUND_FORCE
}

/// Length in feet from metres.
pub(super) fn ft(metres: f64) -> f64 {
    metres / FOOT
}

/// Area in square feet from square metres.
pub(super) fn ft2(square_metres: f64) -> f64 {
    square_metres / (FOOT * FOOT)
}
