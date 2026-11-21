// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The landing forward-CG limit from the tail's own trim authority, in place
//! of Torenbeek's design constant `C_L,h = -0.80` (*Advanced Aircraft Design*,
//! Wiley, 2013, sec. 6.3.3, p. 168, eq. 6.2, a constant paired there with
//! `C_M,ac = -0.3 Delta C_L0`, the flap moment used here too,
//! [`super::support::landing_pitching_moment`]). Two conditions, the more restrictive governing
//! (Raymer, *Aircraft Design: A Conceptual Approach*, 6th ed., pp. 609-610:
//! the elevator must trim the airplane with full flaps at the full-forward
//! CG, with "some additional elevator authority" left for control "including
//! landing flare at maximum forward c.g."):
//!
//! 1. Approach trim at `V_app = 1.23 V_SR0` (CS 25.125), `C_L,app =
//!    C_L,max / 1.23^2`, in free air. The stabiliser is at its sourced
//!    nose-up stop, the elevator neutral:
//!    `C_L,h = a_h (alpha_f - epsilon + i_h,stop)`.
//! 2. Flare to touchdown at `V_TD = 1.15 V_S` with `n = 1.2` (Raymer 6th ed.,
//!    sec. 17.9, "Flare", p. 677), `C_L,fl = n C_L,max / 1.15^2`, in ground
//!    effect: lift-curve slopes of wing and tail +10 % and the downwash
//!    halved (Raymer 6th ed., pp. 609-610). The stabiliser stays at the
//!    setting the approach trim needed at that CG; the elevator alone, at its
//!    sourced up travel with the DATCOM large-deflection reduction K'
//!    (USAF Stability and Control DATCOM 1978, sec. 6.1.1.1, Fig.
//!    6.1.1.1-40; [`super::rotation::large_deflection_effectiveness_factor`]),
//!    supplies the rest.
//!
//! 3. Stall reach at `C_L,max` (CS 25.103(b)(5): the reference stall speed
//!    is determined at the c.g. giving its highest value, the forward one;
//!    CS 25.201(b)(1): from straight flight trimmed between `1.13 V_SR` and
//!    `1.3 V_SR`). From the approach trim the elevator alone, at the up
//!    travel it uses in the flare, must reach `C_L,max,land` in free air;
//!    the stabiliser keeps the approach setting (a fixed stabiliser its
//!    built incidence). The flare stops short of `C_L,max` (`1.2 / 1.15^2 =
//!    0.91`) but in ground effect; the stall reach takes the full lift with
//!    the full free-air downwash, so either can govern.
//!
//! In all of them the tail lift is capped at the stall of the inverted tail
//! (Raymer 6th ed., eq. 12.21, [`super::rotation::tail_download_cap`]) at the
//! condition's speed: the clean section's for the approach of a trimmable
//! stabiliser, whose elevator is neutral, and with the elevator's
//! `Delta c_l,max` wherever the elevator is deflected. The tail slope and the elevator are the rotation
//! balance's ([`super::rotation`], Sadraey 2012, sec. 12.6, `C_L,h = a_h
//! (alpha_f - epsilon + i_h + tau delta_e)`). The fuselage angle uses the
//! flapped zero-angle lift of the wing-body (Torenbeek, *Synthesis of Subsonic
//! Airplane Design*, 1982, App. E eq. E-29, App. G eq. G-21); the downwash is
//! the free-air lifting-line `epsilon = 2 C_L / (pi A)`. The dynamic-pressure
//! ratio stays Torenbeek's paired `q_h/q`. The wing-body slope, centre and
//! moment carry the extended chord of the landing flaps and slats
//! ([`super::extended_chord`]; Raymer 6th ed., printed p. 413), the wing of
//! the neutral point's high-lift condition.
//!
//! The condition applies to every trimmable stabiliser ([`trim_travel_sourced`]),
//! with the preset's own travels or the labelled class values.
//!
//! A fixed stabiliser (pitch trim by elevator tab) has no setting to move: its
//! incidence stays at the built value and the elevator alone carries both
//! conditions, at its declared up travel (the TCDS control-surface movements),
//! so the tab only removes the stick force (CS 25.161(c)(2)) and adds no
//! download. Raymer's two conditions then read: the approach trimmed with up
//! to full up-elevator in free air, and the flare at full up-elevator in
//! ground effect (Raymer 6th ed., pp. 609-610; the same slopes, downwash,
//! elevator effectiveness and tail-stall cap as the trimmable case). This
//! replaces Torenbeek's design constant `C_L,h = -0.80`, which is the download
//! a trimmable stabiliser can reach and overstates what a fixed incidence with
//! the elevator alone gives at the flare's fuselage angle.

