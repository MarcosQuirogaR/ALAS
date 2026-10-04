// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The shared physical CG-limits helper.
//!
//! The forward and aft CG boundaries are derived from independent physical
//! mechanisms: three aft boundaries and two forward boundaries, each with
//! its own cause. A fixed offset from the aft limit would not be a physical
//! boundary, since it would cross the ground limit as soon as the critical
//! neutral point moved forward. The configured CG range is checked as a
//! *minimum usable range*, not applied as a limit.

use alas_config::MacFrame;

use super::rotation::{longitudinal_force_moment_m, rotation_cg_offset_m};
use super::PhaseLimits;

/// Which physical mechanism governs the aft CG boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AftLimitGovernance {
    /// Static-margin floor from the critical (most-forward-moving) neutral
    /// point condition, not the clean one.
    Aerodynamic,
    /// Minimum nose-gear load fraction for steering authority.
    GroundMinimumNoseLoad,
    /// Longitudinal tip-back / tail-scrape geometry.
    TipBack,
}

/// Which physical mechanism governs the forward CG boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForwardLimitGovernance {
    /// Maximum nose-gear load fraction for handling/steering.
    MaxNoseLoadHandling,
    /// Scissor-plot (trim/controllability) estimate at landing `CL_max`.
    /// A diagnostic-only variant: [`physical_cg_limits`] never selects it as
    /// the governing mechanism, which comes from the physically derived
    /// [`Self::RotationNoseWheelLiftoff`] and
    /// [`Self::LandingTrimGroundEffect`] criteria; see
    /// `scissor_plot_fwd_pct_mac` on [`PhysicalCgLimits`].
    ScissorPlotEstimate,
    /// Nose-wheel lift-off (rotation) moment-balance criterion at `V_R`
    /// (Sadraey, *Aircraft Design: A Systems Engineering Approach*, 2012,
    /// sec. 9.6.2 and 12.6): the most-forward CG at which the tail download
    /// at full up-elevator can still supply the wing-body moment, the
    /// required pitch acceleration of the inertia about the main-gear
    /// contact, and the weight moment about that contact.
    RotationNoseWheelLiftoff,
    /// Landing trim/flare criterion (Torenbeek/Roskam scissor
    /// construction with a ground-effect knockdown on tail authority): the
    /// most-forward CG at which the tail can still trim `CL_max,landing`
    /// in ground effect.
    LandingTrimGroundEffect,
}

