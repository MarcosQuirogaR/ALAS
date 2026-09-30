// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Load-time checks on the takeoff-mass sizing mode and its band.
//!
//! The messages are static sentences because the desktop translates a
//! validation message by exact catalogue match.

use crate::{AlasConfig, FuelScheme, MtowSizing, Severity, ValidationIssue};

/// Band half-width above which a warning is raised: a band wider than half
/// its target no longer tells two weight variants apart. Engineering
/// estimate.
const WIDE_BAND_FRACTION: f64 = 0.5;

fn issue(field_path: &str, message: &str, severity: Severity) -> ValidationIssue {
    ValidationIssue {
        field_path: field_path.to_owned(),
        message: message.to_owned(),
        severity,
    }
}

/// Every issue with the MTOW target, the band and their combination with
/// the structure override and the fuel policy.
pub(super) fn mtow_sizing_issues(config: &AlasConfig) -> Vec<ValidationIssue> {
    let objective = &config.optimizer.objective;
    let mut issues = Vec::new();
    let fraction = objective.mtow_band_fraction;
    if !(fraction > 0.0 && fraction < 1.0) {
        issues.push(issue(
            "optimizer.objective.mtow_band_fraction",
            "The MTOW band fraction must lie strictly between zero and one.",
            Severity::Error,
        ));
    } else if fraction > WIDE_BAND_FRACTION {
        issues.push(issue(
            "optimizer.objective.mtow_band_fraction",
            "An MTOW band wider than half its target no longer constrains the design to one weight variant.",
            Severity::Warning,
        ));
    }
    if !objective.mtow_target_kg.is_finite() || objective.mtow_target_kg < 0.0 {
        issues.push(issue(
            "optimizer.objective.mtow_target_kg",
            "The MTOW target must be finite and nonnegative; zero uses the requirement MTOW.",
            Severity::Error,
        ));
    }
    let mode = objective.mtow_sizing;
    if mode.requires_mission_sized_evaluation()
        && config
            .mass_model
            .flops_structure
            .design_gross_mass_kg
            .is_some()
    {
        issues.push(issue(
            "mass_model.flops_structure.design_gross_mass_kg",
            "The explicit structural design gross mass overrides the closed MTOW: the structure stays at the declared weight instead of following the sizing mode.",
            Severity::Warning,
        ));
    }
    if mode == MtowSizing::PayloadAdjusted && config.fuel_policy.scheme == FuelScheme::TripFuelOnly
    {
        issues.push(issue(
            "fuel_policy.scheme",
            "The payload-adjusted MTOW closes with no reserves under the trip-fuel-only scheme, so the sized aircraft cannot be dispatched under an operational fuel policy.",
            Severity::Warning,
        ));
    }
    issues
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn paths(config: &AlasConfig) -> Vec<(String, Severity)> {
        mtow_sizing_issues(config)
            .into_iter()
            .map(|issue| (issue.field_path, issue.severity))
            .collect()
    }

    #[test]
    fn the_shipped_default_raises_nothing() {
        assert!(paths(&AlasConfig::default()).is_empty());
    }

    #[test]
    fn the_band_and_target_domains_are_enforced_at_load() {
        let mut config = AlasConfig::default();
        config.optimizer.objective.mtow_band_fraction = 0.0;
        config.optimizer.objective.mtow_target_kg = f64::NAN;
        let found = paths(&config);
        assert!(found.contains(&(
            "optimizer.objective.mtow_band_fraction".to_owned(),
            Severity::Error
        )));
        assert!(found.contains(&(
            "optimizer.objective.mtow_target_kg".to_owned(),
            Severity::Error
        )));
        config.optimizer.objective.mtow_band_fraction = 0.6;
        config.optimizer.objective.mtow_target_kg = 0.0;
        assert_eq!(
            paths(&config),
            vec![(
                "optimizer.objective.mtow_band_fraction".to_owned(),
                Severity::Warning
            )]
        );
        // The rule is registered with the configuration validator.
        assert!(crate::validate(&config)
            .iter()
            .any(|issue| issue.field_path == "optimizer.objective.mtow_band_fraction"));
    }

    #[test]
    fn a_design_mode_warns_about_a_structure_override_and_trip_fuel_only() {
        let mut config = AlasConfig::default();
        config.mass_model.flops_structure.design_gross_mass_kg = Some(300_000.0);
        config.fuel_policy.scheme = FuelScheme::TripFuelOnly;
        assert!(paths(&config).is_empty());
        config.optimizer.objective.mtow_sizing = MtowSizing::MtowBand;
        assert_eq!(
            paths(&config),
            vec![(
                "mass_model.flops_structure.design_gross_mass_kg".to_owned(),
                Severity::Warning
            )]
        );
        config.optimizer.objective.mtow_sizing = MtowSizing::PayloadAdjusted;
        assert_eq!(paths(&config).len(), 2);
    }
}
