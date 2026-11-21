// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! One-engine-inoperative yaw balance: the trim state, the minimum control
//! speed and the crosswind rudder demand.
//!
//! Sources (read from the handbooks, Sec. 4 of the source list):
//!
//! - Torenbeek, *Synthesis of Subsonic Airplane Design* (1982), Sec. 9.6.1,
//!   Eqs. (9-52) to (9-60): the thrust asymmetry `N_e = dT_e y_e`, the side
//!   force and yawing moment of the airframe without fin, the weight
//!   component `W sin(phi)` of at most 5 deg of bank, and the fin
//!   `Y_v = q S_v eta_v C_Ya,v (tau_v delta_r - (beta - sigma_v))` acting at the
//!   arm `l_v`; Eq. (9-59) is the moment balance, Fig. 9-22 the relief of the
//!   required fin by sideslip; Eq. (9-64) and (9-65) the fuselage `Cn_beta`;
//!   Sec. 9.6.3 Eq. (9-68) the crosswind check; Sec. 9.6.1(b) a rudder
//!   deflection limit of 25 to 30 deg.
//! - Roskam, *Airplane Design Part VII*, Sec. 2.3, Eqs. (2.10) to (2.12): the
//!   same force and moment equations with the aileron and roll balance, and
//!   the acceptability limits (bank at most 5 deg, `|beta|` below 12 deg
//!   before fin stall).
//! - CS 25.149 (14 CFR 25.149): `VMC` is the speed at which control is held
//!   after the critical engine fails, in straight flight with at most 5 deg of
//!   bank; `VMC` may not exceed 1.13 `VSR`.
//!
//! # Conventions
//!
//! SI units, angles in radians. Sideslip `beta` is positive when the relative
//! wind comes from the live-engine side (the nose is displaced toward the
//! failed engine); the fin then loads in the same sense as a rudder pushing
//! the tail toward the failed side. The rudder angle is positive in that
//! sense. The bank is toward the live engine, so its weight component
//! balances the aerodynamic side force that points toward the failed side.
//!
//! # Model and its limits
//!
//! Quasi-steady moment and side-force balance with linear fin aerodynamics.
//! The side force of the airframe without fin enters the side-force
//! balance (`YawControlGeometry::side_force_without_fin_per_rad`); the
//! roll balance and the aileron (Roskam Eq. 2.11), the nacelles' side force,
//! the induced drag in sideslip, the propeller slipstream over the fin, the pedal force (hinge moment) and the
//! transient overshoot are not modelled. The rudder side force follows
//! Roskam, *Airplane Design* Part VI, Eq. (10.123a), p. 461: the section
//! effectiveness interpolated from the published curve points of Sadraey
//! Fig. 12.12 (see `rudder::RUDDER_TAU_POINTS`), times the three-dimensional
//! ratio of Roskam Fig. 8.53 at the fin's effective aspect ratio, the
//! flap-span factor `K_b` of Fig. 8.52 on the fin panel from the body
//! centreline, and the DATCOM large-deflection factor `k'` (Fig.
//! 6.1.1.1-40, Roskam Fig. 8.13) applied to the deflection. The fin
//! effective aspect ratio is the geometric one unless the caller supplies a
//! larger value.

use std::f64::consts::PI;

use super::failed_engine_drag::YawBalanceGeometry;

mod ground_run;
mod rudder;
pub use ground_run::{
    dry_runway_side_force_coefficient, GroundRunGeometry, GroundRunThrust, VMCG_LATERAL_LIMIT_M,
    VMCG_PILOT_REACTION_S, VMCG_RUDDER_RATE_RAD_S, VMCG_THRUST_DECAY_S,
};
pub use rudder::{
    large_deflection_effectiveness_factor, rudder_effectiveness, rudder_large_deflection_factor,
    span_factor, three_dimensional_effectiveness_ratio,
};

