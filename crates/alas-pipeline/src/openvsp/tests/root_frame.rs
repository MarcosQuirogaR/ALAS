// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

#[test]
fn preset_exports_close_symmetric_roots_without_flattening_vertical_fins() {
    for preset in alas_config::presets::registry() {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": preset.name}))
            .expect("registered preset");
        let airplane = AircraftBuilder::new(Some(config.geometry))
            .build(Some(&preset.design_vector), true)
            .expect("preset geometry");
        let script = render_script(&airplane, None, "root.vsp3", "cad.vsp3", "root.png");
        assert!(validate_script(&script).is_ok());
        let mut symmetric_roots = 0;
        let mut vertical_fins = 0;
        for (index, wing) in airplane.wings.iter().enumerate() {
            let root = &wing.xsecs[0];
            let tip = &wing.xsecs[1];
            if wing.symmetric {
                symmetric_roots += 1;
                assert!(root.xyz_le[1].abs() < 1.0e-9, "{} root plane", preset.name);
            } else if (tip.xyz_le[1] - root.xyz_le[1]).abs() < 1.0e-9
                && (tip.xyz_le[2] - root.xyz_le[2]).abs() > 1.0e-6
            {
                vertical_fins += 1;
            }
            let expected = format!(
                "SetParmVal( wing_{index}, \"RotateMatchDideralFlag\", \"XSec_0\", {:.12} );",
                if wing.symmetric { 0.0 } else { 1.0 }
            );
            assert!(
                script.contains(&expected),
                "{} {} root frame",
                preset.name,
                wing.name
            );
            // Closing the seam must not erase the aircraft's root incidence.
            assert!(script.contains(&format!(
                "SetParmVal( wing_{index}, \"Twist\", \"XSec_0\", {:.12} );",
                root.twist
            )));
        }
        assert!(
            symmetric_roots >= 2,
            "{} wing and horizontal tail",
            preset.name
        );
        assert!(vertical_fins >= 1, "{} vertical fin", preset.name);
    }
}
