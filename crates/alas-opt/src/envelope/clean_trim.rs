// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The in-flight forward limit from the tail's trim authority with the flaps
//! retracted: the clean half of CS 25.161(c)(1).
//!
//! CS 25.161(c)(1) (Amdt 28; 14 CFR 25.161(c)(1) is the same clause) asks
//! for longitudinal trim in a climb at maximum continuous power "at a speed
//! not more than 1.3 V_SR1, with the landing gear retracted, and the flaps
//! (i) retracted and (ii) in the take-off position". The take-off half is
//! [`super::ths_trim::TakeoffTrimModel::stop_limited_cg_x_m`]; this module is
//! the retracted half, flown at every airborne mass, so it bounds the en-route
//! states that the landing-trim and rotation balances do not reach.
//!
//! The balance is the take-off trim's (module doc of [`super::ths_trim`]),
//! evaluated with the clean wing-body: `CL = CL_max,clean / 1.3^2`, the clean
//! lift-curve slope (DATCOM), aerodynamic centre
//! ([`super::support::wing_body_ac_frac`]), moment about it and lift at zero
//! fuselage angle (Torenbeek, *Synthesis of Subsonic Airplane Design*, 1982,
//! App. E, eqs. E-29 and E-39/E-40 with no flap), and the free-air
//! lifting-line downwash `epsilon = 2 CL_w / (pi A)`. The CG is solved where
//! the elevator-neutral trim needs exactly the tail's available incidence:
//!
//! - trimmable stabiliser: its sourced nose-up stop (trim is the stabiliser
//!   with the elevator neutral, as in the take-off half);
//! - fixed stabiliser: the built incidence with the elevator at its up travel
//!   (the trim tab only removes the column force, CS 25.161(c)(2) wording, and
//!   adds no download), the same reading as `super::landing_download`.
//!
//! A second condition, the stall reach: the reference stall speed is
//! determined at the c.g. that gives its highest value, the forward one
//! (CS 25.103(b)(5)), from straight flight trimmed at a speed between
//! `1.13 V_SR` and `1.3 V_SR` (CS 25.201(b)(1)), so at the forward limit the
//! elevator alone, from the `1.3 V_SR1` trim, must still reach
//! `CL_max,clean` in free air. The stabiliser keeps the setting the trim
//! needed at that CG; the elevator adds its up travel (with the DATCOM
//! large-deflection reduction, the landing flare's term). A fixed
//! stabiliser reaches it with its built incidence and the elevator at that
//! up travel, wherever the trim tab held the elevator. The more aft of the
//! two limits governs.
//!
//! The tail lift at each limit is capped at the stall of the inverted tail
//! (Raymer, *Aircraft Design: A Conceptual Approach*, 6th ed., eq. 12.21,
//! [`super::rotation::tail_download_cap`]), the rotation balance's cap.
//!
//! Stated simplification: the thrust moment of maximum continuous power is
//! left out. Under-wing engines below the CG give a nose-up moment, so the
//! omission makes the limit slightly conservative (aft); its size is the
//! take-off trim's thrust term at the lower continuous rating.
//!
//! The balance is linear in the CG station and independent of the mass at a
//! fixed speed ratio; the state's mass enters only through which states the
//! phase scope applies it to.

use super::ths_trim::TakeoffTrimModel;

/// Speed of the clean climb-trim requirement over `V_SR1` (CS 25.161(c)(1)).
pub(super) const CLEAN_TRIM_SPEED_OVER_VSR: f64 = 1.3;