/// CS 25.149(b): the maximum angle of bank toward the live engine, rad.
pub const BANK_LIMIT_RAD: f64 = 5.0 * PI / 180.0;
/// CS 25.149(c): `VMC` may not exceed 1.13 `VSR`.
pub const VMC_CEILING_OVER_VSR: f64 = 1.13;
/// Fin dynamic-pressure ratio `eta_v = (V_v / V)^2`, Torenbeek Eq. (9-60).
/// Source: Sadraey, *Aircraft Design: A Systems Engineering Approach* (Wiley,
/// 2013), book p. 276: the tail efficiency of a conventional tail
/// is 0.85 to 0.95 (1.0 for a T-tail, out of the wing and fuselage wake), and
/// the worked fin examples of Chapter 12 use 0.94 to 0.96 (pp. 705, 744-745).
/// 0.95 is the upper end of the conventional range, kept for a fin partly in
/// the fuselage boundary layer; a lower value is conservative for VMC.
pub const FIN_DYNAMIC_PRESSURE_RATIO: f64 = 0.95;
/// Minimum weathercock stability `Cn_beta` of fuselage plus fin, per rad:
/// positive (Sadraey p. 282; CS 25.177). The model term omits wing and
/// nacelle, so it reads 0.02 to 0.08 per rad on the registered transports
/// (probe: A320 0.053, A340 0.024, ATR72 0.078), below the +0.1 to +0.4 of
/// complete aircraft, which is therefore not usable as a floor here.
pub const MINIMUM_DIRECTIONAL_STABILITY_PER_RAD: f64 = 0.0;
/// Normalising scale of the `Cn_beta` shortfall, per rad: the lower end of the
/// typical band, so a shortfall of this size counts as one unit of violation.
pub const DIRECTIONAL_STABILITY_SCALE_PER_RAD: f64 = 0.10;
/// Floor on the net directional stability `Cn_beta` per rad used by the
/// crosswind check: the lower end of the 0.10 to 0.25 range of transports
/// (Torenbeek Sec. 9.6.2); Sadraey (book p. 282) puts the typical
/// value of `Cn_beta` of stable aircraft at +0.1 to +0.4 per rad. The
/// airframe-without-fin term here is the fuselage alone (Eq. 9-64) and
/// neglects the wing and interference terms, so a small or negative net value
/// is an artefact of the missing terms, not a prediction.
pub const NET_DIRECTIONAL_STABILITY_FLOOR: f64 = 0.10;
/// Speed bounds of the minimum-control-speed search, m/s.
const SEARCH_SPEED_RANGE_M_S: (f64, f64) = (5.0, 300.0);

/// Fin, rudder, wing and airframe data of the yaw balance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct YawControlGeometry {
    /// Engine arm, fin arm and area, effective aspect ratio, rudder area and
    /// quarter-chord sweep.
    pub balance: YawBalanceGeometry,
    /// Fin taper ratio `c_tip / c_root`.
    pub fin_taper_ratio: f64,
    /// Rudder chord over local fin chord.
    pub rudder_chord_ratio: f64,
    /// Flap-span factor `K_b` of the rudder on the fin panel measured from
    /// the fuselage centreline ([`span_factor`], Roskam Part VI Fig. 8.52):
    /// the share of the panel's lift a full-chord rudder deflection over the
    /// rudder span produces. 1 for a rudder over the whole panel.
    pub rudder_span_factor: f64,
    /// Largest rudder deflection, rad.
    pub rudder_max_deflection_rad: f64,
    /// Wing reference area, m^2.
    pub wing_area_m2: f64,
    /// Wing reference span, m.
    pub wing_span_m: f64,
    /// Yaw-moment slope of the airframe without the fin, per rad (negative
    /// when destabilizing), referred to `S b`.
    pub cn_beta_without_fin_per_rad: f64,
    /// Side-force slope of the airframe without the fin, `C_Y_beta` per rad
    /// referred to `S` (negative), Roskam, *Airplane Design* Part VI,
    /// Eqs. (10.25) to (10.27), pp. 383-385: the wing's dihedral term and the
    /// body's `-2 K_i S_0 / S`. It carries part of the bank's weight
    /// component in the engine-out side-force balance.
    pub side_force_without_fin_per_rad: f64,
    /// Sidewash factor `d beta_v / d beta` at the fin: the fin angle of
    /// attack per unit airframe sideslip ([`fin_sidewash_factor`]), times
    /// the DATCOM body factor `k(b_v / 2 r_1)` where a body carries the
    /// fin. It multiplies the sideslip only; the rudder angle reaches the
    /// fin unchanged.
    pub fin_sidewash_factor: f64,
}

