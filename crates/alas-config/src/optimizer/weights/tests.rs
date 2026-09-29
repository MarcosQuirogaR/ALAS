// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// or expect is the assertion failing, not a library invariant being broken.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::{ConfigNode, Entry, Kind};

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
    // rule catches them anyway. Reproduced rather than corrected:
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
    // to outweigh any aerodynamic gain that could be traded for it,
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