/// The clean climb-trim balance and the tail's available incidence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CleanTrimModel {
    /// The trim balance with the clean wing-body terms in the climb-trim
    /// slots (`cl_climb_trim`, `cl_zero_alpha_free_air`, `cm_ac_wb_free_air`,
    /// the clean slope and centre) and no thrust term.
    pub balance: TakeoffTrimModel,
    /// Most nose-up effective tail incidence available, rad (negative): the
    /// stabiliser stop with the elevator neutral, or the built incidence plus
    /// the full up-elevator term on a fixed stabiliser.
    pub available_incidence_rad: f64,
    /// Largest download coefficient the tail supports, positive.
    pub section_cap: f64,
    /// Elevator increment from the trim to full up-elevator on a trimmable
    /// stabiliser, rad (negative); `NaN` on a fixed stabiliser.
    pub stall_elevator_increment_rad: f64,
    /// Effective tail incidence available at the stall reach on a fixed
    /// stabiliser, rad (negative): the built incidence with the elevator at
    /// its up travel, whatever the trim held; `NaN` on a trimmable one.
    pub stall_available_incidence_rad: f64,
    /// Largest download coefficient with the elevator deflected, positive.
    pub stall_section_cap: f64,
    /// Whether the stabiliser is fixed (selects the label).
    pub fixed_stabiliser: bool,
}

/// Which clean condition sets the limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanTrimCondition {
    /// Trim at `1.3 V_SR1`, at the tail's trim authority.
    Trim,
    /// Trim limited by the tail's stall.
    TrimTailStall,
    /// Elevator stall reach at `CL_max,clean`.
    StallReach,
    /// Stall reach limited by the tail's stall.
    StallReachTailStall,
}

impl CleanTrimModel {
    /// The most forward CG station, m, the clean condition admits, and which
    /// condition sets it. `None` without finite terms.
    #[must_use]
    pub fn forward_limit(&self) -> Option<(f64, CleanTrimCondition)> {
        let m = &self.balance;
        let (x0, x1) = (m.x_ac_wb_m, m.x_ac_wb_m + m.mac_m);
        // Root of a function linear in the CG station, rising aft.
        let root = |f: &dyn Fn(f64) -> f64| -> Option<f64> {
            let (f0, f1) = (f(x0), f(x1));
            let slope = (f1 - f0) / (x1 - x0);
            (f0.is_finite() && f1.is_finite() && slope > 0.0).then(|| x0 - f0 / slope)
        };
        let trim = |x: f64| m.climb_trim_incidence_rad(x);
        let stall = |x: f64| m.trim_incidence_rad(x);
        // Tail stall: the tail share at CG x and lift cl is (c Cm + cl (x - x_ac)) / arm.
        let arm = m.x_h_ac_m - m.x_ac_wb_m;
        let eta_r = m.eta * m.tail_area_ratio;
        let stall_root = |cl: f64, cap: f64| {
            if cap.is_finite() && cap > 0.0 && cl > 0.0 {
                m.x_ac_wb_m + (-cap * eta_r * arm - m.mac_m * m.cm_ac_wb_free_air) / cl
            } else {
                f64::NEG_INFINITY
            }
        };
        let available = self.available_incidence_rad;
        let x_trim = root(&|x| trim(x) - available)?;
        let x_trim_stall = stall_root(m.cl_climb_trim, self.section_cap);
        // Stall reach: the setting the trim needed plus the elevator increment
        // must reach the incidence CL_max needs.
        // A fixed stabiliser reaches it with the elevator at its up travel from
        // wherever the trim tab held it, so its condition is absolute.
        let increment = self.stall_elevator_increment_rad;
        let fixed_available = self.stall_available_incidence_rad;
        let x_reach = if increment.is_finite() {
            root(&|x| stall(x) - trim(x) - increment).unwrap_or(f64::NEG_INFINITY)
        } else if fixed_available.is_finite() {
            root(&|x| stall(x) - fixed_available).unwrap_or(f64::NEG_INFINITY)
        } else {
            f64::NEG_INFINITY
        };
        let x_reach_stall = if increment.is_finite() || fixed_available.is_finite() {
            stall_root(m.cl_v2, self.stall_section_cap)
        } else {
            f64::NEG_INFINITY
        };
        let candidates = [
            (x_trim, CleanTrimCondition::Trim),
            (x_trim_stall, CleanTrimCondition::TrimTailStall),
            (x_reach, CleanTrimCondition::StallReach),
            (x_reach_stall, CleanTrimCondition::StallReachTailStall),
        ];
        let (x, condition) = candidates
            .into_iter()
            .filter(|(x, _)| !x.is_nan())
            .fold((f64::NEG_INFINITY, CleanTrimCondition::Trim), |best, c| {
                if c.0 > best.0 { c } else { best }
            });
        x.is_finite().then_some((x, condition))
    }