/// Sidewash factor `d beta_v / d beta` at the fin from the empirical relation
/// for the product `(d beta_v / d beta) eta_v` (USAF DATCOM, as reproduced by
/// Raymer, *Aircraft Design: A Conceptual Approach*, 6th ed., Eq. (16.51),
/// p. 619): `0.724 + 3.06 (S'_v / S_w) / (1 + cos L) - 0.4 Z_wf / D_f +
/// 0.009 A_w`, divided by [`FIN_DYNAMIC_PRESSURE_RATIO`]. `S'_v` is the fin
/// area extended to the fuselage centreline, `S_w`, `L` and `A_w` the wing
/// reference area, quarter-chord sweep and aspect ratio, `Z_wf` the height of
/// the wing root quarter chord above the fuselage centreline and `D_f` the
/// fuselage depth there. A low wing (`Z_wf < 0`) raises it. `None` for
/// invalid inputs or a non-positive result.
pub fn fin_sidewash_factor(
    fin_area_to_centreline_m2: f64,
    wing_area_m2: f64,
    wing_quarter_chord_sweep_rad: f64,
    wing_aspect_ratio: f64,
    wing_height_above_centreline_m: f64,
    fuselage_depth_m: f64,
) -> Option<f64> {
    let inputs = [
        fin_area_to_centreline_m2,
        wing_area_m2,
        wing_quarter_chord_sweep_rad,
        wing_aspect_ratio,
        wing_height_above_centreline_m,
        fuselage_depth_m,
    ];
    if !(inputs.iter().all(|v| v.is_finite())
        && fin_area_to_centreline_m2 > 0.0
        && wing_area_m2 > 0.0
        && wing_aspect_ratio > 0.0
        && fuselage_depth_m > 0.0)
    {
        return None;
    }
    let product = 0.724
        + 3.06 * (fin_area_to_centreline_m2 / wing_area_m2)
            / (1.0 + wing_quarter_chord_sweep_rad.cos())
        - 0.4 * wing_height_above_centreline_m / fuselage_depth_m
        + 0.009 * wing_aspect_ratio;
    let factor = product / FIN_DYNAMIC_PRESSURE_RATIO;
    (factor > 0.0).then_some(factor)
}

/// What the pilot (or the autopilot) holds while balancing the asymmetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YawTrimMode {
    /// Zero sideslip with the bank that balances the fin side force, when it
    /// is within the limit; otherwise the limit bank and the sideslip that
    /// completes the balance. The state of least fin side force, hence least
    /// trim drag.
    MinimumDrag,
    /// The state of least rudder demand at a bank up to the limit: the
    /// minimum-control-speed condition of CS 25.149(b).
    LeastRudder,
    /// Wings level, zero sideslip, the fin carries the whole moment (the
    /// ground-run balance without tyre side force).
    RudderOnly,
}

/// A solved engine-out trim state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct YawTrim {
    /// Bank toward the live engine, rad (`NaN` when no weight was given).
    pub bank_rad: f64,
    /// Sideslip, positive with the wind from the live-engine side, rad.
    pub sideslip_rad: f64,
    /// Rudder deflection, rad (positive pushes the tail toward the failed side).
    pub rudder_rad: f64,
    /// Fin side force, N.
    pub fin_force_n: f64,
    /// Whether the bank limit, not the balance, set the bank.
    pub bank_limited: bool,
}

