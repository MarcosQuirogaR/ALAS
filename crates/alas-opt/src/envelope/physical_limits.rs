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

use super::support::ROTATION_CL_H;
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
    /// Nose-wheel liftoff (rotation) moment-balance criterion at `V_R`
    /// (Torenbeek *Synthesis of Subsonic Airplane Design* ch.
    /// 8-9, Roskam *Airplane Design* Part II ch. 11): the most-forward CG at
    /// which the available tail download, at its maximum coefficient, can
    /// still supply the wing-body moment, the required pitch acceleration,
    /// and the weight moment about the main-gear contact point.
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
    /// Minimum nose-load fraction of this state's weight: the steering
    /// minimum, raised near the tabulated weight by a published main-gear
    /// load limit (`alas_config::PublishedAftCgNoseLoad`).
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
    /// download authority, dimensionless in `(0, 1]`; `1.0` = no correction
    /// (`alas_opt::envelope::support::tail_ground_effect_factor`).
    pub tail_ground_effect_factor: f64,
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
    /// Pitch radius of gyration, as a fraction of MAC (`r_y / c`); a
    /// declared Torenbeek/Roskam typical-transport estimate, not measured.
    pub pitch_radius_of_gyration_frac_mac: f64,
    /// Required pitch angular acceleration at rotation, degrees per second
    /// squared; a declared Torenbeek/Roskam typical-transport estimate
    /// (6-8 deg/s^2), not measured.
    pub rotation_angular_accel_deg_s2: f64,
    /// Standard gravity, m/s^2 (`config.requirements.gravity_m_s2`): turns
    /// the pitch-inertia moment into the same weight-normalized fraction of
    /// MAC as every other rotation-criterion term.
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
    /// Diagnostic only: the signed shift the thrust and rolling-friction
    /// term applies to [`Self::rotation_fwd_pct_mac`], `%MAC`; negative
    /// moves the rotation boundary forward.
    pub rotation_longitudinal_force_shift_pct_mac: f64,
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

