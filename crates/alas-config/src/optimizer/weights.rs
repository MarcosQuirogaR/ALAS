// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/optimizer_config.py (`ObjectiveWeights`)
// Reference: alas @ rust-port-baseline.

//! The weights and thresholds shaping the cost the optimizer minimises.
//!
//! The objective is a lift-to-drag reward plus a stack of soft, continuous
//! penalties: an angle-of-attack window, a span cost, parasite drag, wing area
//! and loading bounds, the static margin, the centre-of-gravity envelope, the
//! fuel budget and tank volume, tail area and volume coefficients, taper
//! realism, wing position, fineness ratio and payload shortfall.
//!
//! # Why almost everything here is soft
//!
//! Exactly one condition short-circuits the assembly and returns a flat cost:
//! a candidate that cannot be built or evaluated at all. Everything else --
//! including a design that is physically invalid, with a static margin below
//! the floor or a centre of gravity outside the envelope -- still has its
//! lift-to-drag computed, with a large but graduated penalty added on top.
//!
//! That distinction is the whole design of this module. An early return
//! throws away the gradient: a population of invalid candidates all scoring
//! the same flat cost gives the search nothing to climb, so it cannot find
//! its way back to compliance. A graduated penalty leaves the signal intact
//! and lets it.
//!
//! Fields whose names end in a weight suffix are offered as sliders rather
//! than as numbers, because only their ratio to each other means anything.
//! That rule is upstream's and catches a few thresholds that are not weights
//! at all -- a thickness floor, a fuselage length floor, the failure costs --
//! which is reproduced rather than corrected.

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// Weights and thresholds for the objective function.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct ObjectiveWeights {
    /// Reward multiplier on lift-to-drag ratio.
    #[config(
        label = "L/D reward weight",
        help = "Primary reward multiplier on L/D. Raise to prioritize aerodynamic efficiency over all penalties below."
    )]
    pub ld_weight: f64,

    /// Cost of trimming outside the angle-of-attack window.
    #[config(
        label = "Trim-alpha penalty weight",
        help = "Penalizes cruise trim alpha outside [alpha_min_penalty_deg, alpha_max_penalty_deg]. Zero cost inside the window, quadratic outside. Kept small so the optimizer focuses on L/D."
    )]
    pub alpha_penalty_scale: f64,

    /// Bottom of the acceptable trim window.
    #[config(
        label = "Trim-alpha window: minimum",
        unit = "deg",
        help = "Lower bound of the acceptable cruise trim-alpha window (no penalty above this)."
    )]
    pub alpha_min_penalty_deg: f64,

    /// Top of the acceptable trim window.
    #[config(
        label = "Trim-alpha window: maximum",
        unit = "deg",
        help = "Upper bound of the acceptable cruise trim-alpha window (no penalty below this)."
    )]
    pub alpha_max_penalty_deg: f64,

    /// Linear cost per metre of span, standing in for structural weight.
    #[config(
        label = "Wingspan penalty (per metre)",
        help = "Small linear penalty per metre of span -- discourages excessively large wings."
    )]
    pub span_penalty_per_m: f64,

    /// Cost of parasite drag.
    #[config(
        label = "Parasite-drag (CD0) penalty weight",
        help = "Rewards lower parasite drag (CD0); a thinner/cleaner shape reduces this term's cost. Typical cruise CD0 is 0.016-0.020."
    )]
    pub cd0_penalty_scale: f64,

    /// Cost of exceeding the wing-area bound.
    #[config(
        label = "Max wing-area penalty weight",
        help = "Penalty per m^2 the wing area exceeds requirements.max_wing_area_m2."
    )]
    pub area_penalty_scale: f64,

    /// Cost of falling below the wing-loading bound.
    #[config(
        label = "Min wing-loading penalty weight",
        help = "Penalty per (kg/m^2)^2 below requirements.min_wing_loading_kg_m2."
    )]
    pub wing_loading_penalty_scale: f64,

    /// Retained so old saved configurations still load.
    #[config(
        label = "(Legacy, unused) CG/aero-balance mismatch weight",
        help = "NOT read by the cost function (objective.py) -- kept only so old saved YAML configs referencing this key still load without error. Originally intended to penalize (Delta x_cg / MAC)^2, but that formula was never actually wired up; the field's real runtime effect was fully redundant with static_margin_penalty_scale, since both applied to the identical static-margin-vs-target term, so the two are consolidated into that single, correctly-named, appropriately-soft term. Physical CG-envelope compliance is enforced separately by cg_envelope_penalty_scale/cg_envelope_reward below."
    )]
    pub cg_penalty_scale: f64,

    /// Cost of a centre of gravity outside its envelope.
    #[config(
        label = "CG-envelope violation penalty weight",
        help = "Penalizes the physical CG exceeding the [fwd, aft] CG envelope limits (% MAC, from Design Requirements). At 5% MAC beyond the limit the cost is comparable to one L/D unit, rising steeply further out."
    )]
    pub cg_envelope_penalty_scale: f64,

    /// Reward for an envelope that is compliant throughout.
    #[config(
        label = "CG-envelope compliance reward",
        help = "Reward applied to cost if the aircraft's entire operational CG envelope is within limits."
    )]
    pub cg_envelope_reward: f64,

    /// Cost of a design whose weights leave no fuel.
    #[config(
        label = "Negative-fuel penalty weight",
        help = "Penalizes negative fuel mass (OEW + payload exceeding MTOW), normalized by MTOW."
    )]
    pub fuel_penalty_scale: f64,

    /// Cost of a wing too small to hold the fuel it needs.
    #[config(
        label = "Insufficient wing fuel-volume penalty weight",
        help = "Penalizes the wing's physical usable fuel-tank volume (physics.performance.wing_fuel_volume_m3, Torenbeek geometric estimate) being too small to hold the fuel mass the weight & balance analysis says this design actually needs -- a wing that's too thin/small/tapered to carry its own required fuel is not a buildable aircraft, independent of whether the MTOW fuel-mass budget itself closes. Quadratic on the fractional shortfall (required_fuel - tank_capacity) / required_fuel."
    )]
    pub fuel_volume_penalty_scale: f64,

    /// Pull toward the target static margin.
    #[config(
        label = "Static-margin target penalty weight",
        help = "SOFT preference nudging compliant-but-suboptimal candidates toward requirements.target_static_margin -- NOT a hard requirement (the physical floor, min_physical_static_margin, and the CG-envelope itself are enforced separately and are what actually keep a design safe/legal). Kept deliberately small relative to -L/D (typically 15-25) so this doesn't crowd out genuine aerodynamic improvements: raising it much above ~20-30 risks the optimizer chasing an exact SM match instead of exploring shape space, overwhelming the L/D signal the search is meant to prioritize."
    )]
    pub static_margin_penalty_scale: f64,

    /// How thin the section may get before it is penalized.
    #[config(
        label = "Minimum airfoil thickness scale",
        help = "Minimum allowed airfoil thickness scale (relative to the reference section) before the thickness penalty kicks in."
    )]
    pub thickness_floor: f64,

    /// Cost of a section thinner than the floor.
    #[config(
        label = "Thin-airfoil penalty weight",
        help = "Penalty weight applied when the morphed airfoil thickness collapses below thickness_floor."
    )]
    pub thickness_penalty_scale: f64,

    /// How short the fuselage may get before it is penalized.
    #[config(
        label = "Minimum fuselage length",
        unit = "m",
        help = "Minimum allowed fuselage length before the too-short-fuselage penalty kicks in."
    )]
    pub fuselage_floor_m: f64,

    /// Cost of a fuselage shorter than the floor.
    #[config(
        label = "Too-short-fuselage penalty weight",
        help = "Penalty weight applied when the fuselage shrinks below fuselage_floor_m."
    )]
    pub fuselage_penalty_scale: f64,

    /// Smallest acceptable horizontal tail, as a share of wing area.
    #[config(
        label = "Minimum H-stab area fraction",
        help = "Minimum allowed horizontal-stabiliser area as a fraction of wing area. Typical transports: ~20-30%. Prevents a 'tiny tail on a huge moment arm' cheat."
    )]
    pub min_hstab_area_fraction: f64,

    /// Smallest acceptable fin, as a share of wing area.
    #[config(
        label = "Minimum V-stab area fraction",
        help = "Minimum allowed vertical-stabiliser area as a fraction of wing area. Typical transports: ~8-14%."
    )]
    pub min_vstab_area_fraction: f64,

    /// Cost of a tail below its area minimum.
    #[config(
        label = "Tail-area-deficit penalty weight",
        help = "Penalty weight for tail area fractions below their minimums (stiff quadratic)."
    )]
    pub tail_area_penalty_scale: f64,

    /// Lower bound on the horizontal tail volume coefficient.
    #[config(
        label = "Minimum H-stab volume coefficient (Vh)",
        help = "Lower bound on the horizontal-tail volume coefficient Vh = Sh*Lh/(S*c_bar), which captures tail effectiveness accounting for its moment arm, not just area (Etkin/Reid convention)."
    )]
    pub min_hstab_volume_coef: f64,

    /// Upper bound on the horizontal tail volume coefficient.
    #[config(
        label = "Maximum H-stab volume coefficient (Vh)",
        help = "Upper bound on Vh -- penalises an oversized tail / an unnecessarily stretched fuselage moment arm."
    )]
    pub max_hstab_volume_coef: f64,

    /// Lower bound on the fin volume coefficient.
    #[config(
        label = "Minimum V-stab volume coefficient (Vv)",
        help = "Lower bound on the vertical-tail volume coefficient Vv = Sv*Lv/(S*b)."
    )]
    pub min_vstab_volume_coef: f64,

    /// Upper bound on the fin volume coefficient.
    #[config(
        label = "Maximum V-stab volume coefficient (Vv)",
        help = "Upper bound on Vv (typical jet-transport max is ~0.12)."
    )]
    pub max_vstab_volume_coef: f64,

    /// Cost of a volume coefficient outside its bounds.
    #[config(
        label = "Tail-volume-coefficient penalty weight",
        help = "Quadratic penalty weight for Vh/Vv falling outside their [min, max] bounds."
    )]
    pub tail_volume_penalty_scale: f64,

    /// How little the wing may taper before the break.
    #[config(
        label = "Max break/root chord ratio",
        help = "Upper bound on break_chord_m / root_chord_m. Real transport wings taper noticeably from root to the yehudi break (typically 0.45-0.65); without this bound the optimizer can inflate the break chord toward the root chord to enlarge MAC (c_ref) 'for free', which cheapens every %MAC-normalised penalty (CG envelope, static-margin target) without a real stability improvement. Soft penalty above this ratio, not a hard bound."
    )]
    pub max_break_root_chord_ratio: f64,

    /// Cost of a wing that tapers too little.
    #[config(
        label = "Break-chord taper-realism penalty weight",
        help = "Penalty weight applied when break_chord_m / root_chord_m exceeds max_break_root_chord_ratio."
    )]
    pub taper_realism_penalty_scale: f64,

    /// Cost of a reflex corner at the wing root trailing edge.
    #[config(
        label = "Wing-root trailing-edge angle penalty weight",
        help = "Penalizes the wing's root-to-break trailing edge (seen in planform) making an angle greater than 90 deg with the fuselage centerline -- i.e. the break station's trailing edge sitting forward of the root's. That creates a reflex (concave) corner at the wing-fuselage junction: a severe stress concentration no real transport-category wing root has, caused by a short root chord combined with a comparatively long break chord and/or too little sweep. Quadratic on the angle exceedance beyond 90 deg."
    )]
    pub te_root_angle_penalty_scale: f64,

    /// How far aft the wing must sit.
    #[config(
        label = "Minimum wing position (fraction of fuselage length)",
        help = "Wing-root leading edge must sit at least this fraction of fuselage length aft of the nose -- prevents the optimizer placing the wing in the cockpit. Typical transports: 25-55%."
    )]
    pub min_wing_position_fraction: f64,

    /// Cost of a wing forward of that position.
    #[config(
        label = "Wing-too-far-forward penalty weight",
        help = "Penalty weight applied when the wing sits forward of min_wing_position_fraction."
    )]
    pub wing_position_penalty_scale: f64,

    /// Cost of not fitting the requested payload.
    #[config(
        label = "Payload-shortfall penalty weight",
        help = "Steep quadratic penalty on the fractional shortfall vs. the target passenger count / cargo payload -- guides the optimizer to grow the fuselage long enough to actually fit the requested payload."
    )]
    pub payload_shortfall_penalty_scale: f64,

    /// How slender the fuselage may get.
    #[config(
        label = "Maximum fuselage fineness ratio",
        help = "Maximum allowed fuselage length/diameter ratio before the too-slender-fuselage penalty kicks in."
    )]
    pub fineness_ratio_max: f64,

    /// Cost of a fuselage more slender than that.
    #[config(
        label = "Slender-fuselage penalty weight",
        help = "Penalty weight applied when the fuselage fineness ratio exceeds fineness_ratio_max."
    )]
    pub fineness_ratio_penalty_scale: f64,

    /// Flat cost of a candidate that could not be evaluated at all.
    #[config(
        label = "Invalid-design cost",
        help = "Cost returned for any candidate design that raises an exception or fails a hard guard during evaluation."
    )]
    pub failure_cost: f64,

    /// Severity of the static-margin floor penalty.
    #[config(
        label = "Instability reject cost",
        help = "Severity scale for the static-margin-floor penalty applied to a candidate that builds and analyses successfully but is rejected as physically invalid (static margin below requirements.min_physical_static_margin). NOT a flat returned cost -- this floor is graduated, not an early return (objective.py's `(deficit * severity)**3` term, where this field sets `severity` so raising/lowering it steepens/relaxes the penalty without editing code; the default reproduces the exact cubic constant used before this field was wired up). Kept distinct from failure_cost so run diagnostics can tell 'geometry/analysis crashed' apart from 'physically unstable' rejects (see OptimizationHistory.reject_reason_counts)."
    )]
    pub instability_failure_cost: f64,
}

impl Default for ObjectiveWeights {
    fn default() -> Self {
        Self {
            ld_weight: 1.0,
            alpha_penalty_scale: 5.0,
            alpha_min_penalty_deg: 0.0,
            alpha_max_penalty_deg: 10.0,
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
            taper_realism_penalty_scale: 250.0,
            te_root_angle_penalty_scale: 100.0,
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
        assert_eq!(kind_of("min_hstab_volume_coef"), Kind::Float);
        assert_eq!(kind_of("cg_envelope_reward"), Kind::Float);
    }

    #[test]
    fn every_min_max_pair_leaves_a_usable_range() {
        let weights = ObjectiveWeights::default();
        assert!(weights.alpha_min_penalty_deg < weights.alpha_max_penalty_deg);
        assert!(weights.min_hstab_volume_coef < weights.max_hstab_volume_coef);
        assert!(weights.min_vstab_volume_coef < weights.max_vstab_volume_coef);
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