/// The factor that limits a minimum control speed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlLimit {
    /// The rudder reaches its maximum deflection.
    Rudder,
}

/// Minimum control speed and the trim state at it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MinimumControlSpeed {
    /// True airspeed, m/s.
    pub tas_m_s: f64,
    /// The trim state at that speed.
    pub trim: YawTrim,
    /// What limits control there.
    pub limit: ControlLimit,
}

/// Why a minimum-control-speed search returned no speed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MinimumControlSpeedFailure {
    /// Density or speed of sound is not a positive finite number.
    UnusableInputs,
    /// The control demand is still at or above its limit at the top of the
    /// search range: the asymmetry cannot be held at any searched speed.
    AboveSearchRange {
        /// Top of the search range, true airspeed, m/s.
        tas_m_s: f64,
    },
    /// The control demand is already below its limit at the bottom of the
    /// search range: the asymmetry sets no minimum control speed there.
    BelowSearchRange {
        /// Bottom of the search range, true airspeed, m/s.
        tas_m_s: f64,
    },
    /// The yaw trim could not be evaluated at a probed speed.
    TrimUnavailable {
        /// Probed true airspeed, m/s.
        tas_m_s: f64,
    },
}

/// Fuselage data of the yaw-moment slope, Torenbeek Fig. 9-25.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FuselageYawGeometry {
    /// Fuselage length, m.
    pub length_m: f64,
    /// Nose to centre of gravity, m.
    pub cg_from_nose_m: f64,
    /// Side projected area, m^2.
    pub side_area_m2: f64,
    /// Maximum height, m.
    pub max_height_m: f64,
    /// Height and width at a quarter of the length from the nose, m.
    pub height_quarter_m: f64,
    /// Width at a quarter of the length from the nose, m.
    pub width_quarter_m: f64,
    /// Height at three quarters of the length, m.
    pub height_three_quarter_m: f64,
    /// Width at three quarters of the length, m.
    pub width_three_quarter_m: f64,
}

/// Fuselage yaw-moment slope `Cn_beta,f` per rad on `S b`, Torenbeek
/// Eqs. (9-64) and (9-65): `-K_beta (S_fs l_f / (S b)) (h_1/h_2)^(1/2)
/// (b_2/b_1)^(1/3)` with `K_beta = 0.3 l_cg/l_f + 0.75 h_max/l_f - 0.105`.
///
/// Eq. (9-65) is stated for `l_f/h_max >= 3.5`; `None` outside that range or
/// for invalid data. The wing, nacelle and interference terms are neglected
/// (Torenbeek: small for a moderately swept wing).
pub fn fuselage_cn_beta_per_rad(
    fuselage: FuselageYawGeometry,
    wing_area_m2: f64,
    wing_span_m: f64,
) -> Option<f64> {
    let f = fuselage;
    let positive = [
        f.length_m,
        f.side_area_m2,
        f.max_height_m,
        f.height_quarter_m,
        f.width_quarter_m,
        f.height_three_quarter_m,
        f.width_three_quarter_m,
        wing_area_m2,
        wing_span_m,
    ]
    .iter()
    .all(|v| v.is_finite() && *v > 0.0);
    if !positive || !f.cg_from_nose_m.is_finite() || f.length_m / f.max_height_m < 3.5 {
        return None;
    }
    let k_beta = 0.3 * f.cg_from_nose_m / f.length_m + 0.75 * f.max_height_m / f.length_m - 0.105;
    let shape = (f.height_quarter_m / f.height_three_quarter_m).sqrt()
        * (f.width_three_quarter_m / f.width_quarter_m).cbrt();
    Some(-k_beta * f.side_area_m2 * f.length_m / (wing_area_m2 * wing_span_m) * shape)
}