/// Every input [`physical_cg_limits`] needs, all in the primary aircraft
/// body frame (x aft of the nose tip, z up, metres) unless noted.
#[derive(Debug, Clone, Copy)]
pub struct PhysicalCgLimitsInput {
    /// The frame every `%MAC` figure below is stated against.
    pub mac_frame: MacFrame,
    /// The critical (most-forward) neutral-point station across every
    /// evaluated condition (`alas_stab::neutral_point::NeutralPointConditions::critical`).
    pub critical_np_x_m: f64,
    /// The clean, single-condition neutral point: reported as a diagnostic
    /// only, never as a limit.
    pub clean_np_x_m: f64,
    /// Hard longitudinal-stability floor, fraction of MAC.
    pub min_physical_static_margin: f64,
    /// Effective main-gear longitudinal station.
    pub x_main_gear_m: f64,
    /// Most-aft main-gear axle station (the tip-back pivot).
    pub x_mlg_aft_axle_m: f64,
    /// `x_main_gear_m - x_nose_gear_m`.
    pub wheelbase_m: f64,
    /// This state's loaded centre-of-gravity height above the shared ground
    /// plane, metres; non-finite or non-positive disables the
    /// tip-back boundary (reported as not finite, never as zero).
    pub h_cg_m: f64,
    /// Configured minimum tip-back angle, degrees.
    pub min_tip_back_deg: f64,
    /// Tail-scrape angle at the main gear, degrees, when the fuselage
    /// lower-contour geometry could place one.
    pub scrape_angle_deg: Option<f64>,
    /// Minimum nose-load fraction of this state's weight: the published
    /// aft-CG nose share at this weight when the preset has one, never below
    /// the steering minimum (`alas_config::PublishedAftCgNoseLoad`).
    pub pct_load_nlg_min: f64,
    /// Maximum nose-load fraction of current weight for handling.
    pub pct_load_nlg_max_handling: f64,
    /// Wing-body aerodynamic centre, as a fraction of MAC aft of LEMAC
    /// (`x_ac,wb / c`); the quarter-chord point (`0.25`) absent a
    /// fuselage-corrected wing-body AC of its own.
    pub x_ac_wb_frac: f64,
    /// Landing-configuration wing-body pitching moment about its own AC,
    /// dimensionless; an *estimate* of the flap increment (Torenbeek order
    /// of magnitude for a single-slotted flap, negative/nose-down), not a
    /// measured value.
    pub cm_ac_wb_landing: f64,
    /// Landing maximum lift coefficient (`config.performance.cl_max_land`).
    pub cl_max_landing: f64,
    /// Tail dynamic-pressure efficiency, dimensionless; a declared estimate
    /// (Torenbeek order-of-magnitude for a low tail, `0.9`), not measured.
    pub eta: f64,
    /// Horizontal-tail volume coefficient `S_h * l_h / (S * c)`.
    pub tail_volume_coefficient: f64,
    /// Maximum (most negative) tail lift coefficient available to trim the
    /// landing nose-down moment; an adjustable-stabiliser estimate (Torenbeek),
    /// not a measured value. Negative (downward tail lift).
    pub cl_h_max: f64,
    /// Configured CG range, `%MAC`, now interpreted as the *minimum usable*
    /// range the physical boundaries must admit (see [`PhysicalCgLimits::usable_range_pct_mac`]),
    /// never as a forward limit.
    pub cg_range_pct_mac: f64,
    /// Horizontal tail aerodynamic-centre station, primary body frame, m
    /// aft of the nose tip; `f64::NAN` disables the rotation and
    /// landing-trim mechanisms without poisoning the others (no second/tail
    /// wing).
    pub x_h_ac_m: f64,
    /// Horizontal-tail reference area over the wing reference area
    /// (`S_h / S`), `0.0` with no tail.
    pub tail_area_ratio: f64,
    /// Wieselsberger-style ground-effect knockdown on the tail's maximum
    /// download authority for the landing-trim criterion, dimensionless in
    /// `(0, 1]`; `1.0` = no correction
    /// (`alas_opt::envelope::support::tail_ground_effect_factor`). The
    /// rotation criterion does not use it: its ground effect acts through
    /// the wing downwash in [`Self::rotation_tail_lift_coefficient`].
    pub tail_ground_effect_factor: f64,
    /// Horizontal-tail lift coefficient at rotation with full up-elevator,
    /// on the tail's own area, negative (download), derived from the tail
    /// geometry, the downwash in ground effect and the elevator
    /// (`rotation::rotation_tail_lift`); `NaN` disables the rotation
    /// mechanism.
    pub rotation_tail_lift_coefficient: f64,
    /// Wing-body lift coefficient at the ground (pre-rotation) attitude at
    /// `V_R`, dimensionless; a declared estimate
    /// (`config.landing_gear.cl_ground_attitude_frac_of_cl_max_to * CL_max,TO`,
    /// Torenbeek order of magnitude), not a measured value.
    pub cl_ground_attitude: f64,
    /// The "apparent" lift coefficient implied by weight support alone at
    /// `V_R` (`CL_max,TO / (V_R/V_S1,TO)^2`), dimensionless: the rotation
    /// criterion's moment balance is nondimensionalized by `W = q_R S
    /// CL_R`, which is what lets it stay free of an explicit mass, dynamic
    /// pressure, or wing-area input.
    pub cl_r_rotation: f64,
    /// Wing-body pitching moment about its own AC in the takeoff
    /// configuration, dimensionless; a declared Torenbeek order-of-magnitude
    /// estimate (`alas_opt::envelope::support::ROTATION_CM_AC_WB_TAKEOFF`),
    /// not a measured value.
    pub cm_ac_wb_takeoff: f64,
    /// Pitch radius of gyration about the centre of gravity, m: from the
    /// mass ledger's takeoff `I_yy`, else Raymer's jet-transport radius
    /// (`rotation::pitch_radius_of_gyration_m`). The rotation balance adds
    /// the transfer to the main-gear contact itself.
    pub pitch_radius_of_gyration_m: f64,
    /// Required pitch angular acceleration at rotation, deg/s^2: 7 unless
    /// the configuration overrides it
    /// (`rotation::rotation_pitch_acceleration_deg_s2`).
    pub rotation_angular_accel_deg_s2: f64,
    /// Standard gravity, m/s^2 (`config.requirements.gravity_m_s2`): turns
    /// the pitch-inertia moment into the same weight-normalized length as
    /// every other rotation-criterion term.
    pub gravity_m_s2: f64,
    /// All-engine takeoff/go-around thrust at `V_R` over this state's
    /// weight, dimensionless; non-finite means not evaluated, and the
    /// rotation criterion then takes no thrust credit (friction still
    /// applies).
    pub rotation_thrust_to_weight: f64,
    /// Thrust-line height above the shared ground plane, m; non-finite
    /// means not evaluated (no thrust credit).
    pub rotation_thrust_line_height_m: f64,
    /// Tire rolling-friction coefficient during the takeoff roll,
    /// dimensionless (`config.landing_gear.rotation_rolling_friction_coefficient`).
    pub rotation_rolling_friction_coefficient: f64,
}

