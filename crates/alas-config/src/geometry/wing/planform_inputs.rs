// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Scalar checks on the planform inputs (kept apart from the station types to
//! bound the size of each file).

use super::planform::TransportPlanformError;

/// Reject a non-finite planform input.
pub(super) fn validate_finite(
    field: &'static str,
    value: f64,
) -> Result<(), TransportPlanformError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(TransportPlanformError::NonFinite { field, value })
    }
}

/// Reject a non-positive planform input.
pub(super) fn validate_positive(
    field: &'static str,
    value: f64,
) -> Result<(), TransportPlanformError> {
    if value > 0.0 {
        Ok(())
    } else {
        Err(TransportPlanformError::NonPositive { field, value })
    }
}

/// Reject a leading-edge sweep at or beyond 89 degrees.
pub(super) fn validate_sweep(
    field: &'static str,
    value: f64,
) -> Result<(), TransportPlanformError> {
    if value.abs() < 89.0 {
        Ok(())
    } else {
        Err(TransportPlanformError::InvalidSweep { field, value })
    }
}