/// Lift-curve slope per rad of a fin of effective aspect ratio
/// `aspect_ratio`, DATCOM / Helmbold form with a 2D slope of `2 pi / beta`:
/// `2 pi A / (2 + sqrt(4 + A^2 beta^2 (1 + tan^2(L_c/2) / beta^2)))`.
///
/// `sweep_quarter_rad` is converted to the half-chord sweep with the taper
/// ratio. `None` for invalid inputs or `mach >= 1`.
pub fn fin_lift_curve_slope_per_rad(
    aspect_ratio: f64,
    sweep_quarter_rad: f64,
    taper_ratio: f64,
    mach: f64,
) -> Option<f64> {
    let valid = aspect_ratio.is_finite()
        && aspect_ratio > 0.0
        && sweep_quarter_rad.is_finite()
        && sweep_quarter_rad.abs() < 1.4
        && taper_ratio.is_finite()
        && taper_ratio > 0.0
        && mach.is_finite()
        && (0.0..1.0).contains(&mach);
    if !valid {
        return None;
    }
    let beta_sq = 1.0 - mach * mach;
    let tan_half = sweep_quarter_rad.tan()
        - 4.0 / aspect_ratio * 0.25 * (1.0 - taper_ratio) / (1.0 + taper_ratio);
    let root = 4.0 + aspect_ratio * aspect_ratio * (beta_sq + tan_half * tan_half);
    Some(2.0 * PI * aspect_ratio / (2.0 + root.sqrt()))
}

/// Fin force coefficients at one Mach number.
struct FinAuthority {
    /// `S_v eta_v C_L,alpha`: side force per unit dynamic pressure per rad
    /// of fin angle of attack, m^2.
    area_slope_m2: f64,
    /// Rudder effectiveness on the fin panel at small deflection, Roskam
    /// Part VI Eq. (10.123a): the section value (Sadraey Fig. 12.12) times
    /// the three-dimensional ratio of Fig. 8.53 at the fin's effective
    /// aspect ratio and the flap-span factor `K_b`. The large-deflection
    /// factor `k'` is applied to the deflection itself
    /// ([`YawControlGeometry::rudder_angle_rad`]).
    tau: f64,
}

impl YawControlGeometry {
    fn fin_authority(&self, mach: f64) -> Option<FinAuthority> {
        let g = &self.balance;
        let slope = fin_lift_curve_slope_per_rad(
            g.fin_effective_aspect_ratio,
            g.fin_sweep_rad,
            self.fin_taper_ratio,
            mach,
        )?;
        let section = rudder_effectiveness(self.rudder_chord_ratio)?;
        let three_d = three_dimensional_effectiveness_ratio(g.fin_effective_aspect_ratio, section)?;
        let tau = section * three_d * self.rudder_span_factor;
        Some(FinAuthority {
            area_slope_m2: g.fin_area_m2 * FIN_DYNAMIC_PRESSURE_RATIO * slope,
            tau,
        })
    }

    fn is_usable(&self) -> bool {
        let g = &self.balance;
        [
            g.engine_arm_m,
            g.fin_arm_m,
            g.fin_area_m2,
            self.wing_area_m2,
            self.wing_span_m,
            self.rudder_max_deflection_rad,
            self.rudder_span_factor,
        ]
        .iter()
        .all(|v| v.is_finite() && *v > 0.0)
            && self.cn_beta_without_fin_per_rad.is_finite()
            && self.side_force_without_fin_per_rad.is_finite()
            && self.side_force_without_fin_per_rad <= 0.0
            && self.fin_sidewash_factor.is_finite()
            && self.fin_sidewash_factor > 0.0
    }