/// The physical aft/forward CG boundaries this state's weight and geometry
/// admit, in percent MAC, and which mechanism governs each.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhysicalCgLimits {
    /// The more forward of the three aft boundaries: the governing one.
    pub aft_limit_pct_mac: f64,
    /// Which mechanism produced [`Self::aft_limit_pct_mac`].
    pub aft_limit_governance: AftLimitGovernance,
    /// Diagnostic only: the clean-condition neutral point, `%MAC`.
    pub clean_np_pct_mac: f64,
    /// Diagnostic only: the aerodynamic aft boundary from the critical
    /// neutral point, whether or not it is [`Self::aft_limit_pct_mac`].
    pub aerodynamic_aft_pct_mac: f64,
    /// Diagnostic only: the ground minimum-nose-load aft boundary.
    pub ground_aft_pct_mac: f64,
    /// Diagnostic only: the tip-back aft boundary at this state's `h_cg`.
    pub tip_back_aft_pct_mac: f64,
    /// The more aft of the two forward boundaries: the governing one.
    pub fwd_limit_pct_mac: f64,
    /// Which mechanism produced [`Self::fwd_limit_pct_mac`].
    pub fwd_limit_governance: ForwardLimitGovernance,
    /// Diagnostic only: the maximum-nose-load-fraction forward boundary.
    pub max_nose_load_fwd_pct_mac: f64,
    /// Diagnostic only: the scissor-plot controllability estimate, reported
    /// but never a governance candidate.
    pub scissor_plot_fwd_pct_mac: f64,
    /// Diagnostic and governance candidate: the rotation (nose-wheel
    /// liftoff) moment-balance boundary.
    pub rotation_fwd_pct_mac: f64,
    /// Diagnostic only: the thrust and rolling-friction moment about the
    /// main-gear contact over `W c`, signed so that negative is nose-up,
    /// `%MAC`. The rotation boundary is nonlinear in its moments (the pitch
    /// inertia about the contact grows with the CG offset), so this is the
    /// term's size, not an exact shift of [`Self::rotation_fwd_pct_mac`].
    pub rotation_longitudinal_force_shift_pct_mac: f64,
    /// Diagnostic only: the tail lift coefficient at rotation the boundary
    /// used ([`PhysicalCgLimitsInput::rotation_tail_lift_coefficient`]).
    pub rotation_tail_lift_coefficient: f64,
    /// Diagnostic and governance candidate: the landing trim/flare boundary
    /// in ground effect.
    pub landing_trim_fwd_pct_mac: f64,
    /// `aft_limit_pct_mac - fwd_limit_pct_mac`: the usable CG range these
    /// physical boundaries admit at this state.
    pub usable_range_pct_mac: f64,
}

fn pct_mac(frame: MacFrame, x_m: f64) -> f64 {
    frame.pct_mac(x_m).unwrap_or(f64::NAN)
}

/// Every mechanism, the scope of the unscoped [`physical_cg_limits`] result.
const ALL_PHASES: PhaseLimits = PhaseLimits {
    rotation: true,
    landing_trim: true,
    static_margin: true,
};

impl PhysicalCgLimits {
    /// The governing forward boundary among the mechanisms `phase` admits:
    /// the maximum nose load always, rotation and landing trim when scoped
    /// in. `NaN` candidates are skipped.
    #[must_use]
    pub fn fwd_for(&self, phase: PhaseLimits) -> (f64, ForwardLimitGovernance) {
        most_aft([
            (
                self.max_nose_load_fwd_pct_mac,
                ForwardLimitGovernance::MaxNoseLoadHandling,
            ),
            (
                if phase.rotation {
                    self.rotation_fwd_pct_mac
                } else {
                    f64::NAN
                },
                ForwardLimitGovernance::RotationNoseWheelLiftoff,
            ),
            (
                if phase.landing_trim {
                    self.landing_trim_fwd_pct_mac
                } else {
                    f64::NAN
                },
                ForwardLimitGovernance::LandingTrimGroundEffect,
            ),
        ])
    }

    /// The governing aft boundary among the mechanisms `phase` admits: the
    /// minimum nose load and tip-back always, the static-margin floor when
    /// scoped in. `NaN` candidates are skipped.
    #[must_use]
    pub fn aft_for(&self, phase: PhaseLimits) -> (f64, AftLimitGovernance) {
        most_forward([
            (
                if phase.static_margin {
                    self.aerodynamic_aft_pct_mac
                } else {
                    f64::NAN
                },
                AftLimitGovernance::Aerodynamic,
            ),
            (
                self.ground_aft_pct_mac,
                AftLimitGovernance::GroundMinimumNoseLoad,
            ),
            (self.tip_back_aft_pct_mac, AftLimitGovernance::TipBack),
        ])
    }

    /// These limits with the governing boundaries, their governance and the
    /// usable range restricted to the mechanisms `phase` admits. Every
    /// mechanism's own diagnostic boundary is kept unchanged.
    #[must_use]
    pub fn scoped(&self, phase: PhaseLimits) -> Self {
        let (fwd_limit_pct_mac, fwd_limit_governance) = self.fwd_for(phase);
        let (aft_limit_pct_mac, aft_limit_governance) = self.aft_for(phase);
        Self {
            fwd_limit_pct_mac,
            fwd_limit_governance,
            aft_limit_pct_mac,
            aft_limit_governance,
            usable_range_pct_mac: aft_limit_pct_mac - fwd_limit_pct_mac,
            ..*self
        }
    }
}

fn most_forward(candidates: [(f64, AftLimitGovernance); 3]) -> (f64, AftLimitGovernance) {
    candidates
        .into_iter()
        .filter(|(value, _)| value.is_finite())
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .unwrap_or((f64::NAN, AftLimitGovernance::Aerodynamic))
}

fn most_aft(candidates: [(f64, ForwardLimitGovernance); 3]) -> (f64, ForwardLimitGovernance) {
    candidates
        .into_iter()
        .filter(|(value, _)| value.is_finite())
        .max_by(|left, right| left.0.total_cmp(&right.0))
        .unwrap_or((f64::NAN, ForwardLimitGovernance::MaxNoseLoadHandling))
}