use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;

use super::rotation::{
    elevator_effectiveness, elevator_up_deflection_deg, large_deflection_effectiveness_factor,
    tail_download_cap, takeoff_incidence_rad,
};

/// ISA sea-level density, kg/m^3.
const SEA_LEVEL_DENSITY_KG_M3: f64 = 1.225;
/// Reference approach speed over the stall speed `V_SR0` (CS 25.125).
const APPROACH_OVER_STALL: f64 = 1.23;
/// Touchdown speed over the stall speed (Raymer 6th ed., p. 677).
const TOUCHDOWN_OVER_STALL: f64 = 1.15;
/// Flare load factor of a typical aircraft (Raymer 6th ed., p. 677).
const FLARE_LOAD_FACTOR: f64 = 1.2;
/// Lift-curve-slope gain of wing and tail within about 20 % of the span of
/// the ground (Raymer 6th ed., pp. 609-610).
const GROUND_SLOPE_GAIN: f64 = 1.1;
/// Downwash in that ground effect, over its free-air value (same page).
const GROUND_DOWNWASH_FACTOR: f64 = 0.5;

/// Which condition and which tail limit set the landing forward limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LandingTrimCondition {
    /// Approach trim, stabiliser at its nose-up stop.
    Approach,
    /// Flare increment, elevator alone from the approach trim.
    Flare,
    /// Approach trim limited by the tail's stall.
    ApproachTailStall,
    /// Flare limited by the tail's stall.
    FlareTailStall,
    /// Stall reach at `C_L,max`, elevator alone from the approach trim.
    StallReach,
    /// Stall reach limited by the tail's stall.
    StallReachTailStall,
}

impl LandingTrimCondition {
    /// Stable short label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Approach => "approach trim",
            Self::Flare => "flare",
            Self::ApproachTailStall => "approach, tail stall",
            Self::FlareTailStall => "flare, tail stall",
            Self::StallReach => "stall reach",
            Self::StallReachTailStall => "stall reach, tail stall",
        }
    }
}

/// The tail terms of the two landing conditions (angles in rad, lift
/// coefficients on the tail's own area, stall caps positive).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LandingTrimAuthority {
    /// Approach wing lift coefficient.
    pub cl_approach: f64,
    /// Flare wing lift coefficient.
    pub cl_flare: f64,
    /// Free-air tail lift-curve slope, per rad.
    pub tail_slope: f64,
    /// Fuselage angle at the approach lift, rad.
    pub alpha_approach: f64,
    /// Downwash at the approach lift, rad.
    pub downwash_approach: f64,
    /// Fuselage angle at the flare lift, in ground effect, rad.
    pub alpha_flare: f64,
    /// Downwash at the flare lift, in ground effect, rad.
    pub downwash_flare: f64,
    /// Stall-reach wing lift coefficient, `C_L,max,land`.
    pub cl_stall: f64,
    /// Fuselage angle at `C_L,max`, free air, rad.
    pub alpha_stall: f64,
    /// Downwash at `C_L,max`, free air, rad.
    pub downwash_stall: f64,
    /// Stabiliser incidence at its nose-up stop, rad (negative).
    pub incidence_stop: f64,
    /// Elevator term `tau K' span delta_e` at the up travel, rad
    /// (negative); the flare's (and, on a reversible elevator, the
    /// deflection the flare pull holds, [`Self::with_reversible_elevator`]).
    pub elevator_term: f64,
    /// Elevator term of a fixed stabiliser's approach trim, rad: the up
    /// travel, or on a reversible elevator what the trim tab holds at zero
    /// column force.
    pub elevator_term_approach: f64,
    /// Tail stall download limit at approach speed. A trimmable stabiliser
    /// trims the approach with the elevator neutral, so its cap is the clean
    /// section's (Raymer eq. 12.21 without the elevator's `Delta c_l,max`,
    /// which exists only with the elevator deflected); a fixed stabiliser
    /// trims with the elevator up and keeps the elevator's increment.
    pub stall_cap_approach: f64,
    /// Tail stall download limit at touchdown speed.
    pub stall_cap_flare: f64,
    /// Tail stall download limit at the stall speed.
    pub stall_cap_stall: f64,
    /// Aerodynamic-centre shift of the landing flaps' extended chord, a
    /// fraction of the MAC (aft positive; [`super::extended_chord`]).
    pub ac_shift_frac: f64,
    /// Moment-coefficient increment of the same about the shifted centre.
    pub cm_ac_shift: f64,
    /// A fixed stabiliser: [`Self::incidence_stop`] is the built incidence
    /// and the elevator at its up travel acts in both conditions.
    pub fixed_stabiliser: bool,
    /// Tail arm from the shifted centre over the arm from the clean one.
    pub tail_arm_ratio: f64,
}