    /// The rudder deflection whose fin force equals that of the linear
    /// deflection `linear_rad`: the root of `delta k'(delta) = linear` with
    /// the measured transport-fin large-deflection factor `k'`
    /// ([`rudder_large_deflection_factor`]). `delta k'(delta)` rises
    /// monotonically, and `k' > 0.7`, so the root lies below
    /// `|linear| / 0.5`. Sign follows `linear_rad`.
    pub fn rudder_angle_rad(&self, linear_rad: f64) -> f64 {
        if !linear_rad.is_finite() {
            return linear_rad;
        }
        let target = linear_rad.abs();
        let effective = |delta: f64| {
            delta * rudder_large_deflection_factor(delta.to_degrees(), self.rudder_chord_ratio)
        };
        let (mut low, mut high) = (0.0_f64, target / 0.5);
        for _ in 0..60 {
            let mid = 0.5 * (low + high);
            if effective(mid) < target {
                low = mid;
            } else {
                high = mid;
            }
        }
        0.5 * (low + high) * linear_rad.signum()
    }

    /// Yaw-moment slope of the whole airframe per rad of sideslip, `Cn_beta`
    /// of Torenbeek Eq. (9-62) (jet: no sidewash term). Positive is stable.
    pub fn cn_beta_total_per_rad(&self, mach: f64) -> Option<f64> {
        let authority = self.fin_authority(mach)?;
        Some(
            self.cn_beta_without_fin_per_rad
                + self.fin_sidewash_factor * self.fin_moment_slope(&authority),
        )
    }

    /// Small-deflection rudder yaw-moment derivative `-Cn_delta_r` per rad on
    /// `S b` (positive here): the fin moment slope times the rudder
    /// effectiveness, before the large-deflection factor.
    pub fn cn_delta_r_per_rad(&self, mach: f64) -> Option<f64> {
        let authority = self.fin_authority(mach)?;
        Some(self.fin_moment_slope(&authority) * authority.tau)
    }

    /// `S_v eta_v C_L,alpha l_v / (S b)`: the fin's own contribution to
    /// `Cn_beta`.
    fn fin_moment_slope(&self, authority: &FinAuthority) -> f64 {
        authority.area_slope_m2 * self.balance.fin_arm_m / (self.wing_area_m2 * self.wing_span_m)
    }

