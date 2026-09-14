// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Enforces that any preset configuration with composite structural materials
//! carries an explicit non-laminate / non-certification declaration in its
//! sizing result, and that metallic presets carry no declaration.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod support;

use alas_config::materials;
use alas_config::{presets, AlasConfig, DesignRequirements};
use alas_struct::sizing::size_wingbox;
use support::build_geometry;

#[test]
fn composite_material_assignment_carries_a_non_laminate_declaration() {
    let all_presets = presets::available();
    assert!(
        !all_presets.is_empty(),
        "presets registry must be populated"
    );

    for preset_name in all_presets {
        let value = serde_json::json!({ "preset": preset_name });
        let config = AlasConfig::from_value(&value)
            .unwrap_or_else(|e| panic!("failed to load config for preset {preset_name}: {e:?}"));
        let structures = &config.structures;

        let skin_mat = materials::get(&structures.skin_material)
            .unwrap_or_else(|e| panic!("skin material for {preset_name}: {e:?}"));
        let web_mat = materials::get(&structures.spar_web_material)
            .unwrap_or_else(|e| panic!("web material for {preset_name}: {e:?}"));
        let cap_mat = materials::get(&structures.spar_cap_material)
            .unwrap_or_else(|e| panic!("cap material for {preset_name}: {e:?}"));
        let rib_mat = materials::get(&structures.rib_material)
            .unwrap_or_else(|e| panic!("rib material for {preset_name}: {e:?}"));

        let has_composite = [skin_mat, web_mat, cap_mat, rib_mat]
            .iter()
            .any(|m| m.category == "composite");

        let (spar_fractions, spar_full_span) = structures.resolved_spars();
        let geometry = build_geometry(&spar_fractions, &spar_full_span);
        let req = DesignRequirements::default();
        let sizing = size_wingbox(
            &geometry, structures, &req, skin_mat, web_mat, cap_mat, rib_mat,
        );

        if has_composite {
            let decl = sizing.composite_declaration
                .unwrap_or_else(|| panic!("preset {preset_name} has composite material but sizing result carries no declaration"));
            assert!(
                decl.applicability
                    .contains("Not a certified laminate analysis"),
                "declaration for {preset_name} must contain the non-certification statement"
            );
            assert!(
                decl.applicability
                    .contains("Effective isotropic proxy for a laminate wing box"),
                "declaration for {preset_name} must state effective isotropic proxy"
            );
            assert!(
                !decl.source.is_empty(),
                "declaration for {preset_name} must have a non-empty source"
            );
            // Per strict audit override: uncalibrated relative uncertainty must be
            // explicitly represented as unknown (None), never fabricated as a numeric value or 0.0.
            assert!(
                decl.relative_uncertainty.is_none(),
                "relative_uncertainty must be explicitly None (uncalibrated/unknown) for {preset_name}"
            );
        } else {
            assert!(
                sizing.composite_declaration.is_none(),
                "all-metallic preset {preset_name} must have no composite declaration"
            );
        }
    }
}