impl LandingTrimAuthority {
    /// Forward limit as a fraction of the MAC frame, and what sets it.
    ///
    /// `x_ac` and `cm_ac` are the clean wing-body aerodynamic centre and
    /// moment coefficient (about it, on `S` and the MAC) and `tail_term` the
    /// product `eta S_h l_h / (S c)` of eq. 6.2, which [`Self::flapped`] moves
    /// to the landing wing. The tail lift needed at CG `x` and lift `C_L` is
    /// `C_L,h = (C_L (x - x_ac) + C_m,ac) / tail_term`.
    #[must_use]
    pub fn forward_limit(
        &self,
        x_ac: f64,
        cm_ac: f64,
        tail_term: f64,
    ) -> Option<(f64, LandingTrimCondition)> {
        let ((x1, c1), (x2, c2)) = self.candidates(x_ac, cm_ac, tail_term)?;
        let (x, condition) = if x2 > x1 { (x2, c2) } else { (x1, c1) };
        let (x, condition) = match self.stall_reach(x_ac, cm_ac, tail_term) {
            Some((x3, c3)) if x3 > x => (x3, c3),
            _ => (x, condition),
        };
        x.is_finite().then_some((x, condition))
    }

    /// The stall-reach limit (module doc, condition 3) as a fraction of the
    /// MAC frame, with the tail-stall flag; `None` without finite terms.
    #[must_use]
    pub fn stall_reach(
        &self,
        x_ac: f64,
        cm_ac: f64,
        tail_term: f64,
    ) -> Option<(f64, LandingTrimCondition)> {
        if !(tail_term.is_finite() && tail_term > 0.0 && x_ac.is_finite() && cm_ac.is_finite()) {
            return None;
        }
        let (x_ac, cm_ac, tail_term) = self.flapped(x_ac, cm_ac, tail_term);
        let s = self.cl_stall;
        if !(s.is_finite() && s > 0.0) {
            return None;
        }
        let x_lift = if self.fixed_stabiliser {
            let cl_h = self.tail_slope
                * (self.alpha_stall - self.downwash_stall + self.incidence_stop + self.elevator_term);
            x_ac - cm_ac / s + tail_term * cl_h / s
        } else {
            // The approach trim needs C_L,h,a = (a u + C_m) / T at u = x - x_ac; the
            // elevator adds delta at the stall angle; the stall needs (s u + C_m) / T.
            // Feasible where T delta <= (s - a) u.
            let a = self.cl_approach;
            let delta = self.tail_slope
                * ((self.alpha_stall - self.downwash_stall)
                    - (self.alpha_approach - self.downwash_approach)
                    + self.elevator_term);
            if s > a {
                x_ac + tail_term * delta / (s - a)
            } else {
                f64::NEG_INFINITY
            }
        };
        let x_cap = x_ac - cm_ac / s - tail_term * self.stall_cap_stall / s;
        let (x, condition) = if x_cap > x_lift {
            (x_cap, LandingTrimCondition::StallReachTailStall)
        } else {
            (x_lift, LandingTrimCondition::StallReach)
        };
        x.is_finite().then_some((x, condition))
    }