/// The physical aft and forward CG boundaries for one loading state.
///
/// # Aft boundary
///
/// The more forward (most restrictive) of:
/// - **Aerodynamic**: `critical_np - min_physical_static_margin`.
/// - **Ground minimum nose load**: `x_main_gear - pct_load_nlg_min * wheelbase`.
/// - **Tip-back**: `x_mlg_aft_axle - h_cg * tan(max(min_tip_back_deg, scrape_angle_deg))`.
///
/// # Forward boundary
///
/// The more aft (most restrictive) of:
/// - **Maximum nose-load handling**: `x_main_gear - pct_load_nlg_max_handling * wheelbase`.
/// - **Rotation (nose-wheel lift-off)** (Sadraey 2012, sec. 9.6.2 and
///   12.6): the moment balance about the main-gear contact at `V_R`,
///   divided by the weight `W = q_R S CL_R` (see
///   [`PhysicalCgLimitsInput::cl_r_rotation`]), solved exactly for the most
///   forward CG at which the derived tail lift at full up-elevator, the
///   wing-body lift and moment, thrust and rolling friction give the
///   required pitch acceleration of the pitch inertia transferred to the
///   contact point, `I_P = I_cg + m [(x_P - x_cg)^2 + h_cg^2]`. The balance
///   is quadratic in the CG station; see `rotation::rotation_cg_offset_m`.
///   Drag is omitted (about 1 %MAC; see `super::rotation`). The takeoff
///   `Cm_ac,wb = -0.15` and `x_ac,wb = 0.25` remain declared estimates.
/// - **Landing trim in ground effect**: the same scissor-plot construction
///   as the diagnostic-only estimate below, with a ground-effect knockdown
///   on the tail's available download coefficient.
///
/// The scissor-plot estimate itself
/// (`x_cg,fwd/c = x_ac,wb/c - Cm_ac,wb,landing/CL_max,landing + eta*(S_h l_h/(S c))*CL_h,max/CL_max,landing`,
/// a Torenbeek/Roskam controllability construction, an estimate not a
/// measured trim/rotation solve) is reported only as a diagnostic
/// ([`PhysicalCgLimits::scissor_plot_fwd_pct_mac`]); it is not a
/// governance candidate.
#[must_use]
pub fn physical_cg_limits(input: &PhysicalCgLimitsInput) -> PhysicalCgLimits {
    let frame = input.mac_frame;
    let clean_np_pct_mac = pct_mac(frame, input.clean_np_x_m);
    let aerodynamic_aft_pct_mac =
        pct_mac(frame, input.critical_np_x_m) - 100.0 * input.min_physical_static_margin;
    let ground_aft_x_m = input.x_main_gear_m - input.pct_load_nlg_min * input.wheelbase_m;
    let ground_aft_pct_mac = pct_mac(frame, ground_aft_x_m);
    let required_tip_back_deg = input.min_tip_back_deg.max(
        input
            .scrape_angle_deg
            .filter(|angle| angle.is_finite())
            .unwrap_or(0.0),
    );
    let tip_back_aft_x_m = if input.h_cg_m.is_finite() && input.h_cg_m > 0.0 {
        input.x_mlg_aft_axle_m - input.h_cg_m * required_tip_back_deg.to_radians().tan()
    } else {
        f64::NAN
    };
    let tip_back_aft_pct_mac = pct_mac(frame, tip_back_aft_x_m);

    let max_nose_load_fwd_x_m =
        input.x_main_gear_m - input.pct_load_nlg_max_handling * input.wheelbase_m;
    let max_nose_load_fwd_pct_mac = pct_mac(frame, max_nose_load_fwd_x_m);
    let cl_max = input.cl_max_landing.max(1.0e-6);
    // Diagnostic-only scissor-plot estimate: no ground-effect knockdown,
    // never a governance candidate.
    let scissor_plot_fwd_frac = input.x_ac_wb_frac - input.cm_ac_wb_landing / cl_max
        + input.eta * input.tail_volume_coefficient * input.cl_h_max / cl_max;
    let scissor_plot_fwd_pct_mac = scissor_plot_fwd_frac * 100.0;

    // Landing trim in ground effect: the same construction, with the tail's
    // download coefficient knocked down by the Wieselsberger-style ground
    // effect factor.
    let landing_trim_fwd_frac = input.x_ac_wb_frac - input.cm_ac_wb_landing / cl_max
        + input.eta
            * input.tail_ground_effect_factor
            * input.tail_volume_coefficient
            * input.cl_h_max
            / cl_max;
    let landing_trim_fwd_pct_mac = landing_trim_fwd_frac * 100.0;

    // Rotation (nose-wheel lift-off): the moment balance about the main-gear
    // contact at V_R (Sadraey 2012, eqs. 9.36-9.54a; see `super::rotation`),
    // every moment divided by the weight W = q_R S CL_R, so a moment over W
    // is a length in metres and needs no explicit mass, dynamic pressure or
    // wing area. The tail lift is the derived one at full up-elevator.
    let mac_m = frame.chord_m;
    let x_mg_m = input.x_main_gear_m;
    let cl_r = input.cl_r_rotation.max(1.0e-6);
    let x_ac_wb_m = frame.leading_edge_x_m + input.x_ac_wb_frac * mac_m;
    let wing_lift_m = (input.cl_ground_attitude / cl_r) * (x_mg_m - x_ac_wb_m);
    let pitching_moment_m = input.cm_ac_wb_takeoff / cl_r * mac_m;
    // Tail lift over the wing-referenced lift coefficient: negative
    // (download) at a station aft of the contact, so a nose-up moment.
    let tail_lift_coefficient =
        input.rotation_tail_lift_coefficient * input.eta * input.tail_area_ratio;
    let tail_lift_m = (tail_lift_coefficient / cl_r) * (x_mg_m - input.x_h_ac_m);
    // The lift that unloads the wheels is the same wing and tail lift.
    let lift_over_weight = (input.cl_ground_attitude + tail_lift_coefficient) / cl_r;
    let longitudinal_force_m = longitudinal_force_moment_m(
        input.h_cg_m,
        input.rotation_thrust_to_weight,
        input.rotation_thrust_line_height_m,
        input.rotation_rolling_friction_coefficient,
        lift_over_weight,
    );
    let available_m = wing_lift_m + pitching_moment_m + tail_lift_m + longitudinal_force_m;
    let cg_offset_m = rotation_cg_offset_m(
        available_m,
        input.pitch_radius_of_gyration_m,
        input.h_cg_m,
        input.rotation_angular_accel_deg_s2,
        input.gravity_m_s2.max(1.0e-6),
    );
    let rotation_fwd_pct_mac = pct_mac(frame, x_mg_m - cg_offset_m);

    let limits = PhysicalCgLimits {
        aft_limit_pct_mac: f64::NAN,
        aft_limit_governance: AftLimitGovernance::Aerodynamic,
        clean_np_pct_mac,
        aerodynamic_aft_pct_mac,
        ground_aft_pct_mac,
        tip_back_aft_pct_mac,
        fwd_limit_pct_mac: f64::NAN,
        fwd_limit_governance: ForwardLimitGovernance::MaxNoseLoadHandling,
        max_nose_load_fwd_pct_mac,
        scissor_plot_fwd_pct_mac,
        rotation_fwd_pct_mac,
        rotation_longitudinal_force_shift_pct_mac: -100.0 * longitudinal_force_m / mac_m,
        rotation_tail_lift_coefficient: input.rotation_tail_lift_coefficient,
        landing_trim_fwd_pct_mac,
        usable_range_pct_mac: f64::NAN,
    };
    limits.scoped(ALL_PHASES)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_input() -> PhysicalCgLimitsInput {
        PhysicalCgLimitsInput {
            mac_frame: MacFrame::new(10.0, 4.0).expect("valid frame"),
            critical_np_x_m: 12.0,
            clean_np_x_m: 12.4,
            min_physical_static_margin: 0.05,
            x_main_gear_m: 17.0,
            x_mlg_aft_axle_m: 17.5,
            wheelbase_m: 12.0,
            h_cg_m: 3.0,
            min_tip_back_deg: 15.0,
            scrape_angle_deg: Some(10.0),
            pct_load_nlg_min: 0.06,
            pct_load_nlg_max_handling: 0.20,
            x_ac_wb_frac: 0.25,
            cm_ac_wb_landing: -0.20,
            cl_max_landing: 2.6,
            eta: 0.9,
            tail_volume_coefficient: 1.0,
            cl_h_max: -0.8,
            cg_range_pct_mac: 30.0,
            x_h_ac_m: 30.0,
            tail_area_ratio: 0.25,
            tail_ground_effect_factor: 0.9,
            rotation_tail_lift_coefficient: -1.3,
            cl_ground_attitude: 0.6,
            cl_r_rotation: 1.3,
            cm_ac_wb_takeoff: -0.15,
            pitch_radius_of_gyration_m: 6.0,
            rotation_angular_accel_deg_s2: 5.0,
            gravity_m_s2: 9.806_65,
            rotation_thrust_to_weight: 0.30,
            rotation_thrust_line_height_m: 1.8,
            rotation_rolling_friction_coefficient: 0.02,
        }
    }

    /// Hand-computed thrust and rolling-friction term against
    /// [`base_input`]: h_cg 3.0 m, T/W 0.30 at h_T 1.8 m, mu 0.02, c 4.0 m.
    #[test]
    fn the_thrust_and_friction_term_matches_a_hand_computed_moment_balance() {
        let with_term = physical_cg_limits(&base_input());
        let mut input = base_input();
        input.rotation_thrust_to_weight = f64::NAN;
        input.rotation_rolling_friction_coefficient = 0.0;
        let without = physical_cg_limits(&input);

        // L/W = (CL_g + CL_h eta S_h/S) / CL_R
        //     = (0.6 + (-1.3)(0.9)(0.25)) / 1.3 = 0.3075 / 1.3 = 0.236538.
        let lift_over_weight: f64 = (0.6 - 1.3 * 0.9 * 0.25) / 1.3;
        // Thrust below the CG: 0.30 * (3.0 - 1.8) = 0.36 m, nose-up.
        // Friction: 0.02 * (1 - 0.236538) * 3.0 = 0.045808 m, nose-down.
        // Net 0.314192 m over c = 4.0 m: -7.8548 %MAC.
        let moment_m = 0.30 * (3.0 - 1.8) - 0.02 * (1.0 - lift_over_weight) * 3.0;
        let expected_shift_pct = -100.0 * moment_m / 4.0;
        assert!(
            (with_term.rotation_longitudinal_force_shift_pct_mac - expected_shift_pct).abs()
                < 1.0e-9
        );
        assert_eq!(without.rotation_longitudinal_force_shift_pct_mac, 0.0);
        // The boundary with the term still closes the full balance about the
        // contact: the moment enters the available moment before the
        // inertia is solved, not as an additive shift of the result.
        assert!(
            rotation_balance_residual_m(&base_input(), with_term.rotation_fwd_pct_mac, moment_m)
                .abs()
                < 1.0e-9
        );
        // The thrust line below the CG outweighs the friction here, so the
        // rotation boundary moves forward.
        assert!(with_term.rotation_fwd_pct_mac < without.rotation_fwd_pct_mac);

        // A thrust line above the CG (tail engines) is nose-down and moves
        // the boundary aft.
        let mut high = base_input();
        high.rotation_thrust_line_height_m = 4.0;
        assert!(physical_cg_limits(&high).rotation_fwd_pct_mac > without.rotation_fwd_pct_mac);

        // Friction alone (no thrust credit) is always nose-down.
        let mut friction_only = base_input();
        friction_only.rotation_thrust_to_weight = f64::NAN;
        let friction_limits = physical_cg_limits(&friction_only);
        assert!(friction_limits.rotation_longitudinal_force_shift_pct_mac > 0.0);

        // An unknown CG height disables the whole term, never NaN-poisons it.
        let mut no_height = base_input();
        no_height.h_cg_m = f64::NAN;
        let limits = physical_cg_limits(&no_height);
        assert_eq!(limits.rotation_longitudinal_force_shift_pct_mac, 0.0);
        assert!(limits.rotation_fwd_pct_mac.is_finite());
    }

    #[test]
    fn scoping_restricts_each_boundary_to_the_admitted_mechanisms() {
        let mut input = base_input();
        // Make every forward mechanism distinct and the rotation one govern.
        input.rotation_thrust_to_weight = f64::NAN;
        let all = physical_cg_limits(&input);
        let ground = all.scoped(PhaseLimits::GROUND);
        assert_eq!(ground.fwd_limit_pct_mac, all.max_nose_load_fwd_pct_mac);
        assert_eq!(
            ground.fwd_limit_governance,
            ForwardLimitGovernance::MaxNoseLoadHandling
        );
        assert!(
            ground.aft_limit_pct_mac == all.ground_aft_pct_mac
                || ground.aft_limit_pct_mac == all.tip_back_aft_pct_mac
        );
        let takeoff = all.scoped(PhaseLimits::TAKEOFF);
        assert_eq!(
            takeoff.fwd_limit_pct_mac,
            all.max_nose_load_fwd_pct_mac.max(all.rotation_fwd_pct_mac)
        );
        let flight = all.scoped(PhaseLimits::FLIGHT);
        assert_eq!(
            flight.fwd_limit_pct_mac,
            all.max_nose_load_fwd_pct_mac
                .max(all.landing_trim_fwd_pct_mac)
        );
        for scoped in [ground, takeoff, flight, all.scoped(PhaseLimits::LANDING)] {
            assert!(scoped.fwd_limit_pct_mac <= all.fwd_limit_pct_mac);
            assert!(scoped.aft_limit_pct_mac >= all.aft_limit_pct_mac);
            assert_eq!(
                scoped.usable_range_pct_mac,
                scoped.aft_limit_pct_mac - scoped.fwd_limit_pct_mac
            );
            assert_eq!(scoped.rotation_fwd_pct_mac, all.rotation_fwd_pct_mac);
        }
        // The static-margin floor is the only mechanism that can make the
        // flight aft boundary tighter than the ground one.
        input.critical_np_x_m = 10.5;
        let aero = physical_cg_limits(&input);
        assert_eq!(
            aero.scoped(PhaseLimits::FLIGHT).aft_limit_governance,
            AftLimitGovernance::Aerodynamic
        );
        assert_eq!(
            aero.scoped(PhaseLimits::LANDING).aft_limit_governance,
            AftLimitGovernance::Aerodynamic
        );
        assert_eq!(
            aero.scoped(PhaseLimits::TAKEOFF),
            aero.scoped(ALL_PHASES).scoped(PhaseLimits::TAKEOFF)
        );
    }

    #[test]
    fn the_aft_limit_is_the_most_forward_of_the_three_candidates() {
        let limits = physical_cg_limits(&base_input());
        assert!(
            (limits.aft_limit_pct_mac - limits.aerodynamic_aft_pct_mac).abs() < 1e-9
                || (limits.aft_limit_pct_mac - limits.ground_aft_pct_mac).abs() < 1e-9
                || (limits.aft_limit_pct_mac - limits.tip_back_aft_pct_mac).abs() < 1e-9
        );
        assert!(limits.aft_limit_pct_mac <= limits.aerodynamic_aft_pct_mac + 1e-9);
        assert!(limits.aft_limit_pct_mac <= limits.ground_aft_pct_mac + 1e-9);
        assert!(limits.aft_limit_pct_mac <= limits.tip_back_aft_pct_mac + 1e-9);
    }

    #[test]
    fn the_forward_limit_is_the_most_aft_of_the_three_candidates() {
        let limits = physical_cg_limits(&base_input());
        assert!(limits.fwd_limit_pct_mac >= limits.max_nose_load_fwd_pct_mac - 1e-9);
        assert!(limits.fwd_limit_pct_mac >= limits.rotation_fwd_pct_mac - 1e-9);
        assert!(limits.fwd_limit_pct_mac >= limits.landing_trim_fwd_pct_mac - 1e-9);
        // The scissor-plot estimate is diagnostic only and not a governance
        // candidate, so the forward limit need not dominate it.
        assert!(
            (limits.fwd_limit_pct_mac - limits.max_nose_load_fwd_pct_mac).abs() < 1e-9
                || (limits.fwd_limit_pct_mac - limits.rotation_fwd_pct_mac).abs() < 1e-9
                || (limits.fwd_limit_pct_mac - limits.landing_trim_fwd_pct_mac).abs() < 1e-9
        );
    }

    /// The dimensional moment balance about the main-gear contact at a CG of
    /// `cg_pct_mac`, Sadraey eq. 9.36 with the inertia of eq. 9.53, written
    /// out independently of the solver for a 1000 kg aircraft: the moments
    /// (N m, nose-up positive) minus `I_P th''`, over `W`, m. Zero on the
    /// rotation boundary. `extra_moment_m` is the thrust and friction moment
    /// over `W`.
    fn rotation_balance_residual_m(
        input: &PhysicalCgLimitsInput,
        cg_pct_mac: f64,
        extra_moment_m: f64,
    ) -> f64 {
        let mass_kg = 1000.0;
        let weight_n = mass_kg * input.gravity_m_s2;
        let frame = input.mac_frame;
        let x_cg = frame.leading_edge_x_m + cg_pct_mac / 100.0 * frame.chord_m;
        let x_p = input.x_main_gear_m;
        let x_ac = frame.leading_edge_x_m + input.x_ac_wb_frac * frame.chord_m;
        let wing_lift_n = weight_n * input.cl_ground_attitude / input.cl_r_rotation;
        let tail_lift_n =
            weight_n * input.rotation_tail_lift_coefficient * input.eta * input.tail_area_ratio
                / input.cl_r_rotation;
        let moment_ac_nm = weight_n * input.cm_ac_wb_takeoff / input.cl_r_rotation * frame.chord_m;
        let moments_nm =
            wing_lift_n * (x_p - x_ac) + moment_ac_nm + tail_lift_n * (x_p - input.x_h_ac_m)
                - weight_n * (x_p - x_cg)
                + weight_n * extra_moment_m;
        let inertia_contact = mass_kg * input.pitch_radius_of_gyration_m.powi(2)
            + mass_kg * ((x_p - x_cg).powi(2) + input.h_cg_m.powi(2));
        (moments_nm - inertia_contact * input.rotation_angular_accel_deg_s2.to_radians()) / weight_n
    }

    /// Hand-computed rotation (nose-wheel lift-off) boundary of
    /// [`base_input`] with no thrust or friction, from the moment balance
    /// about the main-gear contact (x_P = 17 m, LEMAC 10 m, c = 4 m).
    ///
    /// Moments over W, m: wing 0.6/1.3 (17 - 11) = 2.769231; wing-body
    /// moment -0.15/1.3 x 4 = -0.461538; tail -1.3 x 0.9 x 0.25/1.3
    /// (17 - 30) = 2.925; available 5.232692. kappa = 5 deg/s^2 / g =
    /// 0.0088987 1/m. Balance 5.232692 - d = kappa (6^2 + 3^2 + d^2), so
    /// d = 4.64061 m, x_cg = 12.35939 m = 58.985 %MAC.
    #[test]
    fn the_rotation_criterion_matches_a_hand_computed_moment_balance() {
        let mut input = base_input();
        input.rotation_thrust_to_weight = f64::NAN;
        input.rotation_rolling_friction_coefficient = 0.0;
        let limits = physical_cg_limits(&input);
        // The expected boundary is computed here from `base_input`'s numbers
        // with the explicit quadratic formula, not the solver's conjugate form.
        let (x_p, lemac, chord) = (17.0, 10.0, 4.0);
        let (cl_g, cl_r, cm_ac, cl_h, eta, s_ratio, x_h) = (0.6, 1.3, -0.15, -1.3, 0.9, 0.25, 30.0);
        let (k_y, h_cg, accel_deg_s2, g) = (6.0_f64, 3.0_f64, 5.0_f64, 9.806_65);
        let wing_lift_m = cl_g / cl_r * (x_p - (lemac + 0.25 * chord));
        let pitching_moment_m = cm_ac / cl_r * chord;
        let tail_lift_m = cl_h * eta * s_ratio / cl_r * (x_p - x_h);
        let available_m = wing_lift_m + pitching_moment_m + tail_lift_m;
        let kappa = accel_deg_s2.to_radians() / g;
        let r = available_m - kappa * (k_y * k_y + h_cg * h_cg);
        let offset_m = (-1.0 + (1.0 + 4.0 * kappa * r).sqrt()) / (2.0 * kappa);
        let expected_pct_mac = (x_p - offset_m - lemac) / chord * 100.0;
        assert!(
            (limits.rotation_fwd_pct_mac - expected_pct_mac).abs() < 1.0e-9,
            "{} against {expected_pct_mac}",
            limits.rotation_fwd_pct_mac
        );
        assert!(rotation_balance_residual_m(&input, limits.rotation_fwd_pct_mac, 0.0).abs() < 1e-9);
        // A CG forward of the boundary cannot rotate; one aft of it can.
        assert!(rotation_balance_residual_m(&input, limits.rotation_fwd_pct_mac - 1.0, 0.0) < 0.0);
        assert!(rotation_balance_residual_m(&input, limits.rotation_fwd_pct_mac + 1.0, 0.0) > 0.0);
        // Without the transfer of the inertia to the contact the balance
        // would be linear, available - d = kappa k_y^2: the transfer moves
        // the boundary aft.
        let linear_offset_m = available_m - kappa * k_y * k_y;
        let linear_pct_mac = (x_p - linear_offset_m - lemac) / chord * 100.0;
        assert!(limits.rotation_fwd_pct_mac > linear_pct_mac);
    }

    /// A larger required pitch acceleration needs a larger tail moment, so
    /// the boundary moves aft monotonically.
    #[test]
    fn the_rotation_boundary_moves_aft_as_the_required_pitch_acceleration_grows() {
        let mut previous = f64::NEG_INFINITY;
        for accel in [0.0, 2.0, 4.0, 6.0, 8.0, 10.0] {
            let mut input = base_input();
            input.rotation_angular_accel_deg_s2 = accel;
            let boundary = physical_cg_limits(&input).rotation_fwd_pct_mac;
            assert!(boundary > previous, "{accel} deg/s^2: {boundary}");
            previous = boundary;
        }
    }

    /// More up-elevator gives more tail download and moves the boundary
    /// forward monotonically.
    #[test]
    fn the_rotation_boundary_moves_forward_as_the_elevator_deflects_further_up() {
        let mut previous = f64::INFINITY;
        for deflection_deg in [-5.0_f64, -10.0, -15.0, -20.0, -25.0, -30.0] {
            let mut input = base_input();
            input.rotation_tail_lift_coefficient = super::super::rotation::tail_lift_coefficient(
                4.0,
                (-2.0_f64).to_radians(),
                3.0_f64.to_radians(),
                0.6,
                deflection_deg.to_radians(),
            );
            let boundary = physical_cg_limits(&input).rotation_fwd_pct_mac;
            assert!(boundary < previous, "{deflection_deg} deg: {boundary}");
            previous = boundary;
        }
    }

    /// Hand-computed landing-trim-in-ground-effect criterion: the scissor
    /// construction with the tail's download knocked down by
    /// `tail_ground_effect_factor`.
    #[test]
    fn the_landing_trim_criterion_matches_a_hand_computed_scissor_construction_with_ground_effect()
    {
        let limits = physical_cg_limits(&base_input());
        let expected_frac = 0.25 - (-0.20 / 2.6) + 0.9 * 0.9 * 1.0 * (-0.8) / 2.6;
        assert!(
            (limits.landing_trim_fwd_pct_mac - expected_frac * 100.0).abs() < 1.0e-9,
            "{} against {}",
            limits.landing_trim_fwd_pct_mac,
            expected_frac * 100.0
        );
        // Ground effect (factor < 1) must make landing trim strictly less
        // permissive (more aft, larger %MAC) than the diagnostic scissor
        // estimate that carries no such knockdown.
        assert!(limits.landing_trim_fwd_pct_mac > limits.scissor_plot_fwd_pct_mac);
    }

    /// A non-finite tail station (no horizontal tail) disables both the
    /// rotation and landing-trim mechanisms without poisoning the others,
    /// mirroring the tip-back boundary's convention.
    #[test]
    fn a_non_finite_tail_station_disables_rotation_and_landing_trim_only() {
        let mut input = base_input();
        input.x_h_ac_m = f64::NAN;
        let limits = physical_cg_limits(&input);
        assert!(limits.rotation_fwd_pct_mac.is_nan());
        // Landing trim does not depend on x_h_ac_m (it uses the tail-volume
        // coefficient, not the raw tail station), so it stays finite.
        assert!(limits.landing_trim_fwd_pct_mac.is_finite());
        assert!(limits.fwd_limit_pct_mac.is_finite());
    }

    #[test]
    fn a_taller_cg_tightens_the_tip_back_boundary_and_can_make_it_govern() {
        let mut input = base_input();
        input.h_cg_m = 0.5;
        let low = physical_cg_limits(&input);
        input.h_cg_m = 6.0;
        let high = physical_cg_limits(&input);
        // A taller CG moves the tip-back boundary forward (smaller %MAC).
        assert!(high.tip_back_aft_pct_mac < low.tip_back_aft_pct_mac);
    }

    #[test]
    fn a_forward_critical_neutral_point_can_make_the_aerodynamic_boundary_govern() {
        let mut input = base_input();
        input.critical_np_x_m = 10.5;
        let limits = physical_cg_limits(&input);
        assert_eq!(limits.aft_limit_governance, AftLimitGovernance::Aerodynamic);
        assert!((limits.aft_limit_pct_mac - limits.aerodynamic_aft_pct_mac).abs() < 1e-9);
    }

    #[test]
    fn a_non_finite_cg_height_disables_the_tip_back_boundary_without_poisoning_the_others() {
        let mut input = base_input();
        input.h_cg_m = f64::NAN;
        let limits = physical_cg_limits(&input);
        assert!(limits.tip_back_aft_pct_mac.is_nan());
        assert!(limits.aft_limit_pct_mac.is_finite());
    }

    #[test]
    fn usable_range_is_the_aft_minus_forward_governing_limits() {
        let limits = physical_cg_limits(&base_input());
        assert!(
            (limits.usable_range_pct_mac - (limits.aft_limit_pct_mac - limits.fwd_limit_pct_mac))
                .abs()
                < 1e-9
        );
    }
}
