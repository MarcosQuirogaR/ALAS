// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The [`ObjectiveWeights`] field set.

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// Weights and thresholds for the objective function.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct ObjectiveWeights {
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

    /// Flat cost of a candidate that could not be evaluated at all.
    #[config(
        label = "Invalid-design cost",
        help = "Cost returned for any candidate design that raises an exception or fails a hard guard during evaluation."
    )]
    pub failure_cost: f64,
}
