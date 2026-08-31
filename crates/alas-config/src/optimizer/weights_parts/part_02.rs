// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


impl Default for ObjectiveWeights {
    fn default() -> Self {
        Self {
            ld_weight: 1.0,
            alpha_penalty_scale: 5.0,
            alpha_min_penalty_deg: 0.0,
            alpha_max_penalty_deg: 10.0,
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
            span_penalty_per_m: 0.02,
            cd0_penalty_scale: 50.0,
            area_penalty_scale: 0.5,
            wing_loading_penalty_scale: 0.005,
            cg_penalty_scale: 200.0,
            cg_envelope_penalty_scale: 400_000.0,
            cg_envelope_reward: 5.0,
            fuel_penalty_scale: 50.0,
            fuel_volume_penalty_scale: 300.0,
            static_margin_penalty_scale: 20.0,
            thickness_floor: 0.90,
            thickness_penalty_scale: 20.0,
            fuselage_floor_m: 60.0,
            fuselage_penalty_scale: 5.0,
            min_hstab_area_fraction: 0.15,
            min_vstab_area_fraction: 0.07,
            tail_area_penalty_scale: 150.0,
            min_hstab_volume_coef: 0.75,
            max_hstab_volume_coef: 1.25,
            min_vstab_volume_coef: 0.06,
            max_vstab_volume_coef: 0.13,
            tail_volume_penalty_scale: 200.0,
            max_break_root_chord_ratio: 0.65,
            min_break_root_chord_ratio: 0.40,
            min_tip_root_chord_ratio: 0.16,
            taper_realism_penalty_scale: 250.0,
            te_root_angle_penalty_scale: 100.0,
            min_inboard_te_sweep_deg: 0.0,
            max_inboard_te_sweep_deg: 22.0,
            min_wing_position_fraction: 0.27,
            wing_position_penalty_scale: 300.0,
            payload_shortfall_penalty_scale: 1000.0,
            fineness_ratio_max: 15.0,
            fineness_ratio_penalty_scale: 500.0,
            failure_cost: 1_000.0,
            instability_failure_cost: 1_000.0,
        }
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Entry, Kind};

    fn kind_of(name: &str) -> Kind {
        let schema = ObjectiveWeights::default().schema();
        match &schema.field(name).unwrap().entry {
            Entry::Leaf(leaf) => leaf.kind,
            Entry::Node(_) => panic!("{name} is not a group"),
        }
    }

    #[test]
    fn a_weight_is_offered_as_a_slider_because_only_its_ratio_matters() {
        assert_eq!(kind_of("ld_weight"), Kind::WeightSlider);
        assert_eq!(kind_of("cd0_penalty_scale"), Kind::WeightSlider);
        assert_eq!(kind_of("span_penalty_per_m"), Kind::WeightSlider);
    }

    #[test]
    fn a_threshold_that_happens_to_end_in_a_weight_suffix_is_offered_as_one_too() {
        // `thickness_floor`, `fuselage_floor_m` and the two costs are
        // physical thresholds rather than relative weights, and the naming
        // rule catches them anyway. Reproduced rather than corrected --
        // recorded as a deviation-candidate in docs/PORTING.md.
        assert_eq!(kind_of("thickness_floor"), Kind::WeightSlider);
        assert_eq!(kind_of("fuselage_floor_m"), Kind::WeightSlider);
        assert_eq!(kind_of("failure_cost"), Kind::WeightSlider);
    }

    #[test]
    fn a_bound_that_is_not_named_as_a_weight_stays_a_number() {
        assert_eq!(kind_of("alpha_min_penalty_deg"), Kind::Float);
        assert_eq!(kind_of("geometric_body_alpha_min_deg"), Kind::Float);
        assert_eq!(kind_of("min_break_wingbox_depth_m"), Kind::Float);
        assert_eq!(kind_of("min_hstab_volume_coef"), Kind::Float);
        assert_eq!(kind_of("cg_envelope_reward"), Kind::Float);
    }

    #[test]
    fn every_min_max_pair_leaves_a_usable_range() {
        let weights = ObjectiveWeights::default();
        assert!(weights.alpha_min_penalty_deg < weights.alpha_max_penalty_deg);
        assert!(weights.geometric_body_alpha_min_deg < weights.geometric_body_alpha_max_deg);
        assert!(weights.tankable_span_start_fraction < weights.tankable_span_end_fraction);
        assert!(weights.min_inboard_te_sweep_deg < weights.max_inboard_te_sweep_deg);
        assert!(weights.min_hstab_volume_coef < weights.max_hstab_volume_coef);
        assert!(weights.min_vstab_volume_coef < weights.max_vstab_volume_coef);
    }

    #[test]
    fn transport_constraints_are_on_by_default_but_remain_explicitly_switchable() {
        let weights = ObjectiveWeights::default();

        assert!(weights.transport_planform_constraints_enabled);
        assert!(!weights.transport_shape_priors_enabled);
        assert_eq!(weights.geometric_body_alpha_min_deg, 2.0);
        assert_eq!(weights.geometric_body_alpha_max_deg, 4.0);
    }

    #[test]
    fn the_envelope_penalty_dominates_the_lift_to_drag_reward() {
        // A design outside its CG envelope is not flyable, so the penalty has
        // to outweigh any aerodynamic gain that could be traded for it --
        // otherwise the search buys L/D with legality.
        let weights = ObjectiveWeights::default();
        assert!(weights.cg_envelope_penalty_scale > weights.ld_weight * 1000.0);
    }

    #[test]
    fn the_static_margin_pull_stays_small_next_to_the_reward_it_competes_with() {
        // Its own explanation says raising it much above 20-30 crowds out the
        // aerodynamic signal the search exists to follow.
        let weights = ObjectiveWeights::default();
        assert!(weights.static_margin_penalty_scale <= 30.0);
    }
}