    /// Solve the engine-out trim at dynamic pressure `q_pa` and Mach `mach`.
    ///
    /// `asymmetric_force_n` is the live-engine thrust plus the failed
    /// engine's windmilling drag (the moment is that force times the engine
    /// arm), `weight_n` the aircraft weight. Without a weight only
    /// [`YawTrimMode::RudderOnly`] and the zero-sideslip state are
    /// available. `None` for unusable inputs.
    pub fn trim(
        &self,
        asymmetric_force_n: f64,
        q_pa: f64,
        mach: f64,
        weight_n: Option<f64>,
        mode: YawTrimMode,
    ) -> Option<YawTrim> {
        if !(self.is_usable() && asymmetric_force_n.is_finite() && asymmetric_force_n >= 0.0) {
            return None;
        }
        if !(q_pa.is_finite() && q_pa > 0.0) {
            return None;
        }
        let authority = self.fin_authority(mach)?;
        let g = &self.balance;
        let moment_nm = asymmetric_force_n * g.engine_arm_m;
        let fin_force_zero_sideslip = moment_nm / g.fin_arm_m;
        let k_n = q_pa * authority.area_slope_m2;
        let zero_sideslip = |bank_rad: f64| YawTrim {
            bank_rad,
            sideslip_rad: 0.0,
            rudder_rad: self.rudder_angle_rad(fin_force_zero_sideslip / (k_n * authority.tau)),
            fin_force_n: fin_force_zero_sideslip,
            bank_limited: false,
        };
        let weight = weight_n.filter(|w| w.is_finite() && *w > 0.0);
        if mode == YawTrimMode::RudderOnly {
            return Some(zero_sideslip(0.0));
        }
        let Some(weight) = weight else {
            return (mode == YawTrimMode::MinimumDrag).then(|| zero_sideslip(f64::NAN));
        };
        // Side force and yaw moment at a given bank (Roskam Part VII Eqs.
        // 2.10 and 2.12, Torenbeek Eqs. 9-53 to 9-58, aileron and roll left
        // out): with the fin force `F` (toward the failed side) and the
        // sideslip `beta` as unknowns,
        //   F + Y_b beta = W sin(phi)      (side force)
        //   l_v F + N_b beta = N_e          (yaw moment)
        // where `Y_b = -q S C_Y_beta,(A-v) >= 0` is the side force of the
        // airframe without fin per rad of sideslip (toward the failed side
        // for the wind from the live side) and `N_b = q S b Cn_beta,(A-v)`
        // its yawing moment (negative when destabilizing). The sideslip is
        // what the balance leaves, not a chosen value.
        let side_slope_n = -q_pa * self.wing_area_m2 * self.side_force_without_fin_per_rad;
        let moment_slope_nm =
            q_pa * self.wing_area_m2 * self.wing_span_m * self.cn_beta_without_fin_per_rad;
        let determinant = moment_slope_nm - g.fin_arm_m * side_slope_n;
        let banked = |bank_rad: f64| -> Option<YawTrim> {
            if determinant.abs() < 1.0e-9 {
                return None;
            }
            let side_n = weight * bank_rad.sin();
            let fin_force_n = (side_n * moment_slope_nm - side_slope_n * moment_nm) / determinant;
            let sideslip = (moment_nm - g.fin_arm_m * side_n) / determinant;
            let rudder = self.rudder_angle_rad(
                (fin_force_n / k_n - self.fin_sidewash_factor * sideslip) / authority.tau,
            );
            Some(YawTrim {
                bank_rad,
                sideslip_rad: sideslip,
                rudder_rad: rudder,
                fin_force_n,
                bank_limited: bank_rad >= BANK_LIMIT_RAD,
            })
        };
        match mode {
            YawTrimMode::MinimumDrag => {
                let capacity_n = weight * BANK_LIMIT_RAD.sin();
                if fin_force_zero_sideslip <= capacity_n {
                    Some(zero_sideslip((fin_force_zero_sideslip / weight).asin()))
                } else {
                    banked(BANK_LIMIT_RAD)
                }
            }
            YawTrimMode::LeastRudder => {
                // The linear rudder demand is linear in sin(phi), so the
                // least demand over 0 <= phi <= 5 deg lies at an end: the
                // limit bank for an airframe that is directionally stable
                // with its fin, wings level otherwise.
                let level = banked(0.0)?;
                let limit = banked(BANK_LIMIT_RAD)?;
                Some(if limit.rudder_rad <= level.rudder_rad {
                    limit
                } else {
                    level
                })
            }
            YawTrimMode::RudderOnly => None,
        }
    }

    /// Minimum control speed for the thrust asymmetry `asymmetric_force_n`:
    /// the largest true airspeed at which the rudder demand reaches its
    /// maximum deflection.
    ///
    /// `rho_kg_m3` and `speed_of_sound_m_s` give the dynamic pressure and
    /// the Mach number of the fin; `mode` selects the balance (use
    /// [`YawTrimMode::LeastRudder`] for the airborne `VMCA` and
    /// [`YawTrimMode::RudderOnly`] for the ground estimate). `None` for every
    /// cause [`Self::minimum_control_speed_outcome`] tells apart.
    pub fn minimum_control_speed(
        &self,
        asymmetric_force_n: f64,
        rho_kg_m3: f64,
        speed_of_sound_m_s: f64,
        weight_n: Option<f64>,
        mode: YawTrimMode,
    ) -> Option<MinimumControlSpeed> {
        self.minimum_control_speed_outcome(
            asymmetric_force_n,
            rho_kg_m3,
            speed_of_sound_m_s,
            weight_n,
            mode,
        )
        .ok()
    }

