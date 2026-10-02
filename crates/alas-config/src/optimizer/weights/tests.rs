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
fn a_threshold_that_happens_to_end_in_a_weight_suffix_is_offered_as_a_slider() {
    // The naming rule catches the failure cost, which is a flat cost rather
    // than a relative weight.
    assert_eq!(kind_of("failure_cost"), Kind::WeightSlider);
    assert_eq!(
        kind_of("wingbox_packaging_penalty_scale"),
        Kind::WeightSlider
    );
}

#[test]
fn a_bound_that_is_not_named_as_a_weight_stays_a_number() {
    assert_eq!(kind_of("geometric_body_alpha_min_deg"), Kind::Float);
    assert_eq!(kind_of("min_break_wingbox_depth_m"), Kind::Float);
}

#[test]
fn every_min_max_pair_leaves_a_usable_range() {
    let weights = ObjectiveWeights::default();
    assert!(weights.geometric_body_alpha_min_deg < weights.geometric_body_alpha_max_deg);
    assert!(weights.tankable_span_start_fraction < weights.tankable_span_end_fraction);
    assert!(weights.min_inboard_te_sweep_deg < weights.max_inboard_te_sweep_deg);
    assert!(weights.min_break_root_chord_ratio < weights.max_break_root_chord_ratio);
}

#[test]
fn transport_constraints_are_on_by_default_but_remain_explicitly_switchable() {
    let weights = ObjectiveWeights::default();

    assert!(weights.transport_planform_constraints_enabled);
    assert!(!weights.transport_shape_priors_enabled);
}
