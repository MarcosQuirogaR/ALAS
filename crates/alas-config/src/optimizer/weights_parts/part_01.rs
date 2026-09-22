// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

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

    /// Whether the native transport-planform constraints shape product searches.
    #[serde(default)]
    #[config(
        label = "Enable transport-planform constraints",
        help = "Apply consequence-based transport checks for geometric body angle of attack and usable wing-tank volume. Airliner-shape preferences are controlled separately by Enable transport shape priors. Disable only to reproduce the legacy product objective; this does not alter the built geometry."
    )]
    pub transport_planform_constraints_enabled: bool,

    /// Whether subjective transport-shape preferences contribute to cost.
    #[serde(default)]
    #[config(
        label = "Enable transport shape priors",
        help = "Opt in to heuristic chord-ratio, wingbox-size, flap-footprint, trailing-edge-sweep and bending-slenderness penalties. These are preliminary airliner-shape preferences, not universal physical laws; they are off by default so the optimizer can discover unconventional feasible designs."
    )]
    pub transport_shape_priors_enabled: bool,

    /// Bottom of the physical body-incidence window at trimmed cruise.
    #[serde(default)]
    #[config(
        label = "Geometric body-AoA window: minimum",
        unit = "deg",
        help = "Lower bound for the uncorrected geometric body angle of attack after force and moment trim at the nominated cruise point. This is not the Prandtl-Glauert display alpha or a local wing-section incidence."
    )]
    pub geometric_body_alpha_min_deg: f64,

    /// Top of the physical body-incidence window at trimmed cruise.
    #[serde(default)]
    #[config(
        label = "Geometric body-AoA window: maximum",
        unit = "deg",
        help = "Upper bound for the uncorrected geometric body angle of attack after force and moment trim at the nominated cruise point. A conventional passenger-transport target is commonly specified as a body-attitude requirement around 2-4 deg, not as a universal local-section limit."
    )]
    pub geometric_body_alpha_max_deg: f64,

    /// Cost of leaving the physical body-incidence window.
    #[serde(default)]
    #[config(
        label = "Geometric body-AoA penalty weight",
        help = "Quadratic penalty on trimmed geometric body angle of attack outside the transport body-AoA window. Kept high enough that L/D cannot buy an implausible cruise attitude, while preserving a gradient back toward compliance."
    )]
    pub geometric_body_alpha_penalty_scale: f64,

    /// Minimum usable depth at the inboard root wingbox.
    #[serde(default)]
    #[config(
        label = "Minimum root wingbox depth",
        unit = "m",
        help = "Minimum available structural depth in the root spar box, estimated from the local airfoil thickness between the configured front and rear spars. This is a preliminary packaging guard for carry-through structure and systems, not a stress calculation."
    )]
    pub min_root_wingbox_depth_m: f64,

    /// Minimum usable depth at the kink wingbox.
    #[serde(default)]
    #[config(
        label = "Minimum kink wingbox depth",
        unit = "m",
        help = "Minimum available structural depth at the cranked/kink station, estimated from the local airfoil thickness between the configured spars. It prevents an optimizer from retaining area while collapsing the inboard volume that feeds the outer panel."
    )]
    pub min_break_wingbox_depth_m: f64,

    /// Minimum chordwise width of the kink wingbox.
    #[serde(default)]
    #[config(
        label = "Minimum kink wingbox width",
        unit = "m",
        help = "Minimum front-to-rear spar separation at the kink station. It is a preliminary proxy for main-gear, flap-mechanism and fuel-system packaging; detailed gear retraction geometry remains a later design step."
    )]
    pub min_break_wingbox_width_m: f64,

    /// Cost of violating any inboard wingbox dimensional reserve.
    #[serde(default)]
    #[config(
        label = "Wingbox packaging penalty weight",
        help = "Quadratic penalty for insufficient root/kink wingbox depth or kink spar-box width. The three deficits are normalized by their configured minimums before being summed."
    )]
    pub wingbox_packaging_penalty_scale: f64,

    /// Minimum physical flap area relative to the reference wing area.
    #[serde(default)]
    #[config(
        label = "Minimum flap-area fraction",
        help = "Minimum physical trailing-edge flap area divided by wing planform area, computed from the configured flap chord and span run. It reserves a credible high-lift-device footprint even though the in-loop aerodynamic model is clean-wing."
    )]
    pub min_flap_area_fraction: f64,

    /// Cost of falling below the flap-area reserve.
    #[serde(default)]
    #[config(
        label = "High-lift area penalty weight",
        help = "Quadratic penalty for a configured flap footprint smaller than the transport high-lift reserve."
    )]
    pub flap_area_penalty_scale: f64,

    /// Maximum geometric bending slenderness of the root spar box.
    #[serde(default)]
    #[config(
        label = "Maximum root bending-box slenderness",
        help = "Upper bound on projected-span squared divided by root wingbox width times depth. This is a dimensionless preliminary bending-capacity proxy: it prevents span growth and root-box collapse from appearing structurally free, but does not replace the wingbox structural sizing analysis."
    )]
    pub max_root_bending_box_slenderness: f64,

    /// Cost of exceeding the root bending slenderness guard.
    #[serde(default)]
    #[config(
        label = "Root bending-slenderness penalty weight",
        help = "Quadratic penalty for exceeding the root bending-box slenderness guard."
    )]
    pub bending_slenderness_penalty_scale: f64,

    /// Inboard edge of the tankable semispan interval.
    #[serde(default)]
    #[config(
        label = "Tankable span start",
        unit = "fraction of semi-span",
        help = "Inboard boundary of the wingbox interval credited as fuel volume. The omitted center region represents carry-through structure, fuselage fairing and non-tankable installation volume."
    )]
    pub tankable_span_start_fraction: f64,

    /// Outboard edge of the tankable semispan interval.
    #[serde(default)]
    #[config(
        label = "Tankable span end",
        unit = "fraction of semi-span",
        help = "Outboard boundary of the wingbox interval credited as fuel volume. The omitted outer region reserves space for ailerons, wingtip structure and systems rather than treating every thin outboard bay as fuel tank."
    )]
    pub tankable_span_end_fraction: f64,

    /// Linear cost per metre of span, standing in for structural weight.
    #[config(
        label = "Wingspan penalty (per metre)",
        help = "Small linear penalty per metre of span: discourages excessively large wings."
    )]
    pub span_penalty_per_m: f64,

    /// Cost of parasite drag.
    #[config(
        label = "Parasite-drag (CD0) penalty weight",
        help = "Rewards lower parasite drag (CD0); a thinner/cleaner shape reduces this term's cost. Typical cruise CD0 is 0.016-0.020."
    )]
    pub cd0_penalty_scale: f64,

    /// Cost of exceeding the wing-area bound while retaining a search gradient.
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
        help = "NOT read by the cost function (objective.py): kept only so old saved YAML configs referencing this key still load without error. Originally intended to penalize (Delta x_cg / MAC)^2, but that formula was never actually wired up; the field's real runtime effect was fully redundant with static_margin_penalty_scale, since both applied to the identical static-margin-vs-target term, so the two are consolidated into that single, correctly-named, appropriately-soft term. Physical CG-envelope compliance is enforced separately by cg_envelope_penalty_scale/cg_envelope_reward below."
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

    /// Retained serialized field from the former MTOW-closure tank penalty.
    #[config(
        label = "Legacy fuel-volume penalty weight (unused)",
        help = "Deprecated compatibility field. MTOW minus zero-fuel mass is a mass allowance, not mission-required fuel, so it is no longer used by the optimizer. Tank capacity will be constrained against mission fuel plus the selected reserve policy."
    )]
    pub fuel_volume_penalty_scale: f64,

    /// Pull toward the target static margin.
    #[config(
        label = "Static-margin target penalty weight",
        help = "SOFT preference nudging compliant-but-suboptimal candidates toward requirements.target_static_margin, NOT a hard requirement (the physical floor, min_physical_static_margin, and the CG-envelope itself are enforced separately and are what actually keep a design safe/legal). Kept deliberately small relative to -L/D (typically 15-25) so this doesn't crowd out genuine aerodynamic improvements: raising it much above ~20-30 risks the optimizer chasing an exact SM match instead of exploring shape space, overwhelming the L/D signal the search is meant to prioritize."
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
        help = "Upper bound on Vh: penalises an oversized tail / an unnecessarily stretched fuselage moment arm."
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

    /// How much the inboard panel must retain chord at the kink.
    #[serde(default)]
    #[config(
        label = "Min break/root chord ratio",
        help = "Lower bound on break_chord_m / root_chord_m. A transport kink needs retained inboard chord for a deep wing box, fuel, high-lift devices and pylon/gear installation; below this ratio the inboard panel becomes an implausibly abrupt taper."
    )]
    pub min_break_root_chord_ratio: f64,

    /// How much chord the tip must retain relative to the root.
    #[serde(default)]
    #[config(
        label = "Min tip/root chord ratio",
        help = "Lower bound on tip_chord_m / root_chord_m. It prevents a mathematically efficient but impractically pinched tip, retaining volume for tip structure, ailerons and a manufacturable trailing edge."
    )]
    pub min_tip_root_chord_ratio: f64,

    /// Cost of a wing that tapers too little.
    #[config(
        label = "Break-chord taper-realism penalty weight",
        help = "Penalty weight applied when break_chord_m / root_chord_m exceeds max_break_root_chord_ratio."
    )]
    pub taper_realism_penalty_scale: f64,

    /// Cost of a reflex corner at the wing root trailing edge.
    #[config(
        label = "Wing-root trailing-edge angle penalty weight",
        help = "Penalizes the wing's root-to-break trailing edge (seen in planform) making an angle greater than 90 deg with the fuselage centerline: i.e. the break station's trailing edge sitting forward of the root's. That creates a reflex (concave) corner at the wing-fuselage junction: a severe stress concentration no real transport-category wing root has, caused by a short root chord combined with a comparatively long break chord and/or too little sweep. Quadratic on the angle exceedance beyond 90 deg."
    )]
    pub te_root_angle_penalty_scale: f64,

    /// Most forward-swept admissible inboard trailing edge.
    #[serde(default)]
    #[config(
        label = "Minimum inboard trailing-edge sweep",
        unit = "deg",
        help = "Lowest admissible exposed side-of-body-to-kink trailing-edge sweep. Zero prevents the edge from running forward outboard, which would make its angle with the fuselage exceed 90 degrees and create a reflex corner."
    )]
    pub min_inboard_te_sweep_deg: f64,

    /// Most aft-swept admissible inboard trailing edge.
    #[serde(default)]
    #[config(
        label = "Maximum inboard trailing-edge sweep",
        unit = "deg",
        help = "Largest admissible root-to-kink trailing-edge sweep. Transport trailing edges are normally appreciably less swept than leading edges because the inboard chord is retained for structure and high-lift devices."
    )]
    pub max_inboard_te_sweep_deg: f64,

    /// How far aft the wing must sit.
    #[config(
        label = "Minimum wing position (fraction of fuselage length)",
        help = "Wing-root leading edge must sit at least this fraction of fuselage length aft of the nose: prevents the optimizer placing the wing in the cockpit. Typical transports: 25-55%."
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
        help = "Steep quadratic penalty on the fractional shortfall vs. the target passenger count / cargo payload: guides the optimizer to grow the fuselage long enough to actually fit the requested payload."
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
        help = "Severity scale for the static-margin-floor penalty applied to a candidate that builds and analyses successfully but is rejected as physically invalid (static margin below requirements.min_physical_static_margin). NOT a flat returned cost; this floor is graduated, not an early return (objective.py's `(deficit * severity)**3` term, where this field sets `severity` so raising/lowering it steepens/relaxes the penalty without editing code; the default reproduces the exact cubic constant used before this field was wired up). Kept distinct from failure_cost so run diagnostics can tell 'geometry/analysis crashed' apart from 'physically unstable' rejects (see OptimizationHistory.reject_reason_counts)."
    )]
    pub instability_failure_cost: f64,
}
