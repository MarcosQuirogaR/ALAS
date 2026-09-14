// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Integration tests for composite proxy declaration propagation into `WingReconciliation`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::design_variables::DesignVector;
use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_mass::wing_reconciliation::reconcile;

#[test]
fn an_all_metallic_wing_carries_no_composite_proxy_declaration() {
    // The default spar caps are CFRP UD, so the metallic case is explicit.
    let mut config = AlasConfig::default();
    config.structures.spar_cap_material = "Al 7075-T6".to_owned();
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&DesignVector::default()), true)
        .expect("default geometry builds");
    let reconciliation = reconcile(&config, &DesignVector::default(), &plane, None)
        .expect("default reconciliation succeeds");
    assert!(
        reconciliation.primary_declaration().is_none(),
        "metallic default wing must not carry a composite proxy declaration"
    );
}

#[test]
fn composite_wing_carries_primary_declaration_into_reconciliation() {
    let mut config = AlasConfig::default();
    config.structures.skin_material = "CFRP QI".to_owned();
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&DesignVector::default()), true)
        .expect("plane geometry builds");
    let reconciliation = reconcile(&config, &DesignVector::default(), &plane, None)
        .expect("composite reconciliation succeeds");
    let decl = reconciliation
        .primary_declaration()
        .expect("composite wing must carry composite proxy declaration");
    assert!(
        decl.source.starts_with("Open source gap"),
        "the declaration names the evidence gap, got {:?}",
        decl.source
    );
    assert!(
        decl.applicability
            .starts_with("Effective isotropic proxy for a laminate wing box"),
        "the declaration states the proxy's scope, got {:?}",
        decl.applicability
    );
    assert_eq!(decl.relative_uncertainty, None);
}

#[test]
fn registered_presets_propagate_composite_declaration_if_composite() {
    for preset_name in ["B787-9", "A220-300"] {
        let preset = presets::get(preset_name).expect("preset exists");
        let value = serde_json::json!({
            "preset": preset_name,
        });
        let config = AlasConfig::from_value(&value).expect("config parses from value");
        assert_eq!(config.structures.skin_material, "CFRP QI");

        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .expect("plane builds");
        let reconciliation = reconcile(&config, &preset.design_vector, &plane, None)
            .unwrap_or_else(|err| panic!("{preset_name} reconciliation failed: {err:?}"));
        let decl = reconciliation
            .primary_declaration()
            .unwrap_or_else(|| panic!("{preset_name} should carry primary_declaration"));
        assert!(
            decl.source.starts_with("Open source gap"),
            "{preset_name}: the declaration names the evidence gap, got {:?}",
            decl.source
        );
        assert!(
            decl.applicability
                .starts_with("Effective isotropic proxy for a laminate wing box"),
            "{preset_name}: the declaration states the proxy's scope, got {:?}",
            decl.applicability
        );
        assert_eq!(decl.relative_uncertainty, None);
    }

    for preset_name in ["A320-200", "A340-300", "A380-800", "DC-10", "ATR72-600"] {
        let preset = presets::get(preset_name).expect("preset exists");
        let value = serde_json::json!({
            "preset": preset_name,
        });
        let config = AlasConfig::from_value(&value).expect("config parses from value");
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .expect("plane builds");
        let reconciliation = reconcile(&config, &preset.design_vector, &plane, None)
            .unwrap_or_else(|err| panic!("{preset_name} reconciliation failed: {err:?}"));
        assert!(
            reconciliation.primary_declaration().is_none(),
            "{preset_name} is metallic and should NOT carry primary_declaration"
        );
    }
}
