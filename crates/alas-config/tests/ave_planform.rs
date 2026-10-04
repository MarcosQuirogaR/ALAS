// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The notional AVE planform keeps its root-to-kink trailing edge running aft.

// A test asserts on values it loaded from the registry, so a failed expect is
// the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig};

/// Upper limit of the `root_to_kink_te_angle` constraint, degrees from the aft
/// fuselage axis to the root-to-kink trailing-edge ray in the x-aft frame.
const MAX_TE_ANGLE_DEG: f64 = 90.0;

/// Margin kept below the limit, degrees (engineering choice).
const MIN_MARGIN_DEG: f64 = 1.0;

#[test]
fn ave_root_to_kink_trailing_edge_clears_its_limit_at_every_wing_shift() {
    let preset = presets::get("AVE").expect("AVE is registered");
    let config =
        AlasConfig::from_value(&serde_json::json!({ "preset": "AVE" })).expect("AVE configuration");
    // The wing shift translates both stations equally, so the angle must not
    // depend on it; the sweep covers the range the preset documents.
    for wing_x_shift_m in [-1.70, 0.0, 0.5, 2.64] {
        let mut design = preset.design_vector;
        design.wing_x_shift_m = wing_x_shift_m;
        let planform = config
            .geometry
            .wing
            .transport_planform(&design)
            .expect("AVE planform");
        let root_te_x_m = planform.root.leading_edge_x_m + planform.root.chord_m;
        let kink_te_x_m = planform.kink.leading_edge_x_m + planform.kink.chord_m;
        let angle_deg = (planform.kink.y_m - planform.root.y_m)
            .atan2(kink_te_x_m - root_te_x_m)
            .to_degrees();
        assert!(
            angle_deg <= MAX_TE_ANGLE_DEG - MIN_MARGIN_DEG,
            "root-to-kink TE angle {angle_deg} deg at wing shift {wing_x_shift_m} m"
        );
    }
}