    /// [`Self::forward_limit`]'s station alone.
    #[must_use]
    pub fn forward_limit_x_m(&self) -> Option<f64> {
        self.forward_limit().map(|(x, _)| x)
    }
}

/// The clean climb-trim model of `plane`: the tail terms of the landing
/// balance's `authority` (slope, the stabiliser stop or the built incidence,
/// and the elevator term a fixed stabiliser's trim tab holds) with the clean
/// wing-body, and the rotation tail's stall cap (the clean section's for an
/// elevator held neutral). `None` without a tail, finite terms or
/// `CL_max,clean`.
// One named physical input per term of the trim balance.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub(super) fn clean_trim_model(
    plane: &alas_geom::aircraft::airplane::Airplane,
    config: &alas_config::AlasConfig,
    s_ref: f64,
    mac_m: f64,
    x_mac_le_m: f64,
    x_h_ac_m: f64,
    tail_area_ratio: f64,
    authority: Option<super::landing_download::LandingTrimAuthority>,
    rotation_tail: Option<super::rotation::RotationTailLift>,
) -> Option<CleanTrimModel> {
    let authority = authority?;
    let tail = rotation_tail?;
    let wing = plane.wings.first()?;
    let cl_max_clean = config.performance.cl_max_clean;
    if !(cl_max_clean.is_finite() && cl_max_clean > 0.0 && tail_area_ratio > 0.0) {
        return None;
    }
    let (cm_ac_wb, cl_zero_alpha) = super::support::clean_wing_body_terms(plane, config, s_ref)?;
    let wing_aspect_ratio = plane.b_ref * plane.b_ref / s_ref;
    let wing_slope = alas_stab::static_stability::datcom(
        wing_aspect_ratio,
        wing.mean_sweep_angle(0.25).to_radians(),
        wing.taper_ratio(),
        0.2,
    );
    let x_ac_wb_m = x_mac_le_m + super::support::wing_body_ac_frac(plane, config) * mac_m;
    let cl_trim = cl_max_clean / CLEAN_TRIM_SPEED_OVER_VSR.powi(2);
    let fixed_stabiliser = authority.fixed_stabiliser;
    let (available_incidence_rad, section_cap, stall_elevator_increment_rad, stall_available_incidence_rad) =
        if fixed_stabiliser {
            (
                authority.incidence_stop + authority.elevator_term_approach,
                tail.section_cap,
                f64::NAN,
                authority.incidence_stop + authority.elevator_term,
            )
        } else {
            (
                authority.incidence_stop,
                tail.clean_section_cap,
                authority.elevator_term,
                f64::NAN,
            )
        };
    let balance = TakeoffTrimModel {
        // `trim_incidence_rad` is the stall reach: CL_max,clean, clean terms.
        cl_v2: cl_max_clean,
        cl_ground_attitude: cl_zero_alpha,
        cl_climb_trim: cl_trim,
        cl_zero_alpha_free_air: cl_zero_alpha,
        cm_ac_wb_free_air: cm_ac_wb,
        wing_lift_curve_slope_per_rad: wing_slope,
        wing_aspect_ratio,
        tail_lift_curve_slope_per_rad: authority.tail_slope,
        tail_area_ratio,
        eta: super::support::tail_efficiency(plane),
        x_ac_wb_m,
        x_h_ac_m,
        mac_m,
        cm_ac_wb,
        thrust_to_weight: 0.0,
        thrust_arm_m: 0.0,
        built_incidence_rad: tail.built_incidence_rad,
        max_nose_up_deg: None,
        rotation_downwash_rad: tail.downwash_rad,
        rotation_elevator_term_rad: tail.elevator_term_rad,
        rotation_section_cap: tail.section_cap,
    };
    let finite = [wing_slope, authority.tail_slope, x_ac_wb_m, x_h_ac_m, available_incidence_rad]
        .iter()
        .all(|value| value.is_finite());
    (finite && wing_slope > 0.0 && authority.tail_slope > 0.0).then_some(CleanTrimModel {
        balance,
        available_incidence_rad,
        section_cap,
        stall_elevator_increment_rad,
        stall_available_incidence_rad,
        stall_section_cap: tail.section_cap,
        fixed_stabiliser,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(available_deg: f64) -> CleanTrimModel {
        CleanTrimModel {
            balance: TakeoffTrimModel {
                cl_v2: 1.5,
                cl_ground_attitude: 0.25,
                cl_climb_trim: 1.5 / 1.69,
                cl_zero_alpha_free_air: 0.25,
                cm_ac_wb_free_air: -0.08,
                wing_lift_curve_slope_per_rad: 5.0,
                wing_aspect_ratio: 9.0,
                tail_lift_curve_slope_per_rad: 3.2,
                tail_area_ratio: 0.25,
                eta: 0.85,
                x_ac_wb_m: 30.0,
                x_h_ac_m: 62.0,
                mac_m: 7.0,
                cm_ac_wb: -0.08,
                thrust_to_weight: 0.0,
                thrust_arm_m: 0.0,
                built_incidence_rad: 0.0,
                max_nose_up_deg: None,
                rotation_downwash_rad: 0.0,
                rotation_elevator_term_rad: 0.0,
                rotation_section_cap: 1.0,
            },
            available_incidence_rad: -available_deg.to_radians(),
            section_cap: 1.0,
            stall_elevator_increment_rad: f64::NAN,
            stall_available_incidence_rad: f64::NAN,
            stall_section_cap: 1.2,
            fixed_stabiliser: false,
        }
    }

    /// At the trim limit the elevator-neutral trim needs exactly the
    /// available incidence; a CG forward of it needs more download.
    #[test]
    fn the_trim_limit_is_where_the_trim_needs_the_available_incidence() {
        let m = model(8.0);
        let (x, condition) = m.forward_limit().expect("finite terms");
        assert_eq!(condition, CleanTrimCondition::Trim);
        let need = |x: f64| m.balance.climb_trim_incidence_rad(x);
        assert!((need(x) - m.available_incidence_rad).abs() < 1.0e-12);
        assert!(need(x - 0.5) < m.available_incidence_rad);
    }

    /// More travel moves the limit forward until the tail's stall holds it.
    #[test]
    fn more_travel_moves_the_limit_forward_until_the_tail_stalls() {
        let small = model(4.0).forward_limit_x_m().unwrap();
        let large = model(8.0).forward_limit_x_m().unwrap();
        assert!(large < small, "{large} {small}");
        let (_, condition) = model(60.0).forward_limit().unwrap();
        assert_eq!(condition, CleanTrimCondition::TrimTailStall);
    }

    /// At the stall-reach limit the trim setting plus the elevator increment
    /// gives exactly the incidence CL_max needs; a small increment governs.
    #[test]
    fn a_small_elevator_increment_sets_the_stall_reach_limit() {
        let mut m = model(12.0);
        m.stall_elevator_increment_rad = (-1.0_f64).to_radians();
        let (x, condition) = m.forward_limit().unwrap();
        assert_eq!(condition, CleanTrimCondition::StallReach);
        let b = &m.balance;
        let gap = b.trim_incidence_rad(x) - b.climb_trim_incidence_rad(x);
        assert!((gap - m.stall_elevator_increment_rad).abs() < 1.0e-12);
        let x_trim_only = model(12.0).forward_limit_x_m().unwrap();
        assert!(x > x_trim_only);
    }

    #[test]
    fn no_finite_incidence_gives_no_limit() {
        let mut m = model(8.0);
        m.available_incidence_rad = f64::NAN;
        assert!(m.forward_limit().is_none());
    }
}