    /// The two landing conditions' own limits as fractions of the MAC frame:
    /// approach trim at the stop with the elevator neutral, and the flare
    /// (elevator alone from that trim), each with the tail-stall flag.
    #[must_use]
    pub fn candidates(
        &self,
        x_ac: f64,
        cm_ac: f64,
        tail_term: f64,
    ) -> Option<((f64, LandingTrimCondition), (f64, LandingTrimCondition))> {
        if !(tail_term.is_finite() && tail_term > 0.0 && x_ac.is_finite() && cm_ac.is_finite()) {
            return None;
        }
        let (x_ac, cm_ac, tail_term) = self.flapped(x_ac, cm_ac, tail_term);
        let a = self.cl_approach;
        let f = self.cl_flare;
        if self.fixed_stabiliser {
            return Some(self.fixed_stabiliser_candidates(x_ac, cm_ac, tail_term));
        }
        // Approach: stabiliser at the stop, elevator neutral.
        let cl_h_stop =
            self.tail_slope * (self.alpha_approach - self.downwash_approach + self.incidence_stop);
        let x_stop = x_ac - cm_ac / a + tail_term * cl_h_stop / a;
        let x_stall_a = x_ac - cm_ac / a - tail_term * self.stall_cap_approach / a;
        let (x1, c1) = if x_stall_a > x_stop {
            (x_stall_a, LandingTrimCondition::ApproachTailStall)
        } else {
            (x_stop, LandingTrimCondition::Approach)
        };
        // Flare: the stabiliser at the setting the approach needs at the CG,
        // the elevator alone for the increment. Linear in the CG offset
        // u = x - x_ac; feasible aft of the root because the flare lift
        // outgrows the tail's gain.
        let ratio = GROUND_SLOPE_GAIN;
        let a_ground = self.tail_slope * ratio;
        let delta = a_ground
            * ((self.alpha_flare - self.downwash_flare)
                - (self.alpha_approach - self.downwash_approach)
                + self.elevator_term);
        let denominator = f - ratio * a;
        let x_linear = if denominator > 0.0 {
            let u = (tail_term * delta - cm_ac * (1.0 - ratio)) / denominator;
            x_ac + u
        } else {
            f64::NEG_INFINITY
        };
        let x_stall_f = x_ac - cm_ac / f - tail_term * self.stall_cap_flare / f;
        let (x2, c2) = if x_stall_f > x_linear {
            (x_stall_f, LandingTrimCondition::FlareTailStall)
        } else {
            (x_linear, LandingTrimCondition::Flare)
        };
        Some(((x1, c1), (x2, c2)))
    }

    /// The clean wing-body centre, moment and tail term of
    /// [`Self::forward_limit`] moved to the landing wing: `(x_ac + shift,
    /// cm_ac + increment, tail_term * arm ratio)`.
    /// The two conditions of a fixed stabiliser on the flapped terms: the
    /// built incidence plus the full up-elevator, free air at the approach,
    /// in ground effect at the flare, each capped at the tail's stall.
    fn fixed_stabiliser_candidates(
        &self,
        x_ac: f64,
        cm_ac: f64,
        tail_term: f64,
    ) -> ((f64, LandingTrimCondition), (f64, LandingTrimCondition)) {
        let at = |cl: f64, cl_h: f64| x_ac - cm_ac / cl + tail_term * cl_h / cl;
        let pick = |cl: f64, cl_h: f64, cap: f64, own, stalled| {
            let x_lift = at(cl, cl_h);
            let x_stall = at(cl, -cap);
            if x_stall > x_lift {
                (x_stall, stalled)
            } else {
                (x_lift, own)
            }
        };
        let a = self.cl_approach;
        let f = self.cl_flare;
        let cl_h_approach = self.tail_slope
            * (self.alpha_approach - self.downwash_approach
                + self.incidence_stop
                + self.elevator_term_approach);
        let cl_h_flare = self.tail_slope
            * GROUND_SLOPE_GAIN
            * (self.alpha_flare - self.downwash_flare + self.incidence_stop + self.elevator_term);
        (
            pick(
                a,
                cl_h_approach,
                self.stall_cap_approach,
                LandingTrimCondition::Approach,
                LandingTrimCondition::ApproachTailStall,
            ),
            pick(
                f,
                cl_h_flare,
                self.stall_cap_flare,
                LandingTrimCondition::Flare,
                LandingTrimCondition::FlareTailStall,
            ),
        )
    }

