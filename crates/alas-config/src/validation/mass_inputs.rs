// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Sign and range of the input masses. A negative or non-finite mass used to
//! surface deep in the mass closure as an unrelated message (for example a
//! landing mass above a negative design gross mass); it is refused here by
//! field name instead.

use super::{Severity, ValidationIssue};
use crate::AlasConfig;

fn issue(field_path: &str, message: String) -> ValidationIssue {
    ValidationIssue {
        field_path: field_path.to_owned(),
        message,
        severity: Severity::Error,
    }
}

/// Masses that must be finite and strictly positive when given.
fn positive(issues: &mut Vec<ValidationIssue>, path: &str, label: &str, kg: f64) {
    if !kg.is_finite() || kg <= 0.0 {
        issues.push(issue(
            path,
            format!("{label} must be a finite mass greater than zero (kg); it is {kg}."),
        ));
    }
}

/// Masses that must be finite and not negative (zero is a valid "none").
fn non_negative(issues: &mut Vec<ValidationIssue>, path: &str, label: &str, kg: f64) {
    if !kg.is_finite() || kg < 0.0 {
        issues.push(issue(
            path,
            format!("{label} must be a finite mass of zero or more (kg); it is {kg}."),
        ));
    }
}

/// The takeoff mass, payloads, optional mass limits, fuel capacity and extra
/// fuel inputs must be physical masses; the passenger count may not be
/// negative.
pub(super) fn input_masses_are_physical(config: &AlasConfig) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    let requirements = &config.requirements;
    positive(
        &mut issues,
        "requirements.mtow_kg",
        "The maximum takeoff mass",
        requirements.mtow_kg,
    );
    non_negative(
        &mut issues,
        "requirements.cargo_payload_kg",
        "The cargo payload",
        requirements.cargo_payload_kg,
    );
    non_negative(
        &mut issues,
        "requirements.cargo_objective_kg",
        "The cargo objective",
        requirements.cargo_objective_kg,
    );
    non_negative(
        &mut issues,
        "requirements.max_structural_payload_kg",
        "The maximum structural payload",
        requirements.max_structural_payload_kg,
    );
    non_negative(
        &mut issues,
        "requirements.passenger_mass_kg",
        "The mass per passenger",
        requirements.passenger_mass_kg,
    );
    for (path, label, value) in [
        (
            "requirements.mlw_kg",
            "The maximum landing mass",
            requirements.mlw_kg,
        ),
        (
            "requirements.mzfw_kg",
            "The maximum zero-fuel mass",
            requirements.mzfw_kg,
        ),
        (
            "requirements.initial_cruise_mass_kg",
            "The initial cruise mass",
            requirements.initial_cruise_mass_kg,
        ),
        (
            "mass_model.flops_transport.maximum_fuel_capacity_kg",
            "The maximum usable fuel capacity",
            config.mass_model.flops_transport.maximum_fuel_capacity_kg,
        ),
    ] {
        if let Some(kg) = value {
            positive(&mut issues, path, label, kg);
        }
    }
    non_negative(
        &mut issues,
        "fuel_policy.additional_fuel_kg",
        "The additional fuel",
        config.fuel_policy.additional_fuel_kg,
    );
    non_negative(
        &mut issues,
        "fuel_policy.extra_fuel_kg",
        "The extra fuel",
        config.fuel_policy.extra_fuel_kg,
    );
    // MMO is an input next to the masses only because it is optional too: a Mach number,
    // subsonic, that sets MC and MD (`DesignRequirements::design_dive_mach`).
    if let Some(mach) = requirements.max_operating_mach {
        if !(mach.is_finite() && mach > 0.0 && mach < 1.0) {
            issues.push(issue(
                "requirements.max_operating_mach",
                format!("The maximum operating Mach number must be finite, above 0 and below 1; it is {mach}."),
            ));
        }
    }
    if requirements.num_passengers < 0 {
        issues.push(issue(
            "requirements.num_passengers",
            format!(
                "The passenger count cannot be negative; it is {}.",
                requirements.num_passengers
            ),
        ));
    }
    issues
}

// A failed unwrap here is the assertion failing.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_supersonic_or_missing_operating_mach_is_rejected() {
        let mut config = AlasConfig::default();
        config.requirements.max_operating_mach = Some(0.82);
        assert!(input_masses_are_physical(&config).is_empty());
        for mach in [0.0, 1.0, f64::NAN] {
            config.requirements.max_operating_mach = Some(mach);
            let issues = input_masses_are_physical(&config);
            assert_eq!(issues.len(), 1, "{mach}");
            assert_eq!(issues[0].field_path, "requirements.max_operating_mach");
        }
    }

    #[test]
    fn a_negative_or_non_finite_mass_is_refused_by_field_name() {
        let mut config = AlasConfig::default();
        assert!(input_masses_are_physical(&config).is_empty());
        config.requirements.mtow_kg = -100.0;
        config.requirements.cargo_payload_kg = f64::NAN;
        config.requirements.mlw_kg = Some(0.0);
        config.fuel_policy.extra_fuel_kg = -1.0;
        config.requirements.num_passengers = -3;
        let paths: Vec<String> = input_masses_are_physical(&config)
            .into_iter()
            .map(|issue| issue.field_path)
            .collect();
        for expected in [
            "requirements.mtow_kg",
            "requirements.cargo_payload_kg",
            "requirements.mlw_kg",
            "fuel_policy.extra_fuel_kg",
            "requirements.num_passengers",
        ] {
            assert!(
                paths.iter().any(|p| p == expected),
                "{expected} in {paths:?}"
            );
        }
    }

    #[test]
    fn every_registered_aircraft_has_physical_input_masses() {
        for name in crate::presets::available() {
            let config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
            assert!(input_masses_are_physical(&config).is_empty(), "{name}");
        }
    }
}
