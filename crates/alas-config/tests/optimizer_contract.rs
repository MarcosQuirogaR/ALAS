// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Public serialized-configuration probes for the optimizer dispatch contract.

use alas_config::{validate, AlasConfig, Severity};

#[test]
fn the_serialized_default_selects_differential_evolution_explicitly(
) -> Result<(), serde_json::Error> {
    let value = serde_json::to_value(AlasConfig::default())?;
    assert_eq!(
        value["optimizer"]["solver"]["method"],
        serde_json::json!("differential_evolution")
    );
    assert_eq!(
        value["optimizer"]["solver"]["strategy"],
        serde_json::json!("best1bin")
    );
    Ok(())
}

#[test]
fn serialized_optimizer_typos_survive_loading_but_are_blocked_by_public_validation(
) -> Result<(), serde_json::Error> {
    let mut value = serde_json::to_value(AlasConfig::default())?;
    value["optimizer"]["solver"]["method"] = serde_json::json!("differential_evoluton");
    value["optimizer"]["solver"]["strategy"] = serde_json::json!("best1bni");

    let config: AlasConfig = serde_json::from_value(value)?;
    let issues = validate(&config);
    for path in ["optimizer.solver.method", "optimizer.solver.strategy"] {
        assert!(
            issues
                .iter()
                .any(|issue| issue.field_path == path && issue.severity == Severity::Error),
            "{path} must be a blocking validation issue: {issues:?}"
        );
    }
    Ok(())
}