    /// This authority at landing mass `mass_kg` with a reversible elevator:
    /// the approach elevator term is what the trim tab holds trimmed (zero
    /// force, CS 25.161(c)) and the flare's what the one-hand pull holds
    /// (CS 25.143(d)), each at its own dynamic pressure (`q = n W / (S C_L)`,
    /// on the tail `eta q`) and tail angle of attack.
    #[must_use]
    pub fn with_reversible_elevator(
        &self,
        elevator: &super::elevator_force::ReversibleElevator,
        weight_n: f64,
        s_ref: f64,
        eta: f64,
    ) -> Self {
        let q_h = |load_factor: f64, cl: f64| eta * load_factor * weight_n / (s_ref * cl);
        let alpha_approach = self.alpha_approach - self.downwash_approach + self.incidence_stop;
        let alpha_flare = self.alpha_flare - self.downwash_flare + self.incidence_stop;
        Self {
            elevator_term_approach: elevator.elevator_term_rad(
                q_h(1.0, self.cl_approach),
                alpha_approach,
                super::elevator_force::TRIMMED_FORCE_N,
            ),
            elevator_term: elevator.elevator_term_rad(
                q_h(FLARE_LOAD_FACTOR, self.cl_flare),
                alpha_flare,
                super::elevator_force::FLARE_PULL_FORCE_N,
            ),
            ..*self
        }
    }

    #[must_use]
    pub fn flapped(&self, x_ac: f64, cm_ac: f64, tail_term: f64) -> (f64, f64, f64) {
        (
            x_ac + self.ac_shift_frac,
            cm_ac + self.cm_ac_shift,
            tail_term * self.tail_arm_ratio,
        )
    }

    /// The tail lift coefficient the governing condition needs at the limit
    /// (diagnostic): negative is download.
    #[must_use]
    pub fn tail_lift_at(
        &self,
        condition: LandingTrimCondition,
        x_ac: f64,
        x: f64,
        cm_ac: f64,
        tail_term: f64,
    ) -> f64 {
        let (x_ac, cm_ac, tail_term) = self.flapped(x_ac, cm_ac, tail_term);
        let cl = match condition {
            LandingTrimCondition::Approach | LandingTrimCondition::ApproachTailStall => {
                self.cl_approach
            }
            LandingTrimCondition::StallReach | LandingTrimCondition::StallReachTailStall => {
                self.cl_stall
            }
            _ => self.cl_flare,
        };
        (cl * (x - x_ac) + cm_ac) / tail_term
    }
}

/// Class value of the trimmable-stabiliser nose-up stop, deg, for a type that
/// declares none [E]: adjustable-tail transports travel about +5 to -12 deg,
/// i.e. 12 deg nose-up (Sadraey, *Aircraft Design: A Systems Engineering
/// Approach*, Wiley 2013, sec. 6.7.1 p.306 and Table 6.5 p.307: A300B -12,
/// L-1011 -14, C-5A -12, 777 -11). One class-generic value; no per-aircraft
/// tuning. The elevator class value is [`super::rotation::MAX_ELEVATOR_UP_DEFLECTION_DEG`]
/// (25 deg, Sadraey sec. 12.6; Table 12.18 p.675 lists 25 to 27 deg for
/// transports).
pub(super) const CLASS_STABILIZER_NOSE_UP_TRAVEL_DEG: f64 = 12.0;

/// The nose-up stop used by the landing balance, deg: the preset's own
/// travel when declared and positive, else the class value.
pub(super) fn stabilizer_nose_up_stop_deg(config: &AlasConfig) -> f64 {
    config
        .landing_gear
        .stabilizer_nose_up_travel_deg
        .filter(|deg| deg.is_finite() && *deg > 0.0)
        .unwrap_or(CLASS_STABILIZER_NOSE_UP_TRAVEL_DEG)
}

/// Whether the stabiliser is trimmable (the two-condition trim balance with a
/// stabiliser stop). The travels are the preset's own where declared and the
/// labelled class values otherwise. A fixed stabiliser (trim by elevator tab)
/// takes the elevator-only balance of
/// [`LandingTrimAuthority::fixed_stabiliser`].
pub(super) fn trim_travel_sourced(config: &AlasConfig) -> bool {
    config.landing_gear.trimmable_stabilizer
}

