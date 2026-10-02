// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Finite typed residual construction and normalized constraint values.
use super::{ConstraintFamily, ConstraintResidual, NUMERICAL_SLACK};
use alas_config::ConstraintPolicy;

impl ConstraintResidual {
    /// Build a residual whose normalized violation is `raw_residual` scaled
    /// by the magnitude of `limit`: the convention every scaled residual in
    /// `mdo::residuals` uses, so a limit near zero cannot divide the
    /// violation toward infinity. Violations below [`NUMERICAL_SLACK`] of
    /// the limit are reported in `raw_residual` but not counted.
    pub(crate) fn scaled(
        id: &'static str,
        family: ConstraintFamily,
        actual: f64,
        limit: f64,
        unit: &'static str,
        raw_residual: f64,
        policy: ConstraintPolicy,
    ) -> Self {
        let scale = limit.abs().max(1e-9);
        let normalized_violation = if [actual, limit, raw_residual].iter().all(|v| v.is_finite()) {
            (raw_residual / scale - NUMERICAL_SLACK).max(0.0)
        } else {
            f64::INFINITY
        };
        Self {
            id,
            family,
            actual,
            limit,
            unit,
            raw_residual,
            normalized_violation,
            policy,
        }
    }

    /// Build a residual whose normalized violation is supplied directly, for
    /// a source (the CG envelope, an evaluation failure) that already
    /// carries its own normalization.
    #[allow(clippy::too_many_arguments)] // one named field per physical quantity of the residual; a struct would only rename them once
    pub(crate) fn direct(
        id: &'static str,
        family: ConstraintFamily,
        actual: f64,
        limit: f64,
        unit: &'static str,
        raw_residual: f64,
        normalized_violation: f64,
        policy: ConstraintPolicy,
    ) -> Self {
        let normalized_violation = if [actual, limit, raw_residual, normalized_violation]
            .iter()
            .all(|value| value.is_finite())
            && normalized_violation >= 0.0
        {
            normalized_violation
        } else {
            f64::INFINITY
        };
        Self {
            id,
            family,
            actual,
            limit,
            unit,
            raw_residual,
            normalized_violation,
            policy,
        }
    }

    /// Whether this residual is on the infeasible side of its limit.
    #[must_use]
    pub fn violated(&self) -> bool {
        !self.normalized_violation.is_finite() || self.normalized_violation > 0.0
    }
}
