// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The one cross-field rule tying a fuel scheme to the propulsion type it is
//! defined for. Split out of `validation.rs` to keep that file at its frozen
//! review size; see `docs/source-size-budgets.tsv`.

use crate::{AlasConfig, FuelScheme, PropulsionTechnology, Severity, ValidationIssue};

/// 14 CFR 121.645 prices flag and supplemental fuel for "turbine-engine
/// powered airplane[s] other than a turbo-propeller powered airplane"
/// (121.645(b), (c)); a turbo-propeller flag or supplemental operation falls
/// under 121.641 instead, which this program does not implement. Nothing in
/// the scheme's own type gates it by propulsion, so a turboprop preset could
/// otherwise be planned under the turbofan rule it is explicitly excluded
/// from. Physics review v1.2, finding F1.
pub(super) fn fuel_scheme_matches_the_propulsion_type(config: &AlasConfig) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    if config.fuel_policy.scheme == FuelScheme::FaaFlagSupplemental
        && config.geometry.engine.propulsion_technology == PropulsionTechnology::Turboprop
    {
        issues.push(ValidationIssue {
            field_path: "fuel_policy.scheme".to_owned(),
            message: "14 CFR 121.645 (flag and supplemental fuel) excludes turbo-propeller \
                airplanes by its own text (121.645(b), (c)); a turboprop preset falls under \
                121.641 instead, which this program does not implement. Choose a different \
                fuel scheme."
                .to_owned(),
            severity: Severity::Error,
        });
    }
    issues
}