    /// [`Self::minimum_control_speed`] with the cause of a missing speed:
    /// not controllable at the top of the search range, controllable down to
    /// its bottom (the asymmetry sets no minimum control speed), a trim that
    /// could not be evaluated, or unusable inputs.
    pub fn minimum_control_speed_outcome(
        &self,
        asymmetric_force_n: f64,
        rho_kg_m3: f64,
        speed_of_sound_m_s: f64,
        weight_n: Option<f64>,
        mode: YawTrimMode,
    ) -> Result<MinimumControlSpeed, MinimumControlSpeedFailure> {
        if !(rho_kg_m3.is_finite() && rho_kg_m3 > 0.0 && speed_of_sound_m_s > 0.0) {
            return Err(MinimumControlSpeedFailure::UnusableInputs);
        }
        let state = |v: f64| -> Result<(YawTrim, f64), MinimumControlSpeedFailure> {
            let q = 0.5 * rho_kg_m3 * v * v;
            let trim = self
                .trim(
                    asymmetric_force_n,
                    q,
                    v / speed_of_sound_m_s,
                    weight_n,
                    mode,
                )
                .ok_or(MinimumControlSpeedFailure::TrimUnavailable { tas_m_s: v })?;
            Ok((trim, demand(&trim, self)))
        };
        let (mut low, mut high) = SEARCH_SPEED_RANGE_M_S;
        // Demand falls with speed: at `high` it must be below one, at `low`
        // at least one.
        if state(high)?.1 >= 1.0 {
            return Err(MinimumControlSpeedFailure::AboveSearchRange { tas_m_s: high });
        }
        if state(low)?.1 < 1.0 {
            return Err(MinimumControlSpeedFailure::BelowSearchRange { tas_m_s: low });
        }
        for _ in 0..80 {
            let mid = 0.5 * (low + high);
            if state(mid)?.1 >= 1.0 {
                low = mid;
            } else {
                high = mid;
            }
        }
        let (trim, _) = state(low)?;
        Ok(MinimumControlSpeed {
            tas_m_s: low,
            trim,
            limit: ControlLimit::Rudder,
        })
    }

    /// Rudder deflection that holds a steady sideslip `sideslip_rad` with no
    /// thrust asymmetry (the crosswind balance of Torenbeek Eq. 9-68): the
    /// net airframe yaw stiffness over the rudder moment slope, so the
    /// dynamic pressure cancels. The net stiffness is floored at
    /// [`NET_DIRECTIONAL_STABILITY_FLOOR`]. Positive opposes the sideslip.
    /// `None` for unusable inputs.
    pub fn crosswind_rudder_rad(&self, sideslip_rad: f64, mach: f64) -> Option<f64> {
        if !(self.is_usable() && sideslip_rad.is_finite()) {
            return None;
        }
        let authority = self.fin_authority(mach)?;
        let fin_slope = self.fin_moment_slope(&authority);
        let net = (self.cn_beta_without_fin_per_rad + self.fin_sidewash_factor * fin_slope)
            .max(NET_DIRECTIONAL_STABILITY_FLOOR);
        Some(self.rudder_angle_rad(net * sideslip_rad / (fin_slope * authority.tau)))
    }
}

/// Control demand of a trim state: the rudder deflection over its stop; at
/// or above one the aircraft is at its limit. CS 25.149 bounds the bank,
/// not the sideslip, and Roskam Part VII Sec. 2.3.3 (step 6, p. 28) limits
/// the sideslip of the state through its drag in the climb requirement, not
/// through a fixed angle, so the sideslip is an output of the balance.
fn demand(trim: &YawTrim, geometry: &YawControlGeometry) -> f64 {
    trim.rudder_rad.max(0.0) / geometry.rudder_max_deflection_rad
}

#[cfg(test)]
mod tests;
