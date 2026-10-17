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
            // A mirrored surface rooted off the plane (wingtip device) has no
            // seam to close and takes the rotated root frame like a fin.
            let closes_seam = wing.symmetric && root.xyz_le[1].abs() < 1.0e-9;
            if closes_seam {
                symmetric_roots += 1;
            } else if wing.symmetric {
                assert!(
                    wing.name.starts_with("Wingtip Device"),
                    "{} {} off-plane root",
                    preset.name,
                    wing.name
                );
            } else if (tip.xyz_le[1] - root.xyz_le[1]).abs() < 1.0e-9
                && (tip.xyz_le[2] - root.xyz_le[2]).abs() > 1.0e-6
            {
                vertical_fins += 1;
            }
            let expected = format!(
                "SetParmVal( wing_{index}, \"RotateMatchDideralFlag\", \"XSec_0\", {:.12} );",
                if closes_seam { 0.0 } else { 1.0 }
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

#[test]
fn only_a_tailplane_passing_through_a_fin_is_exported_as_two_halves() {
    let mut split = Vec::new();
    for preset in alas_config::presets::registry() {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": preset.name}))
            .expect("registered preset");
        let airplane = AircraftBuilder::new(Some(config.geometry))
            .build(Some(&preset.design_vector), true)
            .expect("preset geometry");
        let exported = exported_wings(&airplane);
        if exported.len() == airplane.wings.len() {
            continue;
        }
        split.push(preset.name);
        // The halves are a mirror pair in planform and sit either side of
        // the fin plane, outboard of the clearance.
        let right = exported
            .iter()
            .find(|wing| wing.pose == WingPose::RightHalf)
            .expect("right half");
        let left = exported
            .iter()
            .find(|wing| wing.pose == WingPose::LeftHalf)
            .expect("left half");
        assert!(right.wing.xsecs[0].xyz_le[1] > 0.0);
        assert!(left.wing.xsecs[0].xyz_le[1] < 0.0);
        for (r, l) in right.wing.xsecs.iter().zip(&left.wing.xsecs) {
            assert!((r.xyz_le[1] + l.xyz_le[1]).abs() < 1.0e-12);
            assert!((r.chord - l.chord).abs() < 1.0e-12);
            assert!((r.xyz_le[0] - l.xyz_le[0]).abs() < 1.0e-12);
        }
        let script = render_script(&airplane, None, "root.vsp3", "cad.vsp3", "root.png");
        assert!(script.contains(&format!(
            "SetParmVal( wing_{}, \"X_Rel_Rotation\", \"XForm\", 180.000000000000 );",
            left.index
        )));
    }
    split.sort_unstable();
    assert_eq!(split, ["A400M", "ATR72-600"]);
}