/// The thrust and rolling-friction moment about the main-gear contact at
/// nose-wheel liftoff, over `W c`, nose-up positive: the amount the rotation
/// boundary moves forward (as a fraction of MAC).
///
/// Statics about the main-gear ground contact, heights above the shared
/// ground plane. Thrust `T` acts forward at `h_T` (moment `-T h_T`). The
/// runway friction `mu (W - L)` acts in the contact plane (no moment). The
/// inertial reaction of the forward acceleration,
/// `m a = T - mu (W - L)`, acts aft at the CG (moment `+m a h_cg`). So
///
/// `dM / (W c) = [T/W (h_cg - h_T) - mu (1 - L/W) h_cg] / c`,
///
/// with `L/W = (CL_g + CL_h eta k_ge S_h/S) / CL_R` at `V_R`, clamped so
/// the wheels never carry a negative load. A thrust line below the CG is
/// nose-up; friction is always nose-down.
///
/// Aerodynamic drag is **omitted**; its net moment about the CG is
/// `D (h_D - h_cg)` and its sign depends on where the drag line sits:
///
/// - low wing (drag line below the CG): the moment is nose-down, so leaving
///   drag out moves the rotation boundary *forward*, which is **not
///   conservative** (about 0.3-0.4 %MAC, an engineering estimate [E]);
/// - high wing or high thrust line such as a turboprop like the ATR (drag
///   line above the CG): the moment is nose-up, so omitting drag is
///   **conservative**.
///
/// Adding a takeoff-configuration drag polar is a documented follow-up.
///
/// A non-finite or non-positive `h_cg_m` returns `0.0` (term not
/// evaluated); a non-finite thrust or thrust-line height drops only the
/// thrust part.
fn rotation_longitudinal_force_frac(input: &PhysicalCgLimitsInput, lift_over_weight: f64) -> f64 {
    let h_cg = input.h_cg_m;
    if !h_cg.is_finite() || h_cg <= 0.0 || input.mac_frame.chord_m <= 0.0 {
        return 0.0;
    }
    let thrust_moment = if input.rotation_thrust_to_weight.is_finite()
        && input.rotation_thrust_line_height_m.is_finite()
    {
        input.rotation_thrust_to_weight * (h_cg - input.rotation_thrust_line_height_m)
    } else {
        0.0
    };
    let wheel_load_fraction = (1.0 - lift_over_weight).max(0.0);
    let friction_moment = input.rotation_rolling_friction_coefficient * wheel_load_fraction * h_cg;
    (thrust_moment - friction_moment) / input.mac_frame.chord_m
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
/// - **Rotation (nose-wheel liftoff)** (Torenbeek ch. 8-9, Roskam Part II ch.
///   11): the moment balance about the main-gear contact point at `V_R`,
///   nondimensionalized by the weight-support lift coefficient `CL_R`
///   (see [`PhysicalCgLimitsInput::cl_r_rotation`]), solved for the most
///   forward CG admitting nose liftoff at the tail's maximum download,
///   including the required pitch angular acceleration and the thrust and
///   rolling-friction moment (drag omitted, non-conservative; see
///   `rotation_longitudinal_force_frac`). The tail lift coefficient
///   (`CL_h = -0.55`), the takeoff `Cm_ac,wb = -0.15` and `x_ac,wb = 0.25`
///   are declared estimates; deriving the elevator authority from the tail
///   geometry is a documented follow-up.
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

    // Rotation (nose-wheel liftoff): moment balance about the
    // main-gear contact point at V_R (Torenbeek ch. 8-9, Roskam Part II ch.
    // 11), nondimensionalized by weight (W = q_R S CL_R) so it needs no
    // explicit mass, dynamic pressure, or wing-area input. Every term below
    // is `moment / W`, hence dimensionless (a fraction of MAC once
    // multiplied by 100); see doc comments on the individual input fields
    // for the physical meaning and source of each factor.
    let x_mg_frac = pct_mac(frame, input.x_main_gear_m) / 100.0;
    let x_h_frac = pct_mac(frame, input.x_h_ac_m) / 100.0;
    let cl_r = input.cl_r_rotation.max(1.0e-6);
    let mac_m = frame.chord_m;
    let inertia_frac = input.pitch_radius_of_gyration_frac_mac.powi(2)
        * mac_m
        * input.rotation_angular_accel_deg_s2.to_radians()
        / input.gravity_m_s2.max(1.0e-6);
    let wing_lift_term = (input.cl_ground_attitude / cl_r) * (x_mg_frac - input.x_ac_wb_frac);
    let pitching_moment_term = input.cm_ac_wb_takeoff / cl_r;
    // Tail download at the elevator-limited rotation lift coefficient
    // (`ROTATION_CL_H`, stabiliser at takeoff trim), in ground effect; its
    // moment arm about the main-gear contact is (x_h - x_mg), so this term
    // is positive (nose-up credit). No clamp: the result follows the
    // geometry.
    let tail_lift_coefficient =
        ROTATION_CL_H * input.eta * input.tail_ground_effect_factor * input.tail_area_ratio;
    let tail_download_term = (tail_lift_coefficient / cl_r) * (x_mg_frac - x_h_frac);
    // Thrust and runway friction about the main-gear contact (see
    // `rotation_longitudinal_force_frac`); the lift that unloads the wheels
    // is the same wing and tail lift the moment terms above use.
    let lift_over_weight = (input.cl_ground_attitude + tail_lift_coefficient) / cl_r;
    let longitudinal_force_frac = rotation_longitudinal_force_frac(input, lift_over_weight);
    let rotation_fwd_frac = x_mg_frac + inertia_frac
        - wing_lift_term
        - pitching_moment_term
        - tail_download_term
        - longitudinal_force_frac;
    let rotation_fwd_pct_mac = rotation_fwd_frac * 100.0;

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
        rotation_longitudinal_force_shift_pct_mac: -100.0 * longitudinal_force_frac,
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
            cl_ground_attitude: 0.6,
            cl_r_rotation: 1.3,
            cm_ac_wb_takeoff: -0.15,
            pitch_radius_of_gyration_frac_mac: 0.30,
            rotation_angular_accel_deg_s2: 7.0,
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

        // L/W = (CL_g + CL_h eta k_ge S_h/S) / CL_R
        //     = (0.6 + (-0.55)(0.9)(0.9)(0.25)) / 1.3 = 0.488625 / 1.3 = 0.375865.
        let lift_over_weight = (0.6 + ROTATION_CL_H * 0.9 * 0.9 * 0.25) / 1.3;
        // Thrust below the CG: 0.30 * (3.0 - 1.8) = 0.36 m, nose-up.
        // Friction: 0.02 * (1 - 0.375865) * 3.0 = 0.037448 m, nose-down.
        // Net 0.322552 m over c = 4.0 m: -8.0638 %MAC.
        let moment_m = 0.30 * (3.0 - 1.8) - 0.02 * (1.0 - lift_over_weight) * 3.0;
        let expected_shift_pct = -100.0 * moment_m / 4.0;
        assert!((expected_shift_pct - -8.0638).abs() < 1.0e-3);
        assert!(
            (with_term.rotation_longitudinal_force_shift_pct_mac - expected_shift_pct).abs()
                < 1.0e-9
        );
        assert!(
            (with_term.rotation_fwd_pct_mac - (without.rotation_fwd_pct_mac + expected_shift_pct))
                .abs()
                < 1.0e-9
        );
        assert_eq!(without.rotation_longitudinal_force_shift_pct_mac, 0.0);
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

    /// Hand-computed rotation (nose-wheel liftoff) criterion against
    /// [`base_input`]'s numbers, verifying the moment-balance construction
    /// term by term.
    #[test]
    fn the_rotation_criterion_matches_a_hand_computed_moment_balance() {
        let mut input = base_input();
        input.rotation_thrust_to_weight = f64::NAN;
        input.rotation_rolling_friction_coefficient = 0.0;
        let limits = physical_cg_limits(&input);
        // x_mg_frac = (17.0-10.0)/4.0 = 1.75; x_h_frac = (30.0-10.0)/4.0 = 5.0.
        let x_mg_frac = 1.75;
        let x_h_frac = 5.0;
        let cl_r: f64 = 1.3;
        let inertia_frac = 0.30_f64.powi(2) * 4.0 * 7.0_f64.to_radians() / 9.806_65;
        let wing_lift_term = (0.6 / cl_r) * (x_mg_frac - 0.25);
        let pitching_moment_term = -0.15 / cl_r;
        let tail_download_term: f64 =
            (ROTATION_CL_H * 0.9 * 0.9 * 0.25 / cl_r) * (x_mg_frac - x_h_frac);
        let expected_frac =
            x_mg_frac + inertia_frac - wing_lift_term - pitching_moment_term - tail_download_term;
        assert!(
            (limits.rotation_fwd_pct_mac - expected_frac * 100.0).abs() < 1.0e-9,
            "{} against {}",
            limits.rotation_fwd_pct_mac,
            expected_frac * 100.0
        );
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