/// [`LandingTrimAuthority`] of `plane` at landing mass `mass_kg`, `None`
/// without a tail, a wing-body term set or finite terms.
pub(super) fn landing_authority(
    plane: &Airplane,
    config: &AlasConfig,
    s_ref: f64,
    mass_kg: f64,
) -> Option<LandingTrimAuthority> {
    let fixed_stabiliser = !trim_travel_sourced(config);
    let tail = plane.wings.get(1)?;
    let terms = super::wing_body::wing_body_terms(plane, config)?;
    let wing = super::support::landing_pitching_moment(plane, config, s_ref)?;
    let chord_sum: f64 = tail.xsecs.iter().map(|section| section.chord).sum();
    if tail.xsecs.is_empty() || chord_sum <= 0.0 {
        return None;
    }
    let built_incidence_rad = (tail
        .xsecs
        .iter()
        .map(|section| section.chord * section.twist)
        .sum::<f64>()
        / chord_sum)
        .to_radians();
    let reference = tail.reference_planform();
    let tail_slope = alas_stab::static_stability::datcom(
        reference.aspect_ratio(),
        tail.mean_sweep_angle(0.25).to_radians(),
        tail.taper_ratio(),
        0.0,
    );
    let cl_max = config.performance.cl_max_land;
    let aspect_ratio = plane.b_ref * plane.b_ref / s_ref;
    let zero_alpha_lift = terms.zero_alpha_lift(&wing);
    let frame = plane.mac_frame()?;
    let x_ac_m = frame.x_lemac_m + super::support::wing_body_ac_frac(plane, config) * frame.chord_m;
    let flapped = super::extended_chord::FlappedWingBody::new(
        terms.cl_alpha_wf,
        terms.cl_alpha_wing,
        x_ac_m,
        frame.chord_m,
        zero_alpha_lift,
        super::extended_chord::extended_lift(
            plane,
            config,
            super::extended_chord::landing_flap_deg(config),
        ),
    );
    let tail_arm_m = super::support::horizontal_tail_ac_x_m(plane) - x_ac_m;
    let cl_approach = cl_max / APPROACH_OVER_STALL.powi(2);
    let cl_flare = FLARE_LOAD_FACTOR * cl_max / TOUCHDOWN_OVER_STALL.powi(2);
    let downwash = |cl: f64| 2.0 * cl / (std::f64::consts::PI * aspect_ratio);
    let incidence_stop = if fixed_stabiliser {
        built_incidence_rad
    } else {
        takeoff_incidence_rad(
            built_incidence_rad,
            Some(stabilizer_nose_up_stop_deg(config)),
        )
    };
    let controls = &config.control_surfaces;
    let span_fraction =
        (controls.elevator_span_end_frac - controls.elevator_span_start_frac).clamp(0.0, 1.0);
    let elevator_deg = elevator_up_deflection_deg(config);
    let effectiveness = elevator_effectiveness(controls.elevator_chord_fraction)
        * large_deflection_effectiveness_factor(elevator_deg, controls.elevator_chord_fraction)
        * span_fraction;
    let stall_speed_m_s = (2.0 * mass_kg * config.requirements.gravity_m_s2
        / (SEA_LEVEL_DENSITY_KG_M3 * s_ref * cl_max))
        .sqrt();
    let cap_at = |speed_over_stall: f64| {
        tail_download_cap(tail, config, speed_over_stall * stall_speed_m_s).1
    };
    let clean_cap_at = |speed_over_stall: f64| {
        tail_download_cap(tail, config, speed_over_stall * stall_speed_m_s).0
    };
    let authority = LandingTrimAuthority {
        cl_approach,
        cl_flare,
        tail_slope,
        alpha_approach: (cl_approach - zero_alpha_lift) / flapped.slope,
        downwash_approach: downwash(cl_approach),
        alpha_flare: (cl_flare - zero_alpha_lift) / (flapped.slope * GROUND_SLOPE_GAIN),
        downwash_flare: GROUND_DOWNWASH_FACTOR * downwash(cl_flare),
        cl_stall: cl_max,
        alpha_stall: (cl_max - zero_alpha_lift) / flapped.slope,
        downwash_stall: downwash(cl_max),
        incidence_stop,
        elevator_term: effectiveness * elevator_deg.to_radians(),
        elevator_term_approach: effectiveness * elevator_deg.to_radians(),
        stall_cap_approach: if fixed_stabiliser {
            cap_at(APPROACH_OVER_STALL)
        } else {
            clean_cap_at(APPROACH_OVER_STALL)
        },
        stall_cap_flare: cap_at(TOUCHDOWN_OVER_STALL),
        stall_cap_stall: cap_at(1.0),
        ac_shift_frac: flapped.ac_shift_m / frame.chord_m,
        cm_ac_shift: flapped.cm_ac_shift,
        tail_arm_ratio: (tail_arm_m - flapped.ac_shift_m) / tail_arm_m,
        fixed_stabiliser,
    };
    let finite = [
        authority.ac_shift_frac,
        authority.cm_ac_shift,
        authority.tail_arm_ratio,
        authority.tail_slope,
        authority.alpha_approach,
        authority.alpha_flare,
        authority.elevator_term,
        authority.stall_cap_approach,
        authority.stall_cap_flare,
        authority.alpha_stall,
        authority.stall_cap_stall,
    ]
    .iter()
    .all(|value| value.is_finite());
    finite.then_some(authority)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn authority() -> LandingTrimAuthority {
        LandingTrimAuthority {
            cl_approach: 1.5,
            cl_flare: 2.0,
            tail_slope: 4.0,
            alpha_approach: 0.10,
            downwash_approach: 0.05,
            alpha_flare: 0.12,
            downwash_flare: 0.03,
            cl_stall: f64::NAN,
            alpha_stall: 0.0,
            downwash_stall: 0.0,
            incidence_stop: -0.20,
            elevator_term: -0.50,
            elevator_term_approach: -0.50,
            stall_cap_approach: 5.0,
            stall_cap_flare: 5.0,
            stall_cap_stall: 5.0,
            ac_shift_frac: 0.0,
            cm_ac_shift: 0.0,
            tail_arm_ratio: 1.0,
            fixed_stabiliser: false,
        }
    }

    /// Hand computed: `C_L,h = 4 (0.10 - 0.05 - 0.20) = -0.60`, so
    /// `x = 0.25 + 0.5/1.5 - 0.8 * 0.6 / 1.5 = 0.26333`; the strong elevator
    /// leaves the flare far forward of it.
    #[test]
    fn approach_trim_at_the_stop_matches_a_hand_computed_balance() {
        let (x, condition) = authority().forward_limit(0.25, -0.5, 0.8).unwrap();
        assert_eq!(condition, LandingTrimCondition::Approach);
        assert!((x - (0.25 + 0.5 / 1.5 - 0.8 * 0.6 / 1.5)).abs() < 1e-12);
    }

    /// With no elevator authority the flare, not the approach, governs and
    /// the limit moves aft.
    #[test]
    fn a_weak_elevator_moves_the_limit_to_the_flare() {
        let mut weak = authority();
        weak.elevator_term = 0.0;
        let (x, condition) = weak.forward_limit(0.25, -0.5, 0.8).unwrap();
        assert_eq!(condition, LandingTrimCondition::Flare);
        assert!(x > 0.2633);
    }

    /// Hand computed stall reach with no elevator: from the approach trim the
    /// tail gains `4 ((0.14 - 0.06) - (0.10 - 0.05)) = 0.12` at the stall
    /// angle, so `x = 0.25 + 0.8 * 0.12 / (2.2 - 1.5) = 0.38714`; without a
    /// stall lift coefficient the condition is absent.
    #[test]
    fn the_stall_reach_matches_a_hand_computed_balance() {
        let mut weak = authority();
        assert!(weak.stall_reach(0.25, -0.5, 0.8).is_none());
        weak.elevator_term = 0.0;
        weak.cl_stall = 2.2;
        weak.alpha_stall = 0.14;
        weak.downwash_stall = 0.06;
        let (x, condition) = weak.stall_reach(0.25, -0.5, 0.8).unwrap();
        assert_eq!(condition, LandingTrimCondition::StallReach);
        assert!((x - (0.25 + 0.8 * 0.12 / 0.7)).abs() < 1e-12);
        // The flare (0.509) is further aft here and keeps governing.
        let (governing, c) = weak.forward_limit(0.25, -0.5, 0.8).unwrap();
        assert_eq!(c, LandingTrimCondition::Flare);
        assert!(governing > x);
    }

    /// The landing wing's terms enter as a shifted centre, moment and arm.
    #[test]
    fn the_flapped_terms_shift_the_centre_moment_and_tail_arm() {
        let mut flapped = authority();
        flapped.ac_shift_frac = 0.04;
        flapped.cm_ac_shift = 0.05;
        flapped.tail_arm_ratio = 0.99;
        let (x, _) = flapped.forward_limit(0.25, -0.5, 0.8).unwrap();
        let tail = 0.8 * 0.99;
        assert!((x - (0.29 + 0.45 / 1.5 - tail * 0.6 / 1.5)).abs() < 1e-12);
    }

    /// Fixed stabiliser, hand computed on the same terms: approach
    /// `C_L,h = 4 (0.10 - 0.05 - 0.20 - 0.50) = -2.60`, flare
    /// `C_L,h = 4.4 (0.12 - 0.03 - 0.20 - 0.50) = -2.684`; the flare governs
    /// at `x = 0.25 + 0.5/2.0 - 0.8 * 2.684 / 2.0`.
    #[test]
    fn a_fixed_stabiliser_trims_with_the_full_elevator_in_both_conditions() {
        let mut fixed = authority();
        fixed.fixed_stabiliser = true;
        let ((xa, ca), (xf, cf)) = fixed.candidates(0.25, -0.5, 0.8).unwrap();
        assert_eq!((ca, cf), (LandingTrimCondition::Approach, LandingTrimCondition::Flare));
        assert!((xa - (0.25 + 0.5 / 1.5 - 0.8 * 2.6 / 1.5)).abs() < 1e-12);
        assert!((xf - (0.25 + 0.5 / 2.0 - 0.8 * 2.684 / 2.0)).abs() < 1e-12);
        let (x, condition) = fixed.forward_limit(0.25, -0.5, 0.8).unwrap();
        assert_eq!((x, condition), (xf, LandingTrimCondition::Flare));
        // Less elevator, less download: the limit moves aft.
        fixed.elevator_term = -0.30;
        assert!(fixed.forward_limit(0.25, -0.5, 0.8).unwrap().0 > xf);
    }

    /// A trimmable stabiliser trims the approach with the elevator neutral,
    /// so its approach cap is the clean section's, below the deflected
    /// elevator's cap of the flare and the stall reach; a fixed stabiliser
    /// trims with the elevator up and keeps the deflected cap.
    #[test]
    #[allow(clippy::expect_used)]
    fn the_neutral_elevator_approach_takes_the_clean_tail_section_cap() {
        use alas_geom::builder::AircraftBuilder;
        for (name, trimmable) in [("A320-200", true), ("ATR72-600", false)] {
            let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
                .expect("a registered preset configures");
            assert_eq!(config.landing_gear.trimmable_stabilizer, trimmable, "{name}");
            let registered = alas_config::presets::get(name).expect("a registered preset");
            let plane = AircraftBuilder::new(Some(config.geometry.clone()))
                .build(Some(&registered.design_vector), true)
                .expect("a registered preset builds");
            let mass = config.mass_model.mlw_fraction_mtow * config.requirements.mtow_kg;
            let authority = landing_authority(&plane, &config, plane.s_ref, mass)
                .expect("a registered preset has a landing authority");
            let tail = plane.wings.get(1).expect("a tail");
            let cl_max = config.performance.cl_max_land;
            let stall = (2.0 * mass * config.requirements.gravity_m_s2
                / (SEA_LEVEL_DENSITY_KG_M3 * plane.s_ref * cl_max))
                .sqrt();
            let (clean, deflected) =
                tail_download_cap(tail, &config, APPROACH_OVER_STALL * stall);
            assert!(clean < deflected, "{name}: {clean} {deflected}");
            let expected = if trimmable { clean } else { deflected };
            assert!(
                (authority.stall_cap_approach - expected).abs() < 1e-12,
                "{name}: {} against {expected}",
                authority.stall_cap_approach
            );
            assert!(authority.stall_cap_flare > clean, "{name}");
        }
    }

    /// A tail that stalls at a small download takes over.
    #[test]
    fn a_small_tail_stall_cap_governs() {
        let mut stalled = authority();
        stalled.stall_cap_approach = 0.1;
        let (_, condition) = stalled.forward_limit(0.25, -0.5, 0.8).unwrap();
        assert_ne!(condition, LandingTrimCondition::Approach);
    }
}
