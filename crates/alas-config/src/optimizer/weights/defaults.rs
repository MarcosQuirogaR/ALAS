// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The shipped defaults for [`ObjectiveWeights`].

use super::ObjectiveWeights;

impl Default for ObjectiveWeights {
    fn default() -> Self {
        Self {
            transport_planform_constraints_enabled: true,
            transport_shape_priors_enabled: false,
            geometric_body_alpha_min_deg: 2.0,
            geometric_body_alpha_max_deg: 4.0,
            geometric_body_alpha_penalty_scale: 200.0,
            min_root_wingbox_depth_m: 1.20,
            min_break_wingbox_depth_m: 0.65,
            min_break_wingbox_width_m: 2.50,
            wingbox_packaging_penalty_scale: 250.0,
            min_flap_area_fraction: 0.11,
            flap_area_penalty_scale: 250.0,
            max_root_bending_box_slenderness: 700.0,
            bending_slenderness_penalty_scale: 150.0,
            tankable_span_start_fraction: 0.10,
            tankable_span_end_fraction: 0.75,
            max_break_root_chord_ratio: 0.65,
            min_break_root_chord_ratio: 0.40,
            min_tip_root_chord_ratio: 0.16,
            taper_realism_penalty_scale: 250.0,
            te_root_angle_penalty_scale: 100.0,
            min_inboard_te_sweep_deg: 0.0,
            max_inboard_te_sweep_deg: 22.0,
            failure_cost: 1_000.0,
        }
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
