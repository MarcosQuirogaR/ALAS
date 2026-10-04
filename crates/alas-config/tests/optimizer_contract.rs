// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Public serialized-configuration probes for the optimizer dispatch contract.

use alas_config::{validate, AlasConfig, Severity};

#[test]
fn the_serialized_default_selects_the_product_search_and_has_no_strategy(
) -> Result<(), serde_json::Error> {
    let value = serde_json::to_value(AlasConfig::default())?;
    assert_eq!(
        value["optimizer"]["solver"]["method"],
        serde_json::json!("differential_evolution")
    );
    assert!(value["optimizer"]["solver"].get("strategy").is_none());
    Ok(())
}

#[test]
fn serialized_optimizer_typos_survive_loading_but_are_blocked_by_public_validation(
) -> Result<(), serde_json::Error> {
    let mut value = serde_json::to_value(AlasConfig::default())?;
    value["optimizer"]["solver"]["method"] = serde_json::json!("differential_evoluton");

    let config: AlasConfig = serde_json::from_value(value)?;
    let issues = validate(&config);
    assert!(
        issues.iter().any(|issue| {
            issue.field_path == "optimizer.solver.method" && issue.severity == Severity::Error
        }),
        "the method typo must be a blocking validation issue: {issues:?}"
    );
    Ok(())
}
