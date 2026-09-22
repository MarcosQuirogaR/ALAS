// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Validation for the selected FLOPS mass architecture.

use crate::{AlasConfig, PropulsionTechnology};

use super::{Severity, ValidationIssue};

/// Validate only the FLOPS nodes selected by the active mass architecture.
/// Inactive legacy nodes remain loadable for reference compatibility.
pub(super) fn active_issues(config: &AlasConfig) -> Vec<ValidationIssue> {
    let mass = &config.mass_model;
    let mut issues = Vec::new();
    if !mass.architecture_is_coherent() {
        issues.push(ValidationIssue {
            field_path: "mass_model.mass_architecture".to_owned(),
            message: format!(
                "Mass architecture {:?} disagrees with one or more derived group selectors; reload or migrate the configuration before running.",
                mass.mass_architecture
            ),
            severity: Severity::Error,
        });
    }
    if !mass.mass_architecture.is_pure_flops() {
        return issues;
    }
    if let Err(error) = mass.flops_structure.validate() {
        issues.push(ValidationIssue {
            field_path: structure_validation_path(&error),
            message: error,
            severity: Severity::Error,
        });
    }
    if config.geometry.engine.propulsion_technology == PropulsionTechnology::Turboprop {
        if let Err(field) = mass.flops_turboprop.validate() {
            issues.push(ValidationIssue {
                field_path: format!("mass_model.flops_turboprop.{field}"),
                message: format!(
                    "FLOPS turboprop input {field} is nonphysical or outside its supported range."
                ),
                severity: Severity::Error,
            });
        }
    }
    issues
}

fn structure_validation_path(error: &str) -> String {
    let field = error
        .strip_prefix("FLOPS ")
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap_or("flops_structure");
    format!("mass_model.flops_structure.{field}")
}
