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
    /// Minimum nose-load fraction of current weight for steering authority.
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
///   including the required pitch angular acceleration.
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

    let (aft_limit_pct_mac, aft_limit_governance) = most_forward([
        (aerodynamic_aft_pct_mac, AftLimitGovernance::Aerodynamic),
        (
            ground_aft_pct_mac,
            AftLimitGovernance::GroundMinimumNoseLoad,
        ),
        (tip_back_aft_pct_mac, AftLimitGovernance::TipBack),
    ]);

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
    let tail_download_term =
        (ROTATION_CL_H * input.eta * input.tail_ground_effect_factor * input.tail_area_ratio
            / cl_r)
            * (x_mg_frac - x_h_frac);
    let rotation_fwd_frac =
        x_mg_frac + inertia_frac - wing_lift_term - pitching_moment_term - tail_download_term;
    let rotation_fwd_pct_mac = rotation_fwd_frac * 100.0;

    let (fwd_limit_pct_mac, fwd_limit_governance) = most_aft([
        (
            max_nose_load_fwd_pct_mac,
            ForwardLimitGovernance::MaxNoseLoadHandling,
        ),
        (
            rotation_fwd_pct_mac,
            ForwardLimitGovernance::RotationNoseWheelLiftoff,
        ),
        (
            landing_trim_fwd_pct_mac,
            ForwardLimitGovernance::LandingTrimGroundEffect,
        ),
    ]);

    PhysicalCgLimits {
        aft_limit_pct_mac,
        aft_limit_governance,
        clean_np_pct_mac,
        aerodynamic_aft_pct_mac,
        ground_aft_pct_mac,
        tip_back_aft_pct_mac,
        fwd_limit_pct_mac,
        fwd_limit_governance,
        max_nose_load_fwd_pct_mac,
        scissor_plot_fwd_pct_mac,
        rotation_fwd_pct_mac,
        landing_trim_fwd_pct_mac,
        usable_range_pct_mac: aft_limit_pct_mac - fwd_limit_pct_mac,
    }
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
        }
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
        let limits = physical_cg_limits(&base_input());
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
